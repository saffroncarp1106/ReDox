use std::collections::HashMap;

use crate::app::ui_actions::UiAction;
use eframe::egui::text::{LayoutJob, TextFormat, TextWrapping};
use eframe::egui::{self, Color32, FontId, Frame, Margin, Rect, Sense, Stroke, Ui, vec2};
use pulldown_cmark::{Event, Parser, Tag};
use redox::{
    activity::FileDiff,
    attachments::Attachment,
    domain::{Chat, MessageStatus, Role},
    ollama::{ContextUsage, versioned_ainsley},
};

use super::{icons::{icon_svg, paint_svg}, theme, win98, window_frame};
use crate::app::{ReDox, attachment_limit_message};

mod commands;
mod crt;
mod settings;
pub(super) use settings::Appearance;

const WHITE: Color32 = Color32::WHITE;
const BLACK: Color32 = Color32::BLACK;
const FONT_SIZE: f32 = 8.0;
const CONTROL_FONT_SIZE: f32 = 16.0;
const FONT_FAMILY: &str = "win98_terminal";
const FONT_FILE: &str = "PxPlus_IBM_BIOS.ttf";
const SAFONT_FAMILY: &str = "win98_terminal_safont";
const SAFONT_FILE: &str = "SafFont-Clean.ttf";
const SAFONT_SIZE: f32 = 10.0;
const ROW_HEIGHT: f32 = 10.0;
const ATTACHMENT_CHIP_HEIGHT: f32 = 17.0;
const ATTACHMENT_STATUS_HEIGHT: f32 = 12.0;
const ATTACHMENT_NOTICE_DURATION: f64 = 5.0;
const ATTACHMENT_NOTICE_FADE: f64 = 0.35;
const CONTEXT_METER_HEIGHT: f32 = 18.0;
const ZOOM_MIN: u8 = 75;
const ZOOM_MAX: u8 = 200;
const ZOOM_STEP: u8 = 25;
const SIDEBARS: &str = "win98_terminal_sidebars";
const FULLSCREEN: &str = "win98_terminal_fullscreen";
const CLOSE_CONFIRMATION: &str = "win98_terminal_close_confirmation";

pub(super) fn register_font(fonts: &mut egui::FontDefinitions) {
    let mut family = fonts.families[&egui::FontFamily::Monospace].clone();
    let font = include_bytes!("assets/fonts/ibm-bios/PxPlus_IBM_BIOS.ttf");
    fonts
        .font_data
        .insert(FONT_FILE.into(), egui::FontData::from_static(font).into());
    family.insert(0, FONT_FILE.into());
    fonts
        .families
        .insert(egui::FontFamily::Name(FONT_FAMILY.into()), family.clone());
    fonts.font_data.insert(
        SAFONT_FILE.into(),
        egui::FontData::from_static(include_bytes!("assets/fonts/safont/SafFont-Clean.ttf")).into(),
    );
    family.insert(0, SAFONT_FILE.into());
    fonts
        .families
        .insert(egui::FontFamily::Name(SAFONT_FAMILY.into()), family);
}

fn terminal_font(size: f32) -> FontId {
    FontId::new(size, egui::FontFamily::Name(FONT_FAMILY.into()))
}

pub(super) fn set_open(app: &mut ReDox, context: &egui::Context, open: bool) {
    app.ui_preferences.win98_terminal_open = open;
    context.data_mut(|data| data.remove::<bool>(egui::Id::new(SIDEBARS)));
    context.request_repaint();
    if open {
        context.memory_mut(|memory| {
            if let Some(focused) = memory.focused() {
                memory.surrender_focus(focused);
            }
        });
    } else {
        settings::close(context);
        if is_fullscreen(context) {
            set_fullscreen(context, false);
        }
    }
}

pub(super) fn sidebar_override(context: &egui::Context) -> Option<bool> {
    context.data(|data| data.get_temp(egui::Id::new(SIDEBARS)))
}

pub(super) fn sidebars_visible(app: &ReDox, context: &egui::Context) -> bool {
    sidebar_override(context).unwrap_or(
        app.ui_preferences.win98_terminal_docked
            && (app.sidebar_visible || app.ui_preferences.agents_open),
    )
}

pub(super) fn caption_hidden(app: &ReDox) -> bool {
    app.ui_preferences.win98_terminal_appearance.caption == settings::Caption::Hidden
}

fn toggle_docked(app: &mut ReDox, context: &egui::Context) {
    app.ui_preferences.win98_terminal_docked = !app.ui_preferences.win98_terminal_docked;
    if is_fullscreen(context) {
        set_fullscreen(context, false);
    }
    context.data_mut(|data| data.remove::<bool>(egui::Id::new(SIDEBARS)));
    app.focus_prompt = true;
    context.request_repaint();
}

fn set_sidebars_visible(context: &egui::Context, visible: bool) {
    if visible && is_fullscreen(context) {
        set_fullscreen(context, false);
    }
    context.data_mut(|data| data.insert_temp(egui::Id::new(SIDEBARS), visible));
    context.request_repaint();
}

pub(super) fn is_fullscreen(context: &egui::Context) -> bool {
    context.data(|data| data.get_temp(egui::Id::new(FULLSCREEN)).unwrap_or(false))
}

