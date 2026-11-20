use eframe::egui::{self, Color32, FontId, Frame, Margin, Rect, Sense, Stroke, Ui, vec2};
use redox::{
    activity::FileDiff,
    domain::{Chat, MessageStatus, Role},
};
use redox_studio::actions::UiAction;

use super::{
    controls::focus_new_prompt,
    icons::{icon_svg, paint_svg},
    theme, win98,
};
use crate::app::ReDox;

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
const ROW_HEIGHT: f32 = 10.0;
const ZOOM_MIN: u8 = 75;
const ZOOM_MAX: u8 = 200;
const ZOOM_STEP: u8 = 25;
const OPEN: &str = "win98_terminal_open";
const FULLSCREEN: &str = "win98_terminal_fullscreen";

pub(super) fn register_font(fonts: &mut egui::FontDefinitions) {
    let mut family = fonts.families[&egui::FontFamily::Monospace].clone();
    let font = include_bytes!("assets/fonts/ibm-bios/PxPlus_IBM_BIOS.ttf");
    fonts
        .font_data
        .insert(FONT_FILE.into(), egui::FontData::from_static(font).into());
    family.insert(0, FONT_FILE.into());
    fonts
        .families
        .insert(egui::FontFamily::Name(FONT_FAMILY.into()), family);
}

fn terminal_font(size: f32) -> FontId {
    FontId::new(size, egui::FontFamily::Name(FONT_FAMILY.into()))
}

pub(super) fn is_open(context: &egui::Context) -> bool {
    context.data(|data| data.get_temp(egui::Id::new(OPEN)).unwrap_or(false))
}

pub(super) fn set_open(context: &egui::Context, open: bool) {
    context.data_mut(|data| data.insert_temp(egui::Id::new(OPEN), open));
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

fn is_fullscreen(context: &egui::Context) -> bool {
    context.data(|data| data.get_temp(egui::Id::new(FULLSCREEN)).unwrap_or(false))
}

fn set_fullscreen(context: &egui::Context, fullscreen: bool) {
    context.data_mut(|data| data.insert_temp(egui::Id::new(FULLSCREEN), fullscreen));
    context.send_viewport_cmd(egui::ViewportCommand::Fullscreen(fullscreen));
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
    if !available || !is_open(context) {
        if fullscreen {
            set_fullscreen(context, false);
        }
        return false;
    }
    if !app.startup.is_active()
        && !settings::is_open(context)
        && !egui::Popup::is_any_open(context)
        && context.memory(|memory| memory.top_modal_layer().is_none())
        && context.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F11))
    {
        set_fullscreen(context, !fullscreen);
        app.focus_prompt = true;
        return !fullscreen;
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
    let close_terminal = show_caption(&mut terminal, app, zoom);
    let screen = terminal.available_rect_before_wrap();
    let crt = app.ui_preferences.win98_terminal_appearance.crt;
    let curvature = crt.curvature(screen.intersect(terminal.clip_rect()));
    let text_start = crt.text_start(&terminal);
    egui::TopBottomPanel::bottom("win98_terminal_prompt")
        .show_separator_line(false)
        .frame(Frame::NONE.fill(BLACK).inner_margin(Margin::same(7)))
        .show_inside(&mut terminal, |ui| prompt(ui, app, scale));
    egui::CentralPanel::default()
        .frame(Frame::NONE.fill(BLACK).inner_margin(Margin::same(7)))
        .show_inside(&mut terminal, |ui| {
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
            let wrap = |text: &str| {
                ui.painter()
                    .layout(text.to_owned(), font.clone(), WHITE, width)
                    .rows
                    .iter()
                    .map(|row| row.text())
                    .collect::<Vec<_>>()
            };
            let mut rows = terminal_rows(chat, &wrap);
            if let Some(chat) = chat
                && let Some(output) = commands::output(ui.ctx(), chat.id)
            {
                rows.extend(wrap(&output));
            }
            egui::ScrollArea::both()
                .id_salt(("win98_terminal_transcript", chat.map(|chat| chat.id)))
                .auto_shrink([false; 2])
                .stick_to_bottom(true)
                .show_rows(ui, row_height, rows.len(), |ui, visible| {
                    for row in &rows[visible] {
                        draw_row(ui, row, &font, row_height, curvature);
                    }
                });
        });
    crt.paint(&terminal, screen, text_start);
    if ui.rect_contains_pointer(rect)
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
    if close_terminal {
        set_open(ui.ctx(), false);
        app.focus_prompt = true;
    } else {
        settings::show(ui.ctx(), app);
    }
}

fn show_caption(terminal: &mut Ui, app: &mut ReDox, zoom: u8) -> bool {
    let caption_style = app.ui_preferences.win98_terminal_appearance.caption;
    if caption_style == settings::Caption::Hidden {
        return false;
    }
    egui::TopBottomPanel::top("win98_terminal_caption")
        .exact_height(23.0)
        .show_separator_line(false)
        .frame(Frame::NONE)
        .show_inside(terminal, |ui| {
            let (caption, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
            let ink = caption_style.paint(ui, caption);
            ui.painter().text(
                caption.left_center() + vec2(6.0, 0.0),
                egui::Align2::LEFT_CENTER,
                "ReDox Terminal",
                theme::semibold(13.0),
                ink,
            );
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
            let updated_zoom = zoom_controls(ui, toggle, zoom, ink);
            if updated_zoom != zoom {
                app.focus_prompt = true;
            }
            app.ui_preferences.win98_terminal_zoom = updated_zoom;
            let response = ui.interact(close, ui.id().with("close"), Sense::click());
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Close Terminal")
            });
            win98::caption_control(
                ui.painter(),
                close,
                2,
                false,
                response.is_pointer_button_down_on(),
            );
            response.clicked()
        })
        .inner
}

