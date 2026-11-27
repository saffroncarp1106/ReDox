use crate::app::ReDox;
use eframe::egui;

impl ReDox {
    pub(in crate::app) fn ui_drop_hint(&self, context: &egui::Context) {
        if context.input(|input| !input.raw.hovered_files.is_empty()) {
            egui::Area::new(egui::Id::new("ui_config_drop_hint"))
                .order(egui::Order::Tooltip)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(context, |ui| {
                    egui::Frame::popup(ui.style())
                        .inner_margin(24)
                        .show(ui, |ui| {
                            ui.heading(if self.skin_editor.is_some() {
                                "Drop a background picture or UI pack"
                            } else {
                                "Drop UI config or pack folder"
                            });
                            ui.label("Import it, then choose it under Settings > Interface.");
                        });
                });
        }
    }
}