pub(super) fn reveal_sidebars(context: &egui::Context) {
    context.data_mut(|data| data.insert_temp(egui::Id::new(SIDEBARS), true));
    context.request_repaint();
}

fn set_fullscreen(context: &egui::Context, fullscreen: bool) {
    context.data_mut(|data| data.insert_temp(egui::Id::new(FULLSCREEN), fullscreen));
    window_frame::set_terminal_fullscreen(context, fullscreen);
}

fn consume_plain_key(context: &egui::Context, key: egui::Key) -> bool {
    context.input_mut(|input| {
        let mut pressed = false;
        input.events.retain(|event| {
            let matches = matches!(
                event,
                egui::Event::Key { key: event_key, pressed: true, modifiers, .. }
                    if *event_key == key && *modifiers == egui::Modifiers::NONE
            );
            if matches && matches!(event, egui::Event::Key { repeat: false, .. }) {
                pressed = true;
            }
            !matches
        });
        pressed
    })
}

pub(super) fn fullscreen_for_frame(
    app: &mut ReDox,
    context: &egui::Context,
    available: bool,
) -> bool {
    if context.viewport_id() != egui::ViewportId::ROOT {
        return false;
    }
    let fullscreen = is_fullscreen(context);
    if !available || !app.ui_preferences.win98_terminal_open {
        if fullscreen {
            set_fullscreen(context, false);
        }
        return false;
    }
    if !app.startup.is_active()
        && !settings::is_open(context)
        && !egui::Popup::is_any_open(context)
        && context.memory(|memory| memory.top_modal_layer().is_none())
    {
        if fullscreen
            && !app.settings_open
            && !app.rename_open
            && consume_plain_key(context, egui::Key::Escape)
        {
            set_fullscreen(context, false);
            app.focus_prompt = true;
            return false;
        }
        if consume_plain_key(context, egui::Key::F11) {
            set_fullscreen(context, !fullscreen);
            app.focus_prompt = true;
            return !fullscreen;
        }
    }
    fullscreen
}

pub(super) fn show(ui: &mut Ui, rect: Rect, app: &mut ReDox) {
    let zoom = app
        .ui_preferences
        .win98_terminal_zoom
        .clamp(ZOOM_MIN, ZOOM_MAX);
    let scale = f32::from(zoom) / 100.0;
    let inner = rect.shrink(3.0);
    ui.painter().rect_filled(rect, 0, theme::sidebar());
    win98::bevel(ui.painter(), rect, false);
    win98::bevel(ui.painter(), rect.shrink(1.0), false);
    let mut terminal = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("win98_terminal")
            .max_rect(inner),
    );
    terminal.set_clip_rect(ui.clip_rect().intersect(inner));
    show_caption(&mut terminal, app, zoom);
    let crt = app.ui_preferences.win98_terminal_appearance.crt;
    egui::TopBottomPanel::bottom("win98_terminal_prompt")
        .show_separator_line(false)
        .frame(Frame::NONE.fill(BLACK).inner_margin(Margin::same(7)))
        .show_inside(&mut terminal, |ui| prompt(ui, app, scale));
    egui::TopBottomPanel::bottom("win98_terminal_attachments")
        .exact_height(ATTACHMENT_CHIP_HEIGHT + ATTACHMENT_STATUS_HEIGHT + 6.0)
        .show_separator_line(false)
        .frame(
            Frame::NONE
                .fill(BLACK)
                .inner_margin(Margin::symmetric(7, 1)),
        )
        .show_inside(&mut terminal, |ui| {
            ui.visuals_mut().override_text_color = Some(WHITE);
            let notice_opacity = attachment_notice_opacity(ui.ctx(), app);
            if let Some(chat) = app.session.active_mut() {
                pending_attachments(ui, chat, notice_opacity);
            }
        });
    if app.is_ollama() {
        egui::TopBottomPanel::bottom("win98_terminal_context")
            .exact_height(CONTEXT_METER_HEIGHT)
            .show_separator_line(false)
            .frame(
                Frame::NONE
                    .fill(BLACK)
                    .inner_margin(Margin::symmetric(7, 0)),
            )
            .show_inside(&mut terminal, |ui| show_context_meter(ui, app));
    }
    let screen = terminal
        .available_rect_before_wrap()
        .intersect(terminal.clip_rect());
    egui::CentralPanel::default()
        .frame(Frame::NONE.fill(BLACK).inner_margin(Margin::same(7)))
        .show_inside(&mut terminal, |ui| {
            crt.paint_background(ui, screen);
            let curvature = crt.curvature(screen);
            let visuals = ui.visuals_mut();
            visuals.extreme_bg_color = BLACK;
            visuals.widgets.inactive.bg_fill = WHITE;
            visuals.widgets.hovered.bg_fill = WHITE;
            visuals.widgets.active.bg_fill = WHITE;
            ui.spacing_mut().item_spacing.y = 0.0;
            let chat = app.session.active();
            let font = terminal_font(FONT_SIZE * scale);
            let row_height = ROW_HEIGHT * scale;
            let width = (ui.available_width() - ui.spacing().scroll.bar_width - 8.0).max(1.0);
            let wrap = |text: &str, ainsley| {
                if ainsley {
                    let safont = FontId::new(
                        SAFONT_SIZE * scale,
                        egui::FontFamily::Name(SAFONT_FAMILY.into()),
                    );
                    let job = ainsley_layout(text, &font, &safont, row_height, width);
                    return wrap_ainsley(ui, job);
                }
                ui.painter()
                    .layout(text.to_owned(), font.clone(), WHITE, width)
                    .rows
                    .iter()
                    .map(|row| TranscriptRow::Plain(row.text()))
                    .collect::<Vec<_>>()
            };
            let (mut rows, attachments) = terminal_rows(chat, &wrap);
            if let Some(chat) = chat
                && let Some(output) = commands::output(ui.ctx(), chat.id)
            {
                rows.extend(wrap(&output, false));
            }
            egui::ScrollArea::both()
                .id_salt(("win98_terminal_transcript", chat.map(|chat| chat.id)))
                .auto_shrink([false; 2])
                .stick_to_bottom(true)
                .show_rows(ui, row_height, rows.len(), |ui, visible| {
                    for index in visible {
                        let response =
                            draw_row(ui, &rows[index], &font, row_height, crt, curvature);
                        if let Some(file) = attachments.get(&index) {
                            response.on_hover_ui(|ui| attachment_preview(ui, file));
                        }
                    }
                });
        });
    if !(cfg!(windows) && is_fullscreen(ui.ctx()))
        && ui.rect_contains_pointer(rect)
        && let Some(position) = ui.ctx().pointer_hover_pos()
    {
        ui.ctx().set_cursor_icon(egui::CursorIcon::None);
        let painter = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("win98_terminal_cursor"),
        ));
        paint_svg(
            &painter,
            Rect::from_min_size(position, vec2(11.0, 16.0)),
            icon_svg!("classic-pointer"),
            WHITE,
        );
    }
    settings::show(ui.ctx(), app);
    show_close_confirmation(ui.ctx(), app);
}