fn zoom_controls(ui: &Ui, close: Rect, zoom: u8, ink: Color32) -> u8 {
    let zoom_in = close.translate(vec2(-20.0, 0.0));
    let zoom_out = close.translate(vec2(-77.0, 0.0));
    ui.painter().text(
        zoom_out.center().lerp(zoom_in.center(), 0.5),
        egui::Align2::CENTER_CENTER,
        format!("{zoom}%"),
        FontId::proportional(11.0),
        ink,
    );
    let decrease = zoom_button(ui, zoom_out, "−", "Zoom out", zoom > ZOOM_MIN);
    let increase = zoom_button(ui, zoom_in, "+", "Zoom in", zoom < ZOOM_MAX);
    if decrease {
        zoom.saturating_sub(ZOOM_STEP).max(ZOOM_MIN)
    } else if increase {
        zoom.saturating_add(ZOOM_STEP).min(ZOOM_MAX)
    } else {
        zoom
    }
}

fn zoom_button(ui: &Ui, rect: Rect, symbol: &str, title: &str, enabled: bool) -> bool {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let response = ui
        .interact(rect, ui.id().with(title), sense)
        .on_hover_text(title);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, title));
    ui.painter().rect_filled(rect, 0, win98::control_face());
    win98::bevel(ui.painter(), rect, response.is_pointer_button_down_on());
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        symbol,
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
    let suggestions = if can_complete {
        commands::suggestions(app, &chat.draft)
    } else {
        Vec::new()
    };
    let chat = app.session.active_mut().expect("active Terminal chat");
    if let Some(suggestion) = suggestions.first()
        && let Some(input) = app.prompt_input_id
        && ui.memory(|memory| memory.has_focus(input))
        && ui.input_mut(|input| {
            input.modifiers == egui::Modifiers::NONE
                && input.consume_key(egui::Modifiers::NONE, egui::Key::Tab)
        })
    {
        complete_command(ui.ctx(), input, &mut chat.draft, &suggestion.input);
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
                                        .lock_focus(lock_tab)
                                        .min_size(vec2(0.0, ui.spacing().interact_size.y)),
                                )
                            })
                            .inner
                    })
                    .inner;
                app.prompt_input_id = Some(response.id);
                focus_new_prompt(
                    &response,
                    &mut app.focus_prompt,
                    editable
                        && !settings_open
                        && !app.startup.is_active()
                        && !app.settings_open
                        && !app.rename_open,
                );
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
    context.memory_mut(|memory| memory.request_focus(input));
    context.request_repaint();
}

fn prompt_editor<'a>(draft: &'a mut String, width: f32, font: &FontId) -> egui::TextEdit<'a> {
    egui::TextEdit::multiline(draft)
        .id_salt("win98_terminal_input")
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

fn terminal_rows(chat: Option<&Chat>, wrap: impl Fn(&str) -> Vec<String>) -> Vec<String> {
    let mut rows = Vec::new();
    if let Some(chat) = chat {
        for message in &chat.messages {
            if message.role == Role::User && !message.content.is_empty() {
                rows.extend(wrap(&format!("> {}", message.content)));
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
                rows.extend(wrap(error));
            }
            let activity = &message.activity;
            for file in &activity.files {
                rows.push(file.path.clone());
                append_sections(&mut rows, file);
            }
            if activity.preview_limited {
                rows.push("Diff preview limited. Counts reflect the visible lines.".into());
                rows.push(String::new());
            }
            if message.role == Role::Assistant && !message.content.is_empty() {
                rows.push("----- redox -----".into());
                rows.push(String::new());
                rows.extend(wrap(&message.content));
            }
            rows.push(String::new());
        }
    }
    rows
}

fn append_sections(rows: &mut Vec<String>, file: &FileDiff) {
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

fn append_block(rows: &mut Vec<String>, operation: &str, lines: &mut Vec<&str>) {
    if lines.is_empty() {
        return;
    }
    let marker = format!("----- {operation} -----");
    rows.push(marker.clone());
    rows.push(String::new());
    let count = lines.len();
    let unit = if count == 1 { "line" } else { "lines" };
    rows.extend(lines.drain(..).map(|line| line.replace('\t', "    ")));
    rows.push(String::new());
    rows.push(format!("{count} {unit}"));
    rows.push(marker);
    rows.push(String::new());
}

fn draw_row(
    ui: &mut Ui,
    row: &str,
    font: &FontId,
    row_height: f32,
    curvature: Option<crt::Curvature>,
) {
    let galley = ui
        .painter()
        .layout_no_wrap(row.to_owned(), font.clone(), WHITE);
    let (bounds, response) =
        ui.allocate_exact_size(vec2(galley.size().x, row_height), Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, row));
    let origin = bounds.left_center() - vec2(0.0, galley.size().y * 0.5);
    crt::draw_galley(ui.painter(), origin, galley, curvature);
}

#[cfg(test)]
#[path = "../../tests/ui/win98_terminal.rs"]
mod tests;
