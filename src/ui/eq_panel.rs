//! The equalizer as a small card at the bottom right, opened from the
//! player bar, so the sound can be shaped without leaving the page.
//!
//! It draws the same controls as the equalizer in Settings and sends the
//! same actions; nothing is loaded or allocated until it is opened.

use egui::{Align, Layout, Rect, pos2};

use crate::app::App;
use crate::i18n::{gettext, pgettext};
use crate::model::Action;
use crate::theme::{self, Icon};

use super::widgets;

/// Where the player bar's equalizer button was drawn, for placing the card.
pub const BUTTON_RECT_ID: &str = "eq-button-rect";
/// The card's content width: the preamp and ten band sliders side by side.
const WIDTH: f32 = 440.0;

pub fn popup(app: &mut App, ctx: &egui::Context) {
    if !app.show_eq_panel {
        return;
    }
    let palette = app.palette;
    let locale = app.locale;
    let screen = ctx.content_rect();
    let top_of_bar = ctx
        .data(|data| data.get_temp::<Rect>(egui::Id::new(BUTTON_RECT_ID)))
        .map_or(screen.bottom() - theme::PLAYER_BAR_HEIGHT, |button| {
            button.top()
        });
    let position = pos2(screen.right() - 12.0, (top_of_bar - 12.0).max(8.0));
    egui::Area::new(egui::Id::new("eq-panel"))
        .order(egui::Order::Foreground)
        .fixed_pos(position)
        .pivot(egui::Align2::RIGHT_BOTTOM)
        .show(ctx, |ui| {
            widgets::menu_frame(&palette).show(ui, |ui| {
                ui.set_width(WIDTH);
                let equalizer = gettext(locale, "Equalizer");
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    theme::text(ui, equalizer.as_ref(), theme::bold(16.0), palette.text);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if theme::icon_button(
                            ui,
                            Icon::X,
                            15.0,
                            palette.secondary,
                            palette.text,
                            &gettext(locale, "Close"),
                        )
                        .clicked()
                        {
                            app.actions.push(Action::ToggleEqPanel);
                        }
                        let mut on = app.settings.eq_on;
                        if widgets::switch(ui, &palette, &equalizer, &mut on).changed() {
                            app.actions.push(Action::ToggleEq);
                        }
                        preset_menu(app, ui);
                    });
                });
                // Only this computer's playback runs through the equalizer.
                if !matches!(app.target(), crate::app::Target::Local) {
                    ui.add_space(2.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(gettext(
                                locale,
                                "A ten-band equalizer for playback on this computer. It does not affect other devices.",
                            ))
                            .font(theme::regular(12.5))
                            .color(palette.secondary),
                        )
                        .wrap(),
                    );
                }
                ui.add_space(8.0);
                super::settings::eq_curve(ui, &palette, &crate::app::eq_settings(&app.settings));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    let on = app.settings.eq_on;
                    let mut preamp = app.settings.eq_preamp_db;
                    if super::settings::eq_slider(
                        ui,
                        &palette,
                        &pgettext(locale, "equalizer", "Pre"),
                        &mut preamp,
                        on,
                    ) {
                        app.actions.push(Action::SetEqPreamp(preamp));
                    }
                    for (band, hz) in crate::eq::BANDS.iter().enumerate() {
                        let mut gain = app.settings.eq_bands_db[band];
                        if super::settings::eq_slider(
                            ui,
                            &palette,
                            &super::settings::hertz(*hz),
                            &mut gain,
                            on,
                        ) {
                            app.actions.push(Action::SetEqBand(band, gain));
                        }
                    }
                });
            });
        });
}

/// The preset in use, or "Presets", opening the list of presets.
fn preset_menu(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let current = crate::eq::PRESETS
        .iter()
        .position(|preset| preset.bands_db == app.settings.eq_bands_db);
    let label = current.map_or_else(
        || gettext(app.locale, "Presets").into_owned(),
        |index| crate::eq::PRESETS[index].name.to_string(),
    );
    let button = theme::soft_button(ui, &palette, Some(Icon::ChevronDown), &label, false);
    egui::Popup::menu(&button)
        .frame(widgets::menu_frame(&palette))
        .show(|ui| {
            ui.set_min_width(180.0);
            egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| {
                    for (index, preset) in crate::eq::PRESETS.iter().enumerate() {
                        let icon = (current == Some(index)).then_some(Icon::Check);
                        if widgets::menu_item(ui, &palette, icon, preset.name) {
                            app.actions.push(Action::ApplyEqPreset(index));
                        }
                    }
                });
        });
}
