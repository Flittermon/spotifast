//! Recommended songs for playlists, and Smart Shuffle.
//!
//! Both come from the playlist's radio: Spotify's context resolver mixes
//! about fifty songs that go with the playlist, the same station a radio
//! page shows. Songs the playlist already has are left out, so what
//! remains is new to it.
//!
//! A playlist page lists the first few under "Recommended songs", each
//! with an Add button, like Spotify's own apps. Refresh shows the next few
//! and asks Spotify for a new mix once the current one runs out.
//!
//! Smart Shuffle is a third Shuffle state. While it is on and a playlist
//! plays, every time [`SMART_SHUFFLE_EVERY`] of the playlist's songs have
//! started, one recommended song is added to the queue so it plays next.
//! It goes through the ordinary queue, so it works on this computer and
//! on other Connect devices alike.

use super::*;

/// How many recommended songs a playlist page shows at once.
pub const RECOMMENDATIONS_SHOWN: usize = 10;

/// A song's title and first artist, ignoring case: another release of a
/// song the playlist has (a single, a remaster, a compilation) counts as
/// the same song.
pub(crate) fn title_key(name: &str, artist: Option<&str>) -> String {
    format!(
        "title:{}|{}",
        name.trim().to_lowercase(),
        artist.unwrap_or("").trim().to_lowercase()
    )
}

fn item_title_key(item: &PlayableItem) -> String {
    match item {
        PlayableItem::Track(track) => title_key(
            &track.name,
            track.artists.first().map(|artist| artist.name.as_str()),
        ),
        PlayableItem::Episode(episode) => title_key(&episode.name, None),
    }
}

fn track_title_key(track: &Track) -> String {
    title_key(
        &track.name,
        track.artists.first().map(|artist| artist.name.as_str()),
    )
}

/// Identifies the rows a set of member keys was built from, so the keys
/// are only rebuilt when the playlist's loaded rows change.
fn members_stamp(page: &PlaylistPage) -> (u64, usize, usize, usize) {
    (
        page.items.revision,
        page.items.items.len(),
        page.items.windows.values().map(Vec::len).sum(),
        page.local_additions.len(),
    )
}

/// The URIs and title keys of every song of the playlist that is known:
/// its loaded rows and songs just added to it.
pub(crate) fn playlist_member_keys(page: &PlaylistPage) -> HashSet<String> {
    let mut keys = HashSet::new();
    let rows = page
        .items
        .items
        .iter()
        .chain(page.items.windows.values().flatten());
    for item in rows.filter_map(PlaylistItem::playable) {
        if !item.uri().is_empty() {
            keys.insert(item.uri().to_string());
        }
        keys.insert(item_title_key(item));
    }
    keys.extend(page.local_additions.iter().cloned());
    keys
}

/// Brings the remembered member keys up to date with the playlist's rows.
fn refresh_members(recommendations: &mut Recommendations, page: Option<&PlaylistPage>) {
    let Some(page) = page else {
        return;
    };
    let stamp = members_stamp(page);
    if recommendations.members_stamp != Some(stamp) {
        recommendations.members = playlist_member_keys(page);
        recommendations.members_stamp = Some(stamp);
    }
}

/// The mix's songs that the playlist does not have, in the mix's order,
/// each song once.
pub(crate) fn candidates(recommendations: &Recommendations) -> Vec<&Track> {
    let Some(songs) = recommendations.songs.get() else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    songs
        .iter()
        .filter(|track| track.uri.starts_with("spotify:track:"))
        .filter(|track| track.is_playable != Some(false))
        .filter(|track| {
            !recommendations.members.contains(&track.uri)
                && !recommendations.members.contains(&track_title_key(track))
        })
        .filter(|track| seen.insert(track_title_key(track)))
        .collect()
}

/// The window of `count` candidates starting at `offset`, starting over
/// from the top when the offset has run past the end.
pub(crate) fn shown_window(candidates: &[&Track], offset: usize, count: usize) -> Vec<Track> {
    let start = if offset < candidates.len() { offset } else { 0 };
    candidates
        .iter()
        .skip(start)
        .take(count)
        .map(|track| (*track).clone())
        .collect()
}

impl App {
    /// Whether Smart Shuffle is selected and Shuffle is on.
    pub fn smart_shuffle_on(&self) -> bool {
        self.settings.smart_shuffle && self.shuffle_wanted
    }

    /// Whether `uri` is a song Smart Shuffle added to the queue.
    pub fn smart_shuffle_added(&self, uri: &str) -> bool {
        self.smart_shuffle.added.contains(uri)
    }

    /// The playing playlist, the only kind of collection Smart Shuffle
    /// adds songs to.
    pub(super) fn smart_shuffle_playlist(&self) -> Option<String> {
        self.playing_context_uri()
            .filter(|uri| uri.starts_with("spotify:playlist:"))
    }