fn show_close_confirmation(context: &egui::Context, app: &mut ReDox) {
    let id = egui::Id::new(CLOSE_CONFIRMATION);
    if !context.data(|data| data.get_temp(id).unwrap_or(false)) {
        return;
    }
    let modal = egui::Modal::new(id).show(context, |ui| {
        ui.set_width(210.0);
        ui.strong("Close ReDox?");
        ui.label("Are you sure you want to quit?");
        ui.add_space(6.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let cancel = win98::button(ui, "Cancel", vec2(72.0, 23.0)).clicked();
            let confirm = win98::button(ui, "Close ReDox", vec2(94.0, 23.0)).clicked();
            (confirm, cancel)
        })
        .inner
    });
    let (confirm, cancel) = modal.inner;
    if confirm || cancel || modal.should_close() {
        context.data_mut(|data| data.remove::<bool>(id));
        if confirm {
            context.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            app.focus_prompt = true;
        }
    }
}

fn show_caption(terminal: &mut Ui, app: &mut ReDox, zoom: u8) {
    let caption_style = app.ui_preferences.win98_terminal_appearance.caption;
    if caption_style == settings::Caption::Hidden {
        return;
    }
    egui::TopBottomPanel::top("win98_terminal_caption")
        .exact_height(28.0)
        .show_separator_line(false)
        .frame(Frame::NONE)
        .show_inside(terminal, |ui| {
            let (caption, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), 25.0), Sense::hover());
            let ink = caption_style.paint(ui, caption);
            let close =
                Rect::from_center_size(caption.right_center() - vec2(11.0, 0.0), vec2(16.0, 15.0));
            let fullscreen = is_fullscreen(ui.ctx());
            let toggle = close.translate(vec2(-20.0, 0.0));
            let title = if fullscreen {
                "Exit fullscreen"
            } else {
                "Fullscreen"
            };
            let response = ui
                .interact(toggle, ui.id().with("fullscreen"), Sense::click())
                .on_hover_text(format!("{title} (F11)"));
            response
                .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, title));
            win98::caption_control(
                ui.painter(),
                toggle,
                1,
                fullscreen,
                response.is_pointer_button_down_on(),
            );
            if response.clicked() {
                set_fullscreen(ui.ctx(), !fullscreen);
                app.focus_prompt = true;
            }
            let docked = app.ui_preferences.win98_terminal_docked;
            let minimize = toggle.translate(vec2(-20.0, 0.0));
            {
                let response = ui
                    .interact(minimize, ui.id().with("minimize"), Sense::click())
                    .on_hover_text("Minimize ReDox");
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Minimize ReDox")
                });
                win98::caption_control(
                    ui.painter(),
                    minimize,
                    0,
                    false,
                    response.is_pointer_button_down_on(),
                );
                if response.clicked() {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }
            }
            let dock = Rect::from_min_size(
                egui::pos2(caption.left() + 6.0, toggle.top()),
                vec2(56.0, toggle.height()),
            );
            let zoom_out = Rect::from_min_size(dock.right_top() + vec2(6.0, 0.0), close.size());
            let zoom_in = zoom_out.translate(vec2(57.0, 0.0));
            let icons = Rect::from_min_size(
                zoom_in.right_top() + vec2(6.0, 0.0),
                vec2(72.0, toggle.height()),
            );
            let label = win98::icons_toggle_label(app.ui_preferences.win98_icons_visible(true));
            if caption_button(ui, icons, label, "Toggle Terminal shortcut icons", true) {
                app.ui_preferences.toggle_win98_icons(true);
                ui.ctx().request_repaint();
            }
            let title_rect = Rect::from_min_max(
                egui::pos2(icons.right() + 8.0, caption.top()),
                egui::pos2(minimize.left() - 6.0, caption.bottom()),
            );
            if title_rect.is_positive() {
                ui.painter().with_clip_rect(title_rect).text(
                    title_rect.left_center(),
                    egui::Align2::LEFT_CENTER,
                    "ReDox Terminal",
                    theme::semibold(13.0),
                    ink,
                );
                if !fullscreen && !docked {
                    let drag =
                        ui.interact(title_rect, ui.id().with("drag"), Sense::click_and_drag());
                    window_frame::title_drag(ui.ctx(), &drag);
                }
            }
            let (label, tooltip) = if docked {
                ("Undock", "Expand Terminal across the ReDox window")
            } else {
                ("Dock", "Show Terminal inside the Chat pane")
            };
            if caption_button(ui, dock, label, tooltip, true) {
                toggle_docked(app, ui.ctx());
            }
            let updated_zoom = zoom_controls(ui, zoom_out, zoom_in, zoom, ink);
            if updated_zoom != zoom {
                app.focus_prompt = true;
            }
            app.ui_preferences.win98_terminal_zoom = updated_zoom;
            let response = ui
                .interact(close, ui.id().with("close"), Sense::click())
                .on_hover_text("Close ReDox");
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Close ReDox")
            });
            win98::caption_control(
                ui.painter(),
                close,
                2,
                false,
                response.is_pointer_button_down_on(),
            );
            if response.clicked() {
                if let Some(input) = app.prompt_input_id {
                    ui.memory_mut(|memory| memory.surrender_focus(input));
                }
                app.focus_prompt = false;
                ui.ctx().data_mut(|data| {
                    data.insert_temp(egui::Id::new(CLOSE_CONFIRMATION), true);
                });
            }
        });
}

