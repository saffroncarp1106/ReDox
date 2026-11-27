use super::icons::{self, Icon};
use super::message_edit::{Action as EditAction, Controls as EditControls};
use super::theme::{self, background, muted, text};
use crate::app::ReDox;
use eframe::egui::{
    self, Align, Color32, FontId, Frame, Layout, Margin, RichText, Sense, Stroke, vec2,
};
use redox::domain::{Chat, Message, MessageStatus, Role};
use std::time::Duration;

impl ReDox {
    pub(in crate::app) fn conversation(&mut self, context: &egui::Context) {
        if context.viewport_id() == egui::ViewportId::ROOT {
            egui::TopBottomPanel::top("conversation_header")
                .exact_height(54.0)
                .show_separator_line(false)
                .frame(
                    Frame::NONE
                        .fill(background())
                        .inner_margin(Margin::symmetric(22, 10)),
                )
                .show(context, |ui| {
                    ui.horizontal(|ui| {
                        if icons::button(ui, Icon::Panel, "Toggle sidebar").clicked() {
                            self.sidebar_visible = !self.sidebar_visible;
                        }
                        let (rect, _) = ui.allocate_exact_size(vec2(18.0, 22.0), Sense::hover());
                        icons::paint(ui.painter(), rect.shrink(2.0), Icon::Folder, text());
                        ui.add_space(6.0);
                        let title = self
                            .session
                            .active()
                            .map_or("New chat", |chat| chat.title.as_str());
                        ui.allocate_ui_with_layout(
                            vec2(ui.available_width() - 102.0, 30.0),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(title).font(theme::semibold(14.0)),
                                    )
                                    .truncate(),
                                );
                            },
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if icons::button(ui, Icon::Agent, "Toggle Ollama agents").clicked() {
                                self.ui_preferences.agents_open = !self.ui_preferences.agents_open;
                                context.request_repaint();
                            }
                            ui.menu_button("···", |ui| {
                                if ui.button("Rename chat").clicked() {
                                    self.rename_open = true;
                                    ui.close();
                                }
                                if let Some(chat) = self.session.active() {
                                    let id = chat.id;
                                    let archived = chat.archived;
                                    let label = if chat.archived {
                                        "Restore chat"
                                    } else {
                                        "Archive chat"
                                    };
                                    if ui.button(label).clicked() {
                                        self.archive_local_chat(id, !archived);
                                        ui.close();
                                    }
                                }
                            });
                        });
                    });
                });
        }
        self.composer(context, None);

        egui::CentralPanel::default()
            .frame(
                Frame::NONE
                    .fill(background())
                    .inner_margin(Margin::symmetric(26, 0)),
            )
            .show(context, |ui| {
                let Some(chat) = self.session.active() else {
                    return;
                };
                if let Some(texture) = &self.ui_background {
                    super::skins::paint_background(
                        ui.painter(),
                        ui.max_rect(),
                        texture,
                        self.active_ui.config.background_opacity,
                    );
                }
                if chat.messages.is_empty() {
                    ui.add_space((ui.available_height() * 0.35).max(30.0));
                    ui.vertical_centered(|ui| {
                        let (rect, _) = ui.allocate_exact_size(vec2(52.0, 52.0), Sense::hover());
                        icons::paint(ui.painter(), rect, Icon::Codex, Color32::from_gray(100));
                        ui.add_space(24.0);
                        let heading = chat
                            .workspace
                            .as_ref()
                            .and_then(|path| path.file_name())
                            .map(|name| {
                                format!("What should we build in {}?", name.to_string_lossy())
                            })
                            .unwrap_or_else(|| "What should we build?".into());
                        ui.label(RichText::new(heading).size(28.0));
                        ui.add_space(9.0);
                        ui.label(
                            RichText::new("Ask a question or start working on your project.")
                                .color(muted()),
                        );
                    });
                    return;
                }

                self.chat_transcript(ui, chat.id, 0.0);
            });
    }

    pub(super) fn chat_transcript(
        &mut self,
        ui: &mut egui::Ui,
        scroll_id: impl std::hash::Hash,
        padding: f32,
    ) {
        let enabled = self.can_edit_messages();
        let Some(chat) = self.session.active() else {
            return;
        };
        let follow_latest = self.follow_edited_chat == Some(chat.id);
        if follow_latest {
            self.follow_edited_chat = None;
        }
        let mut controls = EditControls {
            edit: &mut self.message_edit,
            enabled,
            can_send: enabled && self.runtime.is_some(),
            follow_latest,
            action: None,
        };
        render_transcript(
            ui,
            chat,
            scroll_id,
            padding,
            self.ui_preferences.codex_mode,
            Some(&mut controls),
        );
        match controls.action {
            Some(EditAction::Edit(id)) => self.begin_message_edit(id),
            Some(EditAction::Cancel) => self.close_message_edit(ui.ctx()),
            Some(EditAction::Send) => self.submit_message_edit(ui.ctx()),
            Some(EditAction::Switch(message, older)) if enabled => {
                if let Err(error) = self.session.switch_prompt_version(message, older) {
                    self.status = error;
                }
            }
            _ => return,
        }
        ui.ctx().request_repaint();
    }

    pub(super) fn composer(&mut self, context: &egui::Context, parent: Option<&mut egui::Ui>) {
        let classic = parent.is_some();
        let wii = theme::is_wii(context);
        let mut send = false;
        let mut stop = false;
        let panel =
            egui::TopBottomPanel::bottom(egui::Id::new(("composer", context.viewport_id())))
                .min_height(if classic { 88.0 } else { 102.0 })
                .show_separator_line(false)
                .frame(if classic {
                    Frame::NONE.fill(theme::surface()).inner_margin(8)
                } else {
                    Frame::NONE.fill(background()).inner_margin(Margin {
                        left: 16,
                        right: 10,
                        top: 12,
                        bottom: 18,
                    })
                });
        let contents = |ui: &mut egui::Ui| {
            if wii {
                super::wii::striped_panel(ui, ui.max_rect().expand2(vec2(16.0, 18.0)));
            }
            ui.set_clip_rect(ui.clip_rect().intersect(ui.max_rect()));
            Frame::new()
                .fill(theme::input_background())
                .stroke(Stroke::new(
                    1.0,
                    if classic || wii {
                        theme::border()
                    } else {
                        Color32::from_gray(59)
                    },
                ))
                .corner_radius(if classic { 0 } else { 20 })
                .inner_margin(Margin::symmetric(12, 6))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.set_max_width(ui.available_width());
                    if let Some(chat) = self.session.active_mut() {
                        let mut remove = None;
                        let available = ui.available_width();
                        let columns = ((available + 5.0) / 257.0).floor().max(1.0) as usize;
                        let chip_width =
                            ((available - (columns - 1) as f32 * 5.0) / columns as f32 - 12.0)
                                .clamp(1.0, 240.0);
                        ui.vertical(|ui| {
                            for row in chat.attachments.chunks(columns) {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 5.0;
                                    for file in row {
                                        Frame::new()
                                            .fill(if classic || wii {
                                                theme::surface()
                                            } else {
                                                Color32::from_gray(62)
                                            })
                                            .corner_radius(if classic { 0 } else { 8 })
                                            .inner_margin(6)
                                            .show(ui, |ui| {
                                                ui.set_width(chip_width);
                                                ui.horizontal(|ui| {
                                                    ui.add_sized(
                                                        [(chip_width - 33.0).max(1.0), 20.0],
                                                        egui::Label::new(
                                                            RichText::new(file.name()).size(12.0),
                                                        )
                                                        .truncate(),
                                                    )
                                                    .on_hover_ui(|ui| {
                                                        ui.label(file.path.display().to_string());
                                                        if file.text.is_none() {
                                                            ui.set_max_width(280.0);
                                                            super::media::show(
                                                                ui,
                                                                &file.path.to_string_lossy(),
                                                                false,
                                                                None,
                                                            );
                                                        }
                                                    });
                                                    if ui
                                                        .small_button("\u{00d7}")
                                                        .on_hover_text("Remove attachment")
                                                        .clicked()
                                                    {
                                                        remove = Some(file.id);
                                                    }
                                                });
                                            });
                                    }
                                });
                            }
                        });
                        if let Some(id) = remove {
                            chat.attachments.retain(|file| file.id != id);
                        }
                    }
                    if self
                        .session
                        .active()
                        .is_some_and(|chat| !chat.attachments.is_empty())
                    {
                        ui.add_space(10.0);
                    }
                    if self.picking_files {
                        ui.label(RichText::new("Loading files...").size(12.0).color(muted()));
                    }
                    if let Some(error) = &self.attachment_error {
                        ui.label(
                            RichText::new(error)
                                .size(12.0)
                                .color(Color32::from_rgb(235, 150, 135)),
                        );
                    }
                    if let Some(chat) = self.session.active_mut() {
                        ui.add_enabled_ui(!chat.archived, |ui| {
                            let response = ui.add(
                                egui::TextEdit::multiline(&mut chat.draft)
                                    .id_salt("prompt_input")
                                    .frame(false)
                                    .font(FontId::proportional(14.0))
                                    .hint_text(&self.active_ui.config.placeholder)
                                    .desired_width(ui.available_width())
                                    .desired_rows(1)
                                    .return_key(egui::KeyboardShortcut::new(
                                        egui::Modifiers::SHIFT,
                                        egui::Key::Enter,
                                    )),
                            );
                            self.prompt_input_id = Some(response.id);
                            super::controls::focus_new_prompt(
                                &response,
                                &mut self.focus_prompt,
                                response.enabled()
                                    && !self.startup.is_active()
                                    && !self.settings_open
                                    && !self.rename_open,
                            );
                            if response.enabled()
                                && response.has_focus()
                                && context.input(|input| {
                                    input.key_pressed(egui::Key::Enter)
                                        && !input.modifiers.shift
                                        && !input
                                            .events
                                            .iter()
                                            .any(|event| matches!(event, egui::Event::Ime(_)))
                                })
                            {
                                send = true;
                            }
                        });
                    }
                    ui.add_space(6.0);
                    ui.allocate_ui_with_layout(
                        vec2(ui.available_width(), 34.0),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            ui.spacing_mut().item_spacing.x = 3.0;
                            ui.spacing_mut().button_padding.x = 5.0;
                            if classic {
                                ui.spacing_mut().interact_size.y = 28.0;
                                ui.spacing_mut().button_padding.y = 0.0;
                            }
                            self.attachment_menu(ui);
                            self.prompt_access(ui);

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let can_send = !self.is_editing_message()
                                    && self.runtime.is_some()
                                    && !self.picking_files
                                    && self.session.active().is_some_and(|chat| {
                                        !chat.archived
                                            && (!chat.draft.trim().is_empty()
                                                || !chat.attachments.is_empty())
                                    });
                                let running = self.active_chat_running();
                                let sense = if can_send {
                                    Sense::click_and_drag()
                                } else {
                                    Sense::click()
                                };
                                if classic {
                                    let response = ui.add_enabled(
                                        can_send,
                                        egui::Button::new("Send")
                                            .min_size(vec2(58.0, 28.0))
                                            .sense(sense),
                                    );
                                    super::win98::bevel(
                                        ui.painter(),
                                        response.rect,
                                        response.is_pointer_button_down_on(),
                                    );
                                    if can_send {
                                        send |= self.queue_send_control(response);
                                    }
                                    if running {
                                        stop |= super::win98::button(ui, "Stop", vec2(52.0, 28.0))
                                            .clicked();
                                    }
                                    self.model_effort_picker(ui, "composer_model_effort");
                                    return;
                                }
                                if wii {
                                    let response = ui
                                        .add_enabled_ui(can_send || running, |ui| {
                                            super::wii::button_with_sense(
                                                ui,
                                                if running && !can_send { "Stop" } else { "Send" },
                                                76.0,
                                                can_send,
                                                sense,
                                            )
                                        })
                                        .inner;
                                    if can_send {
                                        send |= self.queue_send_control(response);
                                    } else if running {
                                        stop |= response.clicked();
                                    }
                                    if running && can_send && ui.button("Stop").clicked() {
                                        stop = true;
                                    }
                                    self.model_effort_picker(ui, "composer_model_effort");
                                    return;
                                }
                                let (rect, response) =
                                    ui.allocate_exact_size(vec2(28.0, 28.0), sense);
                                let fill = if can_send || running {
                                    theme::blue()
                                } else {
                                    Color32::from_gray(85)
                                };
                                ui.painter().circle_filled(rect.center(), 14.0, fill);
                                let (icon, inset) = if running && !can_send {
                                    (Icon::Stop, 8.0)
                                } else {
                                    (Icon::ArrowUp, 6.0)
                                };
                                icons::paint(ui.painter(), rect.shrink(inset), icon, text());
                                if can_send {
                                    send |= self.queue_send_control(response);
                                } else if running {
                                    stop |= response.on_hover_text("Stop response").clicked();
                                }
                                if running
                                    && can_send
                                    && icons::button(ui, Icon::Close, "Stop response").clicked()
                                {
                                    stop = true;
                                }
                                ui.add_enabled_ui(false, |ui| {
                                    icons::button(
                                        ui,
                                        Icon::Mic,
                                        "Voice input is not available yet",
                                    );
                                });
                                self.model_effort_picker(ui, "composer_model_effort");
                            });
                        },
                    );
                });
            self.queued_messages(ui);
            if self.active_chat_running() {
                ui.add(
                    egui::Label::new(RichText::new(&self.status).size(12.0).color(muted()))
                        .truncate(),
                );
            }
            if !self.is_ollama()
                && self.models.is_empty()
                && !self.refreshing
                && !self.status.starts_with("Ready")
            {
                ui.label(
                    RichText::new(&self.status)
                        .size(12.0)
                        .color(Color32::from_rgb(225, 145, 130)),
                );
            }
        };
        let _panel = if let Some(ui) = parent {
            panel.show_inside(ui, contents)
        } else {
            panel.show(context, contents)
        };

        if send {
            self.dispatch_ui_action(crate::app::ui_actions::UiAction::Send, context);
        }
        if stop {
            self.dispatch_ui_action(crate::app::ui_actions::UiAction::Stop, context);
        }
    }
}