    /// The recommendations' loading state for a playlist, by its URI.
    pub fn recommendations_state(&self, playlist_uri: &str) -> Option<&Loadable<Vec<Track>>> {
        self.recommendations
            .get(playlist_uri)
            .map(|recommendations| &recommendations.songs)
    }

    /// Whether a new mix is on its way while the old one stays shown.
    pub fn recommendations_refreshing(&self, playlist_uri: &str) -> bool {
        self.recommendations
            .get(playlist_uri)
            .is_some_and(|recommendations| recommendations.refreshing)
    }

    /// The songs a playlist page lists under "Recommended songs". Asks
    /// for the playlist's mix the first time.
    pub fn recommended_songs(&mut self, playlist_uri: &str, page: &PlaylistPage) -> Vec<Track> {
        self.load_recommendations(playlist_uri);
        let Some(recommendations) = self.recommendations.get_mut(playlist_uri) else {
            return Vec::new();
        };
        refresh_members(recommendations, Some(page));
        let candidates = candidates(recommendations);
        shown_window(&candidates, recommendations.offset, RECOMMENDATIONS_SHOWN)
    }

    /// Asks for a playlist's mix unless it is here or on its way.
    pub(super) fn load_recommendations(&mut self, playlist_uri: &str) {
        if util::uri_kind(playlist_uri) != Some("playlist")
            || util::station_uri(playlist_uri).is_none()
        {
            return;
        }
        let recommendations = self
            .recommendations
            .entry(playlist_uri.to_string())
            .or_default();
        // A failed mix waits for Refresh rather than asking every frame.
        if !matches!(recommendations.songs, Loadable::NotLoaded) {
            return;
        }
        self.load_generation = self.load_generation.wrapping_add(1);
        recommendations.generation = self.load_generation;
        recommendations.songs = Loadable::Loading;
        self.backend.send(Command::Radio {
            seed: playlist_uri.to_string(),
            generation: recommendations.generation,
        });
    }

    /// Asks Spotify for a new mix, keeping the current songs on screen
    /// until it arrives.
    pub(super) fn request_new_mix(&mut self, playlist_uri: &str) {
        let Some(recommendations) = self.recommendations.get_mut(playlist_uri) else {
            self.load_recommendations(playlist_uri);
            return;
        };
        let loading = matches!(recommendations.songs, Loadable::Loading);
        let loaded = recommendations.songs.get().is_some();
        if loading || (loaded && recommendations.refreshing) {
            return;
        }
        if !loaded {
            recommendations.songs = Loadable::NotLoaded;
            self.load_recommendations(playlist_uri);
            return;
        }
        self.load_generation = self.load_generation.wrapping_add(1);
        recommendations.generation = self.load_generation;
        recommendations.refreshing = true;
        self.backend.send(Command::Radio {
            seed: playlist_uri.to_string(),
            generation: recommendations.generation,
        });
    }

    /// Refresh under "Recommended songs": the next few songs of the mix,
    /// or a new mix once this one has been shown through.
    pub(super) fn refresh_recommendations(&mut self, playlist_uri: &str) {
        let page = util::uri_id(playlist_uri).and_then(|id| self.playlist_pages.get(id));
        let Some(recommendations) = self.recommendations.get_mut(playlist_uri) else {
            self.load_recommendations(playlist_uri);
            return;
        };
        // Refresh while the first mix is still on its way asks again, in
        // case that request was lost; the older answer is then ignored.
        if matches!(recommendations.songs, Loadable::Loading) {
            recommendations.songs = Loadable::NotLoaded;
            self.load_recommendations(playlist_uri);
            return;
        }
        refresh_members(recommendations, page);
        let remaining = candidates(recommendations).len();
        let next = recommendations.offset + RECOMMENDATIONS_SHOWN;
        if recommendations.songs.get().is_some() && next < remaining {
            recommendations.offset = next;
            return;
        }
        self.request_new_mix(playlist_uri);
    }

    /// Takes the answer to a recommendations request. False when the
    /// answer belongs to something else, such as a radio page.
    pub(crate) fn receive_recommendations(
        &mut self,
        seed: &str,
        generation: u64,
        result: &Result<Vec<Track>, String>,
    ) -> bool {
        let Some(recommendations) = self
            .recommendations
            .get_mut(seed)
            .filter(|recommendations| recommendations.generation == generation)
        else {
            return false;
        };
        let refreshing = std::mem::take(&mut recommendations.refreshing);
        match result {
            Ok(songs) => {
                recommendations.songs = Loadable::Loaded(songs.clone());
                recommendations.offset = 0;
                for track in songs {
                    if let Some(id) = &track.id {
                        self.track_cache
                            .entry(id.clone())
                            .or_insert_with(|| track.clone());
                    }
                }
            }
            // A failed refresh keeps the songs on screen.
            Err(_) if refreshing => {}
            Err(error) => recommendations.songs = Loadable::Failed(error.clone()),
        }
        self.smart_shuffle_catch_up();
        true
    }

