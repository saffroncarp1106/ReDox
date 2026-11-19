use eframe::egui::{self, Color32, Layout, RichText, Stroke, vec2};
use redox::domain::{Chat, Message};
use uuid::Uuid;

use super::{
    icons::{self, Icon},
    theme,
};
use crate::app::MessageEdit;

pub(super) enum Action {
    Edit(Uuid),
    Cancel,
    Send,
    Switch(Uuid, bool),
}

pub(super) struct Controls<'a> {
    pub edit: &'a mut Option<MessageEdit>,
    pub enabled: bool,
    pub can_send: bool,
    pub follow_latest: bool,
    pub action: Option<Action>,
}

impl Controls<'_> {
    pub fn editing(&mut self, ui: &mut egui::Ui, chat: &Chat, message: &Message) -> bool {
        let Some(edit) = self
            .edit
            .as_mut()
            .filter(|edit| edit.chat == chat.id && edit.message == message.id)
        else {
            return false;
        };
        if let Some(action) = editor(ui, edit, message, self.can_send && !chat.archived) {
            self.action = Some(action);
        }
        true
    }

    pub fn row_actions(&mut self, ui: &mut egui::Ui, chat: &Chat, message: &Message) {
        if !message.is_editable_prompt() {
            return;
        }
        let enabled = self.enabled && self.edit.is_none();
        let versions = chat.prompt_version(message.id);
        if chat.archived && versions.is_none() {
            return;
        }
        ui.allocate_ui_with_layout(
            vec2(ui.available_width(), 30.0),
            Layout::right_to_left(egui::Align::Center),
            |ui| {
                if !chat.archived && icon(ui, Icon::Edit, "Edit message", enabled).clicked() {
                    self.action = Some(Action::Edit(message.id));
                }
                if let Some((version, total)) = versions {
                    if icon(
                        ui,
                        Icon::Forward,
                        "Next version",
                        enabled && version < total,
                    )
                    .clicked()
                    {
                        self.action = Some(Action::Switch(message.id, false));
                    }
                    ui.label(
                        RichText::new(format!("{version}/{total}"))
                            .small()
                            .color(theme::muted()),
                    );
                    if icon(ui, Icon::Back, "Previous version", enabled && version > 1).clicked() {
                        self.action = Some(Action::Switch(message.id, true));
                    }
                }
            },
        );
    }
}

fn icon(ui: &mut egui::Ui, icon: Icon, label: &str, enabled: bool) -> egui::Response {
    let response = ui
        .add_enabled_ui(enabled, |ui| icons::button(ui, icon, label))
        .inner;
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    response
}

fn editor(
    ui: &mut egui::Ui,
    state: &mut MessageEdit,
    message: &Message,
    can_send: bool,
) -> Option<Action> {
    let classic = theme::is_classic(ui);
    let width = (ui.available_width() - 22.0).clamp(1.0, 680.0);
    let waiting = state.waiting();
    let focus = std::mem::take(&mut state.focus);
    ui.with_layout(Layout::top_down(egui::Align::Max), |ui| {
        let editor = egui::Frame::new()
            .fill(theme::input_background())
            .stroke(Stroke::new(1.0, theme::border()))
            .corner_radius(if classic { 0 } else { 13 })
            .inner_margin(11)
            .show(ui, |ui| {
                ui.set_width(width);
                if !message.attachments.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        for file in &message.attachments {
                            let removed = state.removed_attachments.contains(&file.id);
                            ui.push_id(file.id, |ui| {
                                let label = if removed {
                                    format!("Restore {}", file.name())
                                } else {
                                    format!("{}  ×", file.name())
                                };
                                if ui
                                    .add_enabled(!waiting, egui::Button::new(label).truncate())
                                    .clicked()
                                {
                                    if removed {
                                        state.removed_attachments.retain(|id| *id != file.id);
                                    } else {
                                        state.removed_attachments.push(file.id);
                                    }
                                    state.error = None;
                                }
                            });
                        }
                    });
                }
                let id = ui.make_persistent_id(("edit_text", message.id));
                state.input_id = Some(id);
                if focus {
                    ui.memory_mut(|memory| memory.request_focus(id));
                }
                let keyboard_send = !waiting
                    && ui.memory(|memory| memory.has_focus(id))
                    && ui.input_mut(|input| {
                        !input
                            .events
                            .iter()
                            .any(|event| matches!(event, egui::Event::Ime(_)))
                            && input.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter)
                    });
                egui::ScrollArea::vertical()
                    .id_salt("edit_text_scroll")
                    .max_height(200.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        let response = ui.add_enabled(
                            !waiting,
                            egui::TextEdit::multiline(&mut state.text)
                                .id(id)
                                .hint_text("Edit message")
                                .frame(false)
                                .desired_width(ui.available_width())
                                .desired_rows(3),
                        );
                        if response.changed() {
                            state.error = None;
                        }
                    });
                if let Some(error) = &state.error {
                    ui.colored_label(
                        if ui.visuals().dark_mode {
                            Color32::LIGHT_RED
                        } else {
                            Color32::DARK_RED
                        },
                        error,
                    );
                }
                let has_content = !state.text.trim().is_empty()
                    || message.attachments.len() > state.removed_attachments.len();
                let mut action = None;
                let height = if theme::is_wii(ui.ctx()) { 38.0 } else { 28.0 };
                ui.allocate_ui_with_layout(
                    vec2(ui.available_width(), height),
                    Layout::right_to_left(egui::Align::Center).with_main_wrap(true),
                    |ui| {
                        let send = ui
                            .add_enabled_ui(can_send && has_content && !waiting, |ui| {
                                button(ui, if waiting { "Preparing…" } else { "Send" }, true)
                            })
                            .inner;
                        if send.clicked() || (can_send && has_content && keyboard_send) {
                            action = Some(Action::Send);
                        }
                        if button(ui, "Cancel", false).clicked() {
                            action = Some(Action::Cancel);
                        }
                    },
                );
                action
            });
        if focus {
            editor.response.scroll_to_me(None);
        }
        editor.inner
    })
    .inner
}

fn button(ui: &mut egui::Ui, label: &str, primary: bool) -> egui::Response {
    if theme::is_classic(ui) {
        return super::win98::button(ui, label, vec2(80.0, 28.0));
    }
    if theme::is_wii(ui.ctx()) {
        return super::wii::button(ui, label, 80.0, primary);
    }
    let button = if primary {
        let fill = theme::blue();
        let foreground = if egui::Rgba::from(fill).intensity() > 0.18 {
            Color32::BLACK
        } else {
            Color32::WHITE
        };
        egui::Button::new(RichText::new(label).color(foreground)).fill(fill)
    } else {
        egui::Button::new(label)
    };
    ui.add(button.min_size(vec2(66.0, 28.0)))
}