fn zoom_controls(ui: &Ui, zoom_out: Rect, zoom_in: Rect, zoom: u8, ink: Color32) -> u8 {
    ui.painter().text(
        zoom_out.center().lerp(zoom_in.center(), 0.5),
        egui::Align2::CENTER_CENTER,
        format!("{zoom}%"),
        FontId::proportional(11.0),
        ink,
    );
    let decrease = caption_button(ui, zoom_out, "−", "Zoom out", zoom > ZOOM_MIN);
    let increase = caption_button(ui, zoom_in, "+", "Zoom in", zoom < ZOOM_MAX);
    if decrease {
        zoom.saturating_sub(ZOOM_STEP).max(ZOOM_MIN)
    } else if increase {
        zoom.saturating_add(ZOOM_STEP).min(ZOOM_MAX)
    } else {
        zoom
    }
}

fn caption_button(ui: &Ui, rect: Rect, label: &str, tooltip: &str, enabled: bool) -> bool {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let response = ui
        .interact(rect, ui.id().with(tooltip), sense)
        .on_hover_text(tooltip);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tooltip));
    ui.painter().rect_filled(rect, 0, win98::control_face());
    win98::bevel(ui.painter(), rect, response.is_pointer_button_down_on());
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::proportional(12.0),
        if enabled {
            win98::control_ink()
        } else {
            Color32::GRAY
        },
    );
    response.clicked()
}