    /// Turns Smart Shuffle on or off. On also turns Shuffle on; off
    /// leaves plain Shuffle as it is.
    pub(super) fn set_smart_shuffle(&mut self, on: bool) {
        if self.settings.smart_shuffle != on {
            self.settings.smart_shuffle = on;
            self.mark_settings_dirty();
        }
        self.smart_shuffle.since_last = 0;
        self.smart_shuffle.pending.clear();
        self.smart_shuffle.playlist = None;
        if !on {
            return;
        }
        if !self.shuffle_wanted {
            self.set_shuffle(true);
        }
        if let Some(playlist) = self.smart_shuffle_playlist() {
            self.smart_shuffle.playlist = Some(playlist.clone());
            self.prepare_smart_shuffle(&playlist);
        }
        self.toast(gettext(
            self.locale,
            "Smart Shuffle on: a recommended song plays after every 3 songs",
        ));
    }

    /// Gets the playlist's mix, and its rows so songs it has are left out.
    fn prepare_smart_shuffle(&mut self, playlist_uri: &str) {
        self.load_recommendations(playlist_uri);
        if let Some(id) = util::uri_id(playlist_uri)
            && !self.playlist_pages.contains_key(id)
        {
            self.ensure_loaded(Page::Playlist(id.to_string()));
        }
    }

    /// Counts a song that started. `from_manual_queue` is true when it
    /// was the head of the songs queued by hand, which are not the
    /// playlist's own.
    pub(super) fn smart_shuffle_song_started(&mut self, uri: &str, from_manual_queue: bool) {
        if let Some(at) = self
            .smart_shuffle
            .pending
            .iter()
            .position(|pending| pending == uri)
        {
            self.smart_shuffle.pending.remove(at);
            self.smart_shuffle.since_last = 0;
            return;
        }
        if !self.smart_shuffle_on() {
            return;
        }
        let Some(playlist) = self.smart_shuffle_playlist() else {
            return;
        };
        if self.smart_shuffle.playlist.as_deref() != Some(playlist.as_str()) {
            self.smart_shuffle.playlist = Some(playlist.clone());
            self.smart_shuffle.since_last = 0;
            self.smart_shuffle.pending.clear();
            self.prepare_smart_shuffle(&playlist);
        }
        if from_manual_queue {
            return;
        }
        self.smart_shuffle.since_last = self.smart_shuffle.since_last.saturating_add(1);
        // A queued recommendation that never started, because it was
        // skipped past or the queue was cleared, stops holding up the next.
        if !self.smart_shuffle.pending.is_empty()
            && self.smart_shuffle.since_last >= 2 * SMART_SHUFFLE_EVERY
        {
            self.smart_shuffle.pending.clear();
        }
        self.smart_shuffle_catch_up();
    }

    /// Queues a recommendation once enough of the playlist has played
    /// since the last one, as soon as the mix is here.
    pub(super) fn smart_shuffle_catch_up(&mut self) {
        if !self.smart_shuffle_on()
            || !self.smart_shuffle.pending.is_empty()
            || self.smart_shuffle.since_last < SMART_SHUFFLE_EVERY
        {
            return;
        }
        let Some(playlist) = self.smart_shuffle.playlist.clone() else {
            return;
        };
        if self.smart_shuffle_playlist().as_deref() != Some(playlist.as_str()) {
            return;
        }
        let page = util::uri_id(&playlist).and_then(|id| self.playlist_pages.get(id));
        let Some(recommendations) = self.recommendations.get_mut(&playlist) else {
            self.load_recommendations(&playlist);
            return;
        };
        refresh_members(recommendations, page);
        let fresh_songs = |recommendations: &Recommendations| -> Vec<Track> {
            candidates(recommendations)
                .into_iter()
                .filter(|track| !recommendations.used.contains(&track.uri))
                .cloned()
                .collect()
        };
        let mut fresh = fresh_songs(recommendations);
        let settled = recommendations.songs.get().is_some() && !recommendations.refreshing;
        if fresh.is_empty() && settled && recommendations.exhausted {
            // A new mix brought nothing that was not offered already:
            // offer its songs again rather than none.
            recommendations.used.clear();
            fresh = fresh_songs(recommendations);
        }
        let Some(track) = fresh.first().cloned() else {
            // Everything in this mix has been offered: ask for another,
            // and queue from it when it arrives.
            if settled {
                recommendations.exhausted = true;
            }
            self.request_new_mix(&playlist);
            return;
        };
        recommendations.exhausted = false;
        recommendations.used.insert(track.uri.clone());
        // Fetch the next mix before this one runs out.
        if fresh.len() <= SMART_SHUFFLE_EVERY as usize {
            self.request_new_mix(&playlist);
        }
        self.smart_shuffle.pending.push(track.uri.clone());
        self.smart_shuffle.added.insert(track.uri.clone());
        self.queue_one(track.uri.clone(), track.name.clone(), false);
    }
}