fn render_transcript(
    ui: &mut egui::Ui,
    chat: &Chat,
    scroll_id: impl std::hash::Hash,
    horizontal_padding: f32,
    codex_mode: bool,
    mut edit: Option<&mut EditControls<'_>>,
) {
    ui.spacing_mut().scroll = theme::conversation_scroll();
    super::transcript::show(
        ui,
        &chat.messages,
        scroll_id,
        horizontal_padding,
        codex_mode,
        edit.as_ref().is_some_and(|edit| edit.follow_latest),
        |ui, message| {
            if edit
                .as_deref_mut()
                .is_some_and(|edit| edit.editing(ui, chat, message))
            {
                return;
            }
            render_message(
                ui,
                message,
                chat.workspace.as_deref(),
                codex_mode,
                chat.is_saffyos_locked(),
            );
            if let Some(edit) = &mut edit {
                edit.row_actions(ui, chat, message);
            }
        },
    );
}

fn render_message(
    ui: &mut egui::Ui,
    message: &Message,
    workspace: Option<&std::path::Path>,
    codex_mode: bool,
    saffyos_locked: bool,
) {
    if message.role == Role::User {
        let classic = theme::is_classic(ui);
        let bubble_width = ui.available_width() * 0.72;
        ui.with_layout(Layout::top_down(Align::Max), |ui| {
            Frame::new()
                .fill(if classic && super::win98::high_contrast() {
                    theme::surface()
                } else if !ui.visuals().dark_mode {
                    Color32::from_rgb(218, 231, 247)
                } else {
                    Color32::from_rgb(25, 62, 116)
                })
                .corner_radius(if classic { 0 } else { 19 })
                .inner_margin(Margin::symmetric(15, 10))
                .show(ui, |ui| {
                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                        ui.set_max_width(bubble_width.min(ui.available_width()));
                        for file in message.attachments.iter().filter(|_| !saffyos_locked) {
                            if file.text.is_none() {
                                super::media::show(
                                    ui,
                                    &file.path.to_string_lossy(),
                                    false,
                                    workspace,
                                );
                            }
                            ui.label(RichText::new(format!("{}", file.name())).size(12.0).color(
                                if classic {
                                    muted()
                                } else {
                                    Color32::from_gray(215)
                                },
                            ));
                        }
                        if saffyos_locked {
                            ui.label(&message.content);
                        } else {
                            if !message.attachments.is_empty() {
                                ui.add_space(12.0);
                            }
                            ui.push_id(message.id, |ui| {
                                super::markdown::show_in_workspace(ui, &message.content, workspace)
                            });
                        }
                        if message.status == MessageStatus::Queued && message.error.is_none() {
                            ui.label(RichText::new("Pending").small());
                        }
                        if let Some(error) = &message.error {
                            ui.label(RichText::new(error).small());
                        }
                    });
                });
        });
        return;
    }

    let show_activity = !saffyos_locked && codex_mode && !message.activity.is_empty();
    if message.content.is_empty() && message.status == MessageStatus::Streaming && !show_activity {
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().size(14.0));
            ui.label(RichText::new("Working...").color(muted()));
        });
    } else {
        if message.model.starts_with("Ollama ·") {
            ui.label(RichText::new(&message.model).small().color(muted()));
        }
        ui.push_id(message.id, |ui| {
            if saffyos_locked {
                ui.label(&message.content);
            } else if show_activity {
                let mut start = 0;
                for (index, command) in message.activity.commands.iter().enumerate() {
                    if let Some(text) = message.content.get(start..command.offset) {
                        if !text.is_empty() {
                            ui.push_id(index, |ui| {
                                super::response_text::show_text(ui, message.id, text, workspace);
                            });
                        }
                        start = command.offset;
                    }
                    super::activity::command(
                        ui,
                        command,
                        message.status == MessageStatus::Streaming,
                    );
                }
                if start < message.content.len() {
                    super::response_text::show_text(
                        ui,
                        message.id,
                        &message.content[start..],
                        workspace,
                    );
                }
                super::activity::changes(ui, &message.activity);
            } else {
                super::response_text::show(ui, message, workspace);
            }
        });
    }

    if let Some(error) = &message.error {
        ui.label(RichText::new(error).color(Color32::from_rgb(235, 150, 135)));
    }
    if matches!(
        message.status,
        MessageStatus::Cancelled | MessageStatus::Interrupted
    ) {
        ui.label(RichText::new("Response stopped").size(12.0).color(muted()));
    }
    if message.status != MessageStatus::Streaming && !message.content.is_empty() {
        ui.add_space(10.0);
        copy_response_button(ui, &message.content);
    }
}