fn prompt(ui: &mut Ui, app: &mut ReDox, scale: f32) {
    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.max_rect().top() - 4.0,
        Stroke::new(1.0, WHITE),
    );
    ui.visuals_mut().override_text_color = Some(WHITE);
    ui.visuals_mut().selection.bg_fill = Color32::from_gray(80);
    let caret_width = settings::caret_width(app.ui_preferences.win98_terminal_appearance);
    ui.visuals_mut().text_cursor.stroke = Stroke::new(caret_width, WHITE);
    let busy = app.running.is_some();
    let active_running = app.active_chat_running();
    let settings_open = settings::is_open(ui.ctx());
    let can_enter = !settings_open
        && !app.is_editing_message()
        && !app.picking_files
        && !app.startup.is_active();
    let can_send = app.can_start_prompt() || busy;
    let Some(chat) = app.session.active() else {
        return;
    };
    let editable = !chat.archived;
    let can_complete = editable && can_enter && !app.settings_open && !app.rename_open;
    let input_id = egui::Id::new(("win98_terminal_input", ui.ctx().viewport_id()));
    let window_ready = ui.input(|input| {
        input.focused
            && !input.pointer.any_down()
            && !input.pointer.any_released()
            && !input.key_pressed(egui::Key::Tab)
    });
    if can_complete
        && window_ready
        && !egui::Popup::is_any_open(ui.ctx())
        && ui.memory(|memory| memory.top_modal_layer().is_none())
        && (app.focus_prompt
            || is_fullscreen(ui.ctx()) && ui.memory(|memory| memory.focused().is_none()))
    {
        ui.memory_mut(|memory| memory.request_focus(input_id));
        app.focus_prompt = false;
    }
    let suggestions = if can_complete {
        commands::suggestions(app, &chat.draft)
    } else {
        Vec::new()
    };
    let chat = app.session.active_mut().expect("active Terminal chat");
    if let Some(suggestion) = suggestions.first()
        && ui.memory(|memory| memory.has_focus(input_id))
        && ui.input_mut(|input| {
            input.modifiers == egui::Modifiers::NONE
                && input.consume_key(egui::Modifiers::NONE, egui::Key::Tab)
        })
    {
        // egui schedules focus traversal before widgets handle input.
        ui.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
        complete_command(ui.ctx(), input_id, &mut chat.draft, &suggestion.input);
    }
    let lock_tab = !suggestions.is_empty();
    let has_attachments = !chat.attachments.is_empty();
    let ready = |draft: &str| {
        can_enter
            && editable
            && (commands::is_command(draft)
                || can_send && (!draft.trim().is_empty() || has_attachments))
    };
    let font = terminal_font(FONT_SIZE * scale);
    let row_height = ROW_HEIGHT * scale;
    let mut send = false;
    let mut stop = false;
    let response = ui
        .horizontal(|ui| {
            ui.add_enabled_ui(editable && can_enter, |ui| {
                app.attachment_menu_sized(ui, 20.0)
            });
            app.prompt_access(ui);
            let chat = app.session.active_mut().expect("active Terminal chat");
            ui.label(egui::RichText::new(">").font(font.clone()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if active_running {
                    stop = terminal_button(ui, "Stop", true);
                }
                let label = if commands::is_command(&chat.draft) {
                    "Run"
                } else if busy {
                    "Queue"
                } else {
                    "Send"
                };
                send = terminal_button(ui, label, ready(&chat.draft));
                let response = ui
                    .with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("win98_terminal_prompt_scroll")
                            .auto_shrink([false, true])
                            .max_height(4.0 * row_height)
                            .show(ui, |ui| {
                                ui.add_enabled(
                                    editable,
                                    prompt_editor(&mut chat.draft, ui.available_width(), &font)
                                        .id(input_id)
                                        .lock_focus(lock_tab)
                                        .min_size(vec2(0.0, ui.spacing().interact_size.y)),
                                )
                            })
                            .inner
                    })
                    .inner;
                app.prompt_input_id = Some(response.id);
                send |= ready(&chat.draft) && enter_pressed(&response);
                response
            })
            .inner
        })
        .inner;

    if can_complete && !send && !stop {
        let chat = app.session.active().expect("active Terminal chat");
        let suggestions = commands::suggestions(app, &chat.draft);
        let has_suggestions = !suggestions.is_empty();
        if response.changed() || lock_tab != has_suggestions {
            ui.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        tab: has_suggestions,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        ..Default::default()
                    },
                )
            });
            ui.ctx().request_repaint();
        }
        if has_suggestions {
            ui.horizontal_wrapped(|ui| {
                for suggestion in suggestions {
                    let button = egui::Button::new(
                        egui::RichText::new(&suggestion.input).font(font.clone()),
                    )
                    .fill(BLACK)
                    .stroke(Stroke::new(1.0, WHITE));
                    if ui
                        .add(button)
                        .on_hover_text(suggestion.description)
                        .clicked()
                        && let Some(chat) = app.session.active_mut()
                    {
                        complete_command(ui.ctx(), response.id, &mut chat.draft, &suggestion.input);
                    }
                }
            });
        }
    }
    if send && let Some(chat) = app.session.active_mut() {
        if commands::is_command(&chat.draft) {
            let command = std::mem::take(&mut chat.draft);
            commands::run(app, ui.ctx(), &command);
        } else {
            commands::clear_output(ui.ctx(), chat.id);
            app.dispatch_ui_action(UiAction::Send, ui.ctx());
        }
        app.focus_prompt = true;
    }
    if stop {
        app.dispatch_ui_action(UiAction::Stop, ui.ctx());
        app.focus_prompt = true;
    }
    if let Some(error) = &app.attachment_error {
        ui.label(egui::RichText::new(error).font(font));
    }
}

fn complete_command(context: &egui::Context, input: egui::Id, draft: &mut String, command: &str) {
    draft.clear();
    draft.push_str(command);
    if let Some(mut state) = egui::TextEdit::load_state(context, input) {
        let end = egui::text::CCursor::new(command.chars().count());
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(end)));
        egui::TextEdit::store_state(context, input, state);
    }
    context.memory_mut(|memory| {
        if !memory.has_focus(input) {
            memory.request_focus(input);
        }
    });
    context.request_repaint();
}

fn prompt_editor<'a>(draft: &'a mut String, width: f32, font: &FontId) -> egui::TextEdit<'a> {
    egui::TextEdit::multiline(draft)
        .font(font.clone())
        .desired_width(width)
        .desired_rows(1)
        .vertical_align(egui::Align::Center)
        .frame(false)
        .return_key(egui::KeyboardShortcut::new(
            egui::Modifiers::SHIFT,
            egui::Key::Enter,
        ))
}

