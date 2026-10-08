//! Interface text. English is the only language bundled: the other
//! catalogs were left out to keep the binary and its memory small. The
//! gettext calls stay, so a catalog can be added back in `assets/i18n`.

pub use fastframe_i18n::{gettext, ngettext, pgettext};

include!(concat!(env!("OUT_DIR"), "/catalogs.rs"));

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Locale {
    #[default]
    #[value(name = "en", alias = "en-US")]
    English,
}

impl fastframe_i18n::Locale for Locale {
    fn catalog(self) -> Option<&'static dyn fastframe_i18n::Translator> {
        match self {
            Self::English => None,
        }
    }
}

impl Locale {
    /// The tag this locale is named by on the command line and in
    /// `settings.json`. [`Self::from_tag`] reads each one back.
    pub fn tag(self) -> &'static str {
        match self {
            Self::English => "en",
        }
    }

    /// The locale a stored or typed tag names exactly, as [`Self::tag`]
    /// writes it or by one of its command-line aliases.
    pub fn from_tag(tag: &str) -> Option<Self> {
        <Self as clap::ValueEnum>::from_str(tag, true).ok()
    }

    /// The language's name in that language.
    pub fn native_name(self) -> &'static str {
        match self {
            Self::English => "English",
        }
    }

    /// The interface language whatever the operating system is read in:
    /// English is the only one bundled.
    pub fn from_system() -> Self {
        Self::English
    }

    /// The catalog closest to a language tag, BCP 47 (`en-GB`) or POSIX
    /// (`en_US.UTF-8`), or `None` when no catalog speaks it.
    pub fn from_language_tag(tag: &str) -> Option<Self> {
        fastframe_i18n::LanguageTag::parse(tag).and_then(|tag| Self::from_parsed_tag(&tag))
    }

    fn from_parsed_tag(tag: &fastframe_i18n::LanguageTag) -> Option<Self> {
        match tag.language.as_str() {
            "en" => Some(Self::English),
            _ => None,
        }
    }

    pub fn liked_song_count(self, count: u32) -> String {
        ngettext(
            self,
            // Translators: Keep {count} exactly as written. It becomes the number of liked songs.
            "Playlist • {count} song",
            "Playlist • {count} songs",
            count,
        )
        .replace("{count}", &count.to_string())
    }

    pub fn song_count(self, count: u32) -> String {
        ngettext(
            self,
            // Translators: Keep {count} exactly as written. It becomes a number of songs.
            "{count} song",
            "{count} songs",
            count,
        )
        .replace("{count}", &count.to_string())
    }

    pub fn playlist_count(self, count: u32) -> String {
        ngettext(
            self,
            // Translators: Keep {count} exactly as written. It becomes a number of playlists.
            "{count} playlist",
            "{count} playlists",
            count,
        )
        .replace("{count}", &count.to_string())
    }

    pub fn folder_playlist_count(self, count: u32) -> String {
        ngettext(
            self,
            // Translators: Keep {count} exactly as written. It becomes the number of playlists a folder holds.
            "Folder • {count} playlist",
            "Folder • {count} playlists",
            count,
        )
        .replace("{count}", &count.to_string())
    }

    pub fn folder_state_label(self, name: &str, collapsed: bool) -> String {
        // Translators: Keep {name} exactly as written. It becomes the folder name.
        let label = if collapsed {
            gettext(self, "{name}, folder, collapsed")
        } else {
            gettext(self, "{name}, folder, expanded")
        };
        label.replace("{name}", name)
    }
}

/// Every bundled language, in the order the Settings picker lists them.
pub const LOCALES: &[Locale] = &[Locale::English];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_use_the_english_source() {
        assert_eq!(gettext(Locale::English, "Home"), "Home");
        assert_eq!(pgettext(Locale::English, "lyrics", "Follow"), "Follow");
        assert_eq!(Locale::default(), Locale::English);
    }

    #[test]
    fn every_system_language_reads_english() {
        assert_eq!(Locale::from_system(), Locale::English);
        for tag in ["en-GB", "en_US.UTF-8", "en"] {
            assert_eq!(
                Locale::from_language_tag(tag),
                Some(Locale::English),
                "{tag}"
            );
        }
        for tag in ["de-DE", "es", "ja-JP", "zh-Hant", "C", "", "und"] {
            assert_eq!(Locale::from_language_tag(tag), None, "{tag}");
        }
    }

    #[test]
    fn every_locale_round_trips_through_its_tag_and_is_listed_once() {
        for &locale in LOCALES {
            assert_eq!(Locale::from_tag(locale.tag()), Some(locale));
            assert_eq!(Locale::from_language_tag(locale.tag()), Some(locale));
            assert!(!locale.native_name().is_empty());
        }
        assert_eq!(
            LOCALES.len(),
            <Locale as clap::ValueEnum>::value_variants().len()
        );
        assert_eq!(Locale::from_tag("en-US"), Some(Locale::English));
        assert_eq!(Locale::from_tag("de"), None);
    }

    #[test]
    fn zero_one_and_many_songs_have_complete_labels() {
        for (count, english) in [
            (0, "Playlist • 0 songs"),
            (1, "Playlist • 1 song"),
            (2, "Playlist • 2 songs"),
            (100_000, "Playlist • 100000 songs"),
        ] {
            assert_eq!(Locale::English.liked_song_count(count), english);
        }
        assert_eq!(Locale::English.song_count(1), "1 song");
        assert_eq!(Locale::English.playlist_count(2), "2 playlists");
        assert_eq!(
            Locale::English.folder_playlist_count(1),
            "Folder • 1 playlist"
        );
        assert_eq!(
            Locale::English.folder_playlist_count(2),
            "Folder • 2 playlists"
        );
        assert_eq!(
            Locale::English.folder_state_label("Road trips", true),
            "Road trips, folder, collapsed"
        );
        assert_eq!(
            Locale::English.folder_state_label("Road trips", false),
            "Road trips, folder, expanded"
        );
    }
}