#[derive(Clone, Copy)]
struct CopyFeedback {
    button: egui::Id,
    expires_at: f64,
}

fn copy_response_button(ui: &mut egui::Ui, content: &str) {
    let button = ui.make_persistent_id("copy_response");
    let feedback_id = egui::Id::new("copy_response_feedback");
    let now = ui.input(|input| input.time);
    let mut expires_at = ui.data(|data| {
        data.get_temp::<CopyFeedback>(feedback_id)
            .filter(|feedback| feedback.button == button)
            .map(|feedback| feedback.expires_at)
    });
    let response = icons::button_with_icon(ui, Sense::click(), |response| {
        if response.clicked() {
            response.ctx.copy_text(content.to_owned());
            let feedback = CopyFeedback {
                button,
                expires_at: now + 1.0,
            };
            response
                .ctx
                .data_mut(|data| data.insert_temp(feedback_id, feedback));
            expires_at = Some(feedback.expires_at);
        }
        if expires_at.is_some_and(|until| now < until) {
            Icon::Check
        } else {
            Icon::Copy
        }
    });
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), "Copy response")
    });

    if let Some(until) = expires_at {
        if now < until {
            ui.ctx()
                .request_repaint_after(Duration::from_secs_f64(until - now));
            if ui.is_rect_visible(response.rect) {
                egui::Tooltip::for_widget(&response)
                    .show(|ui| ui.label(RichText::new("Copied").small()));
            }
            return;
        }
        // Wait for a fresh hover before showing the regular tooltip again.
        if response.hovered() {
            return;
        }
        ui.data_mut(|data| data.remove::<CopyFeedback>(feedback_id));
    }
    response.on_hover_text("Copy response");
}

#[cfg(test)]
#[path = "../../tests/ui/transcript_layout.rs"]
mod layout_tests;

#[cfg(test)]
#[path = "../../tests/ui/copy_response.rs"]
mod copy_tests;