fn enter_pressed(response: &egui::Response) -> bool {
    response.has_focus()
        && response.ctx.input_mut(|input| {
            !input.modifiers.shift
                && !input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Ime(_)))
                && input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
        })
}

fn terminal_button(ui: &mut Ui, label: &str, enabled: bool) -> bool {
    ui.add_enabled(
        enabled,
        egui::Button::new(egui::RichText::new(label).font(terminal_font(CONTROL_FONT_SIZE)))
            .fill(BLACK)
            .stroke(Stroke::new(1.0, WHITE))
            .min_size(vec2(45.0, 20.0)),
    )
    .clicked()
}

fn attachment_notice_opacity(context: &egui::Context, app: &mut ReDox) -> f32 {
    let Some((chat_id, started)) = app.attachment_limit_notice else {
        return 0.0;
    };
    let remaining = ATTACHMENT_NOTICE_DURATION - (context.input(|input| input.time) - started);
    if remaining <= 0.0 {
        app.attachment_limit_notice = None;
        return 0.0;
    }
    if app.session.active_chat != Some(chat_id) {
        return 0.0;
    }
    if remaining <= ATTACHMENT_NOTICE_FADE {
        super::request_animation_frame(context);
    } else {
        context.request_repaint_after(std::time::Duration::from_secs_f64(
            remaining - ATTACHMENT_NOTICE_FADE,
        ));
    }
    (remaining / ATTACHMENT_NOTICE_FADE).min(1.0) as f32
}

fn context_meter_text(usage: Option<ContextUsage>) -> String {
    const SEGMENTS: usize = 9;
    let Some(usage) = usage else {
        return "Context usage unavailable".into();
    };
    let used = 1.0 - usage.remaining_percent() / 100.0;
    let filled = (used * SEGMENTS as f64).round() as usize;
    format!(
        "{} [{}{}]",
        super::usage::ollama_context_label(usage),
        "|".repeat(filled),
        " ".repeat(SEGMENTS - filled)
    )
}

fn qualification_duration(seconds: f64) -> String {
    let tenths = (seconds * 10.0).round() as u64;
    let minutes = tenths / 600;
    let seconds = (tenths / 10) % 60;
    let fraction = tenths % 10;
    if minutes == 0 {
        format!("{seconds}.{fraction}s")
    } else {
        format!("{minutes}m {seconds:02}.{fraction}s")
    }
}

fn show_context_meter(ui: &mut Ui, app: &ReDox) {
    let usage = app
        .session
        .active()
        .and_then(redox::domain::Chat::ollama_context);
    let qualification = app.session.active().and_then(|chat| {
        let (id, timing) = chat.ollama_usage.qualification.as_ref()?;
        chat.messages
            .iter()
            .any(|message| {
                message.id == *id
                    && message.model.strip_prefix("Ollama · ") == Some(chat.ollama_model.as_str())
            })
            .then_some(timing)
    });
    let mut meter = context_meter_text(usage);
    if !app.active_chat_running()
        && let Some(timing) = qualification.filter(|timing| timing.completed)
    {
        meter.push_str("  · Qualification ");
        meter.push_str(&qualification_duration(timing.elapsed_seconds));
    }
    ui.scope(|ui| {
        ui.visuals_mut().override_text_color = Some(WHITE);
        ui.horizontal_centered(|ui| {
            ui.add(
                egui::Label::new(egui::RichText::new(meter).font(terminal_font(FONT_SIZE)))
                    .truncate(),
            )
            .on_hover_ui(|ui| {
                if let Some(timing) = qualification {
                    let label = if timing.completed {
                        "Qualification total"
                    } else {
                        "Qualification elapsed at last update"
                    };
                    ui.label(format!(
                        "{label} · {} · Attempt {}",
                        qualification_duration(timing.elapsed_seconds),
                        timing.attempt
                    ));
                    if let Some(seconds) = timing.first_load_seconds {
                        ui.label(format!(
                            "First model load · {}",
                            qualification_duration(seconds)
                        ));
                    }
                    for (index, seconds) in timing.trial_seconds.iter().enumerate() {
                        ui.label(format!(
                            "Reasoning trial {} · {}",
                            index + 1,
                            qualification_duration(*seconds)
                        ));
                    }
                    if let Some(seconds) = timing.rust_seconds {
                        ui.label(format!(
                            "Rust parser trial · {}",
                            qualification_duration(seconds)
                        ));
                    }
                    ui.separator();
                }
                let Some(usage) = usage else {
                    ui.label("Ollama context usage appears when a prompt is sent.");
                    return;
                };
                let label = if usage.estimated {
                    "Estimated prompt"
                } else {
                    "Last Ollama request"
                };
                ui.label(format!(
                    "{label} · {} of {} tokens left",
                    usage.remaining_tokens(),
                    usage.limit
                ));
                if let Some(seconds) = usage.load_seconds {
                    ui.label(format!("Model load · {seconds:.2} s"));
                }
                if let Some(seconds) = usage.prompt_seconds {
                    ui.label(format!("Prompt evaluation · {seconds:.2} s"));
                }
                if let Some(tokens) = usage.cached_tokens {
                    ui.label(format!("Prompt cache · {tokens} tokens reused"));
                }
            });
            if app.active_chat_running() {
                let status = app.ollama_stage.map_or(app.status.as_str(), |stage| stage);
                let summary = status
                    .strip_prefix("Ollama · ")
                    .and_then(|text| text.split_once(": "))
                    .map_or(status, |(_, summary)| summary);
                ui.add(egui::Label::new(egui::RichText::new(summary).size(11.0)).truncate())
                    .on_hover_text(status);
            }
        });
    });
}

