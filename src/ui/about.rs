use eframe::egui::{self, vec2};

use super::{theme, win98};
use crate::app::ReDox;

impl ReDox {
    pub(super) fn about_settings(&mut self, ui: &mut egui::Ui) {
        let context = ui.ctx().clone();
        let texture_id = egui::Id::new("about_redox_logo");
        let cached = context.data(|data| data.get_temp::<egui::TextureHandle>(texture_id));
        let logo = cached.unwrap_or_else(|| {
            let icon = Self::app_icon();
            let texture = context.load_texture(
                "ReDox logo",
                egui::ColorImage::from_rgba_unmultiplied(
                    [icon.width as usize, icon.height as usize],
                    &icon.rgba,
                ),
                egui::TextureOptions::LINEAR,
            );
            context.data_mut(|data| data.insert_temp(texture_id, texture.clone()));
            texture
        });
        ui.horizontal(|ui| {
            let logo = egui::Image::new(&logo).fit_to_exact_size(vec2(48.0, 48.0));
            ui.add(egui::Button::image(logo).frame(false))
                .on_hover_text("ReDox logo");
            ui.vertical(|ui| {
                ui.heading("ReDox");
                ui.label(concat!("Version ", env!("CARGO_PKG_VERSION")));
            });
        });
        ui.label("A native Rust client for Codex and Ollama.");
        ui.add_space(12.0);
        ui.separator();
        ui.heading("Discord Rich Presence");
        ui.label(
            "Share the custom text below on your Discord profile. Discord desktop must be running.",
        );
        let settings = &mut self.ui_preferences.discord_presence;
        let label = if settings.enabled {
            "Disable Presence"
        } else {
            "Enable Presence"
        };
        let classic = theme::is_classic(ui);
        let button = ui.add_sized([140.0, 26.0], egui::Button::new(label));
        if classic {
            win98::bevel(
                ui.painter(),
                button.rect,
                button.is_pointer_button_down_on(),
            );
        }
        if button.clicked() {
            settings.enabled = !settings.enabled;
        }
        ui.label("Details (first line)");
        ui.add(
            egui::TextEdit::singleline(&mut settings.details)
                .char_limit(128)
                .desired_width(f32::INFINITY),
        );
        ui.label("State (second line)");
        ui.add(
            egui::TextEdit::singleline(&mut settings.state)
                .char_limit(128)
                .desired_width(f32::INFINITY),
        );
        ui.label(
            "Changes save automatically and update while enabled. Blank text uses ReDox / Vibing.",
        );
        self.presence.sync(settings);
        ui.label(self.presence.status());
        if settings.enabled {
            context.request_repaint_after(std::time::Duration::from_secs(1));
        }
    }
}

#[cfg(test)]
#[path = "../../tests/ui/about.rs"]
mod tests;