fn pending_attachments(ui: &mut Ui, chat: &mut Chat, notice_opacity: f32) {
    let mut remove = None;
    Frame::NONE
        .stroke(Stroke::new(1.0, Color32::GRAY))
        .inner_margin(Margin::symmetric(4, 1))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let width = ui.available_width();
            let (row, _) =
                ui.allocate_exact_size(vec2(width, ATTACHMENT_CHIP_HEIGHT), Sense::hover());
            if !chat.attachments.is_empty() {
                let count = chat.attachments.len() as f32;
                let gap = 4.0_f32.min(width / (2.0 * count));
                let chip_width = ((width - (count - 1.0) * gap) / count).min(144.0);
                for (index, file) in chat.attachments.iter().enumerate() {
                    let chip = Rect::from_min_size(
                        row.left_top() + vec2(index as f32 * (chip_width + gap), 0.0),
                        vec2(chip_width, row.height()),
                    );
                    ui.painter().rect_stroke(
                        chip,
                        0,
                        Stroke::new(1.0, Color32::DARK_GRAY),
                        egui::StrokeKind::Inside,
                    );
                    let mut label = chip.shrink2(vec2(2.0_f32.min(chip_width * 0.1), 1.0));
                    if !chat.archived {
                        let remove_width = 16.0_f32.min(label.width() / 2.0);
                        let remove_rect = label.with_min_x(label.right() - remove_width);
                        label.set_right(remove_rect.left());
                        if ui
                            .put(remove_rect, egui::Button::new("×").small().frame(false))
                            .on_hover_text("Remove attachment")
                            .clicked()
                        {
                            remove = Some(file.id);
                        }
                    }
                    ui.put(
                        label,
                        egui::Label::new(egui::RichText::new(file.name()).size(11.0)).truncate(),
                    )
                    .on_hover_ui(|ui| attachment_preview(ui, file));
                }
            }
            let (status, _) =
                ui.allocate_exact_size(vec2(width, ATTACHMENT_STATUS_HEIGHT), Sense::hover());
            if notice_opacity > 0.0 {
                ui.painter()
                    .with_clip_rect(status.intersect(ui.clip_rect()))
                    .text(
                        status.left_center(),
                        egui::Align2::LEFT_CENTER,
                        attachment_limit_message(),
                        FontId::proportional(11.0),
                        WHITE.gamma_multiply(notice_opacity),
                    );
            }
        });
    if let Some(id) = remove {
        chat.attachments.retain(|file| file.id != id);
    }
}

fn attachment_preview(ui: &mut Ui, file: &Attachment) {
    ui.visuals_mut().override_text_color = None;
    ui.set_width(ui.available_width().min(320.0));
    ui.label(file.name());
    if let Some(text) = &file.text {
        let mut chars = text.chars();
        let excerpt: String = chars.by_ref().take(800).collect();
        ui.label(egui::RichText::new(excerpt).monospace());
        if chars.next().is_some() {
            ui.label("…");
        }
    } else {
        super::media::show(ui, &file.path.to_string_lossy(), false, None);
    }
}

enum TranscriptRow {
    Plain(String),
    Mixed(LayoutJob),
}

impl From<&str> for TranscriptRow {
    fn from(text: &str) -> Self {
        Self::Plain(text.to_owned())
    }
}

impl TranscriptRow {
    fn text(&self) -> &str {
        match self {
            Self::Plain(text) => text,
            Self::Mixed(job) => &job.text,
        }
    }
}

fn ainsley_layout(
    text: &str,
    ibm: &FontId,
    safont: &FontId,
    row_height: f32,
    width: f32,
) -> LayoutJob {
    let mut job = LayoutJob {
        wrap: TextWrapping {
            max_width: width,
            ..Default::default()
        },
        ..Default::default()
    };
    let format = |font: &FontId| TextFormat {
        line_height: Some(row_height),
        ..TextFormat::simple(font.clone(), WHITE)
    };
    let prose = format(safont);
    let code = format(ibm);
    let mut offset = 0;
    for (event, range) in Parser::new(text).into_offset_iter() {
        if matches!(event, Event::Code(_) | Event::Start(Tag::CodeBlock(_))) {
            if offset < range.start {
                job.append(&text[offset..range.start], 0.0, prose.clone());
            }
            job.append(&text[range.clone()], 0.0, code.clone());
            offset = range.end;
        }
    }
    if offset < text.len() {
        job.append(&text[offset..], 0.0, prose);
    }
    job
}

fn wrap_ainsley(ui: &Ui, job: LayoutJob) -> Vec<TranscriptRow> {
    let galley = ui.painter().layout_job(job);
    let mut start = 0;
    galley
        .rows
        .iter()
        .map(|row| {
            let end = start
                + row
                    .glyphs
                    .iter()
                    .map(|glyph| glyph.chr.len_utf8())
                    .sum::<usize>();
            let job = &galley.job;
            let mut line = LayoutJob::default();
            let first = job
                .sections
                .partition_point(|section| section.byte_range.end <= start);
            for section in &job.sections[first..] {
                let range = &section.byte_range;
                if range.start >= end {
                    break;
                }
                line.append(
                    &job.text[range.start.max(start)..range.end.min(end)],
                    0.0,
                    section.format.clone(),
                );
            }
            start = end + usize::from(row.ends_with_newline);
            TranscriptRow::Mixed(line)
        })
        .collect()
}

fn terminal_rows(
    chat: Option<&Chat>,
    wrap: impl Fn(&str, bool) -> Vec<TranscriptRow>,
) -> (Vec<TranscriptRow>, HashMap<usize, &Attachment>) {
    let mut rows = Vec::new();
    let mut attachments = HashMap::new();
    if let Some(chat) = chat {
        for message in &chat.messages {
            if message.role == Role::User && !message.content.is_empty() {
                rows.extend(wrap(&format!("> {}", message.content), false));
            }
            for file in &message.attachments {
                for label in wrap(&format!("[Attachment: {}]", file.name()), false) {
                    attachments.insert(rows.len(), file);
                    rows.push(label);
                }
            }
            match message.status {
                MessageStatus::Queued => rows.push("[queued]".into()),
                MessageStatus::Streaming if message.content.is_empty() => {
                    rows.push("Working...".into());
                }
                MessageStatus::Cancelled => rows.push("Response stopped.".into()),
                MessageStatus::Interrupted => rows.push("Response interrupted.".into()),
                MessageStatus::Failed if message.error.is_none() => {
                    rows.push("Response failed.".into());
                }
                _ => {}
            }
            if let Some(error) = &message.error {
                rows.extend(wrap(error, false));
            }
            let activity = &message.activity;
            for file in &activity.files {
                rows.push(TranscriptRow::Plain(file.path.clone()));
                append_sections(&mut rows, file);
            }
            if activity.preview_limited {
                rows.push("Diff preview limited. Counts reflect the visible lines.".into());
                rows.push("".into());
            }
            if message.role == Role::Assistant && !message.content.is_empty() {
                rows.push("----- redox -----".into());
                rows.push("".into());
                let ainsley = message
                    .model
                    .strip_prefix("Ollama · ")
                    .and_then(versioned_ainsley)
                    .is_some();
                rows.extend(wrap(&message.content, ainsley));
            }
            rows.push("".into());
        }
    }
    (rows, attachments)
}

fn append_sections(rows: &mut Vec<TranscriptRow>, file: &FileDiff) {
    let mut removed = Vec::new();
    let mut added = Vec::new();
    let mut in_hunk = false;
    for line in file.patch.lines() {
        if in_hunk {
            if let Some(code) = line.strip_prefix('-') {
                removed.push(code);
                continue;
            } else if let Some(code) = line.strip_prefix('+') {
                added.push(code);
                continue;
            } else if line.starts_with("\\ ") {
                continue;
            }
        }
        in_hunk |= line.starts_with("@@ ");
        append_block(rows, "remove", &mut removed);
        append_block(rows, "add", &mut added);
    }
    append_block(rows, "remove", &mut removed);
    append_block(rows, "add", &mut added);
    if !in_hunk {
        rows.push("No text changes in this file.".into());
    }
}

fn append_block(rows: &mut Vec<TranscriptRow>, operation: &str, lines: &mut Vec<&str>) {
    if lines.is_empty() {
        return;
    }
    let marker = format!("----- {operation} -----");
    rows.push(TranscriptRow::Plain(marker.clone()));
    rows.push("".into());
    let count = lines.len();
    let unit = if count == 1 { "line" } else { "lines" };
    rows.extend(
        lines
            .drain(..)
            .map(|line| TranscriptRow::Plain(line.replace('\t', "    "))),
    );
    rows.push("".into());
    rows.push(TranscriptRow::Plain(format!("{count} {unit}")));
    rows.push(TranscriptRow::Plain(marker));
    rows.push("".into());
}

fn draw_row(
    ui: &mut Ui,
    row: &TranscriptRow,
    font: &FontId,
    row_height: f32,
    filter: crt::Settings,
    curvature: Option<crt::Curvature>,
) -> egui::Response {
    let galley = match row {
        TranscriptRow::Plain(text) => {
            ui.painter()
                .layout_no_wrap(text.clone(), font.clone(), WHITE)
        }
        TranscriptRow::Mixed(job) => ui.painter().layout_job(job.clone()),
    };
    let (bounds, response) =
        ui.allocate_exact_size(vec2(galley.size().x, row_height), Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, row.text()));
    let origin = bounds.left_center() - vec2(0.0, galley.size().y * 0.5);
    filter.draw_galley(ui.painter(), origin, galley, curvature);
    response
}

#[cfg(test)]
#[path = "../../tests/ui/win98_terminal.rs"]
mod tests;
