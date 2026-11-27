use eframe::egui::{
    self, Align, Color32, Frame, Layout, Margin, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2,
    pos2, vec2,
};

use super::{
    icons::{icon_svg, paint_svg},
    theme, window_frame,
};
use crate::app::ReDox;

pub(super) const ID: &str = "win98";
pub(super) const DARK_ID: &str = "win98-dark";
pub(super) const HEADER_HEIGHT: f32 = 46.0;
pub(super) const NAVY: Color32 = Color32::from_rgb(0, 0, 128);
const SILVER: Color32 = Color32::from_gray(192);
const WHITE: Color32 = Color32::WHITE;
const BLACK: Color32 = Color32::BLACK;

pub(super) fn is_builtin(id: &str) -> bool {
    matches!(id, ID | DARK_ID)
}

pub(super) fn high_contrast() -> bool {
    theme::palette().high_contrast
}

macro_rules! classic_icon_svg {
    ($name:literal) => {
        if high_contrast() {
            egui::include_image!(concat!("assets/icons/", $name, "-gray.svg"))
        } else {
            icon_svg!($name)
        }
    };
}

pub(super) fn control_face() -> Color32 {
    if high_contrast() {
        theme::rail()
    } else {
        SILVER
    }
}

pub(super) fn control_ink() -> Color32 {
    if high_contrast() {
        theme::text()
    } else {
        BLACK
    }
}

pub(super) fn pane_fill() -> Color32 {
    if high_contrast() {
        theme::input_background()
    } else {
        WHITE
    }
}

pub(super) fn inactive_tab() -> Color32 {
    if high_contrast() {
        theme::surface()
    } else {
        Color32::from_gray(176)
    }
}

pub(super) fn selected_fill() -> Color32 {
    if high_contrast() {
        Color32::from_gray(59)
    } else {
        NAVY
    }
}

#[derive(Clone, Copy)]
enum Shortcut {
    Chat,
    Terminal,
    Chats,
    Project,
    Agents,
    Settings,
}

const SHORTCUTS: [Shortcut; 4] = [
    Shortcut::Chats,
    Shortcut::Project,
    Shortcut::Agents,
    Shortcut::Settings,
];
const WINDOWS: [Shortcut; 2] = [Shortcut::Chat, Shortcut::Terminal];

impl Shortcut {
    fn label(self) -> &'static str {
        match self {
            Self::Chat => "Chat",
            Self::Terminal => "Terminal",
            Self::Chats => "My Chats",
            Self::Project => "Open Project",
            Self::Agents => "Ollama Agents",
            Self::Settings => "Settings",
        }
    }

    fn activate(self, app: &mut ReDox, context: &egui::Context) {
        match self {
            Self::Chat => app.focus_win98_prompt(context),
            Self::Terminal => {
                super::win98_terminal::set_open(app, context, true);
                app.focus_prompt = true;
            }
            _ => {
                if let Some(action) = self.action() {
                    app.dispatch_ui_action(action, context);
                    if app.ui_preferences.win98_terminal_open
                        && (matches!(self, Self::Chats) && app.sidebar_visible
                            || matches!(self, Self::Agents) && app.ui_preferences.agents_open)
                    {
                        super::win98_terminal::reveal_sidebars(context);
                    }
                }
            }
        }
    }

    fn action(self) -> Option<crate::app::ui_actions::UiAction> {
        use crate::app::ui_actions::UiAction;
        Some(match self {
            Self::Chat | Self::Terminal => return None,
            Self::Chats => UiAction::Sidebar,
            Self::Project => UiAction::OpenProject,
            Self::Agents => UiAction::OllamaAgent,
            Self::Settings => UiAction::Settings,
        })
    }

    fn paint(self, ui: &Ui, origin: Pos2) {
        let bounds = Rect::from_min_size(origin, vec2(32.0, 32.0));
        let source = match self {
            Self::Chat => icon_svg!("chat"),
            Self::Terminal => {
                let screen = bounds.shrink2(vec2(1.0, 4.0));
                ui.painter().rect_filled(screen, 0, control_face());
                bevel(ui.painter(), screen, false);
                ui.painter().rect_filled(screen.shrink(3.0), 0, BLACK);
                ui.painter().text(
                    screen.left_top() + vec2(6.0, 4.0),
                    egui::Align2::LEFT_TOP,
                    ">_",
                    egui::FontId::monospace(12.0),
                    WHITE,
                );
                return;
            }
            Self::Chats => classic_icon_svg!("classic-chats"),
            Self::Project => classic_icon_svg!("classic-project"),
            Self::Agents => classic_icon_svg!("classic-agents"),
            Self::Settings => classic_icon_svg!("classic-settings"),
        };
        paint_svg(ui.painter(), bounds, source, WHITE);
    }
}

pub(super) fn apply_style(context: &egui::Context) {
    theme::set_controls(context, theme::Controls::Classic);
    let dark = high_contrast();
    let mut visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.panel_fill = theme::sidebar();
    visuals.window_fill = theme::sidebar();
    visuals.window_stroke = Stroke::new(1.0, theme::border());
    visuals.window_corner_radius = egui::CornerRadius::ZERO;
    visuals.menu_corner_radius = egui::CornerRadius::ZERO;
    visuals.window_shadow = egui::Shadow::NONE;
    visuals.popup_shadow = egui::Shadow::NONE;
    visuals.override_text_color = Some(theme::text());
    visuals.hyperlink_color = if dark { theme::blue() } else { NAVY };
    visuals.selection.bg_fill = if dark {
        selected_fill()
    } else {
        Color32::from_rgb(178, 207, 243)
    };
    visuals.extreme_bg_color = pane_fill();
    visuals.code_bg_color = if dark {
        theme::surface()
    } else {
        Color32::from_gray(231)
    };
    let widgets = &mut visuals.widgets;
    for widget in [
        &mut widgets.noninteractive,
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
        &mut widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::ZERO;
        widget.bg_fill = control_face();
        widget.weak_bg_fill = control_face();
        widget.bg_stroke = Stroke::new(1.0, theme::border());
        widget.expansion = 0.0;
    }
    widgets.hovered.weak_bg_fill = Color32::from_gray(214);
    widgets.active.weak_bg_fill = Color32::from_gray(170);
    if dark {
        widgets.noninteractive.fg_stroke.color = theme::muted();
        widgets.inactive.fg_stroke.color = theme::text();
        for widget in [&mut widgets.hovered, &mut widgets.active, &mut widgets.open] {
            widget.bg_fill = theme::surface();
            widget.weak_bg_fill = theme::surface();
            widget.bg_stroke.color = theme::blue();
            widget.fg_stroke.color = theme::text();
        }
    }
    context.set_visuals(visuals);
    context.style_mut(|style| {
        style.spacing.button_padding = vec2(7.0, 3.0);
        style.spacing.item_spacing = vec2(5.0, 5.0);
        style.spacing.interact_size.y = 22.0;
        style.spacing.scroll = egui::style::ScrollStyle::solid();
        style.spacing.scroll.bar_width = 15.0;
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(13.0));
    });
}

pub(super) fn bevel(painter: &egui::Painter, rect: Rect, sunken: bool) {
    let shadow = if high_contrast() {
        theme::border().gamma_multiply(0.6)
    } else {
        BLACK
    };
    let (light, dark) = if sunken {
        (shadow, WHITE)
    } else {
        (WHITE, shadow)
    };
    let edge = rect.shrink(0.5);
    painter.hline(edge.x_range(), edge.top(), Stroke::new(1.0, light));
    painter.vline(edge.left(), edge.y_range(), Stroke::new(1.0, light));
    painter.hline(edge.x_range(), edge.bottom(), Stroke::new(1.0, dark));
    painter.vline(edge.right(), edge.y_range(), Stroke::new(1.0, dark));
}

pub(super) fn icons_toggle_label(visible: bool) -> &'static str {
    if visible { "Hide icons" } else { "Show icons" }
}

pub(super) fn button(
    ui: &mut Ui,
    label: impl Into<egui::WidgetText>,
    size: Vec2,
) -> egui::Response {
    let response = ui.add_sized(size, egui::Button::new(label));
    bevel(
        ui.painter(),
        response.rect,
        response.is_pointer_button_down_on(),
    );
    response
}

pub(super) fn list_box(ui: &mut Ui, contents: impl FnOnce(&mut Ui)) {
    let frame = Frame::NONE
        .fill(pane_fill())
        .inner_margin(3)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            contents(ui);
        });
    bevel(ui.painter(), frame.response.rect, true);
}

pub(super) fn group_box(ui: &mut Ui, label: &str, contents: impl FnOnce(&mut Ui)) {
    let frame = Frame::NONE
        .inner_margin(Margin {
            top: 13,
            ..Margin::same(10)
        })
        .outer_margin(Margin {
            top: 7,
            ..Margin::ZERO
        })
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            contents(ui);
        });
    let mut rect = frame.response.rect;
    rect.min.y += 7.0;
    let painter = ui.painter();
    bevel(painter, rect, true);
    let galley = painter.layout_no_wrap(
        label.into(),
        egui::FontId::proportional(13.0),
        theme::text(),
    );
    let position = pos2(rect.left() + 10.0, rect.top() - galley.size().y / 2.0);
    painter.rect_filled(
        Rect::from_min_size(position, galley.size()).expand2(vec2(4.0, 0.0)),
        0,
        theme::sidebar(),
    );
    painter.galley(position, galley, theme::text());
}

pub(super) fn caption_background(painter: &egui::Painter, rect: Rect) {
    if high_contrast() {
        painter.rect_filled(rect, 0, BLACK);
        painter.hline(
            rect.x_range(),
            rect.bottom() - 0.5,
            Stroke::new(1.0, theme::border()),
        );
    } else {
        let blue = Color32::from_rgb(24, 109, 186);
        gradient(painter, rect, [NAVY, blue, blue, NAVY]);
    }
}

pub(super) fn caption_control(
    painter: &egui::Painter,
    rect: Rect,
    index: usize,
    maximized: bool,
    pressed: bool,
) {
    painter.rect_filled(rect, 0, control_face());
    bevel(painter, rect, pressed);
    if !pressed {
        let inner = rect.shrink(1.5);
        painter.hline(
            inner.x_range(),
            inner.bottom(),
            Stroke::new(1.0, Color32::GRAY),
        );
        painter.vline(
            inner.right(),
            inner.y_range(),
            Stroke::new(1.0, Color32::GRAY),
        );
    }
    let origin = rect.min + Vec2::splat(if pressed { 3.0 } else { 2.0 });
    let bounds = Rect::from_min_size(origin, vec2(12.0, 10.0));
    let source = match index {
        0 => icon_svg!("classic-minimize"),
        1 if maximized => icon_svg!("classic-restore"),
        1 => icon_svg!("classic-maximize"),
        _ => icon_svg!("classic-close"),
    };
    paint_svg(painter, bounds, source, control_ink());
}

fn mark(painter: &egui::Painter, rect: Rect) {
    paint_svg(painter, rect, classic_icon_svg!("classic-mark"), WHITE);
}

fn gradient(painter: &egui::Painter, rect: Rect, colors: [Color32; 4]) {
    let mut mesh = egui::Mesh::default();
    for (point, color) in [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ]
    .into_iter()
    .zip(colors)
    {
        mesh.colored_vertex(point, color);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

fn welcome_monitor(painter: &egui::Painter, bounds: Rect) {
    if bounds.width() < 300.0 || bounds.height() < 225.0 {
        return;
    }
    let width = (bounds.width() * 0.40).min(285.0);
    let origin = bounds.right_bottom() - vec2(width + 24.0, width * 0.76 + 18.0);
    let point = |x, y| origin + vec2(x * width, y * width);
    let (face, side, screen) = if high_contrast() {
        (
            Color32::from_white_alpha(30),
            Color32::from_white_alpha(22),
            Color32::from_white_alpha(14),
        )
    } else {
        (
            Color32::from_rgba_unmultiplied(115, 158, 200, 37),
            Color32::from_rgba_unmultiplied(82, 127, 175, 38),
            Color32::from_rgba_unmultiplied(64, 116, 172, 25),
        )
    };
    for (points, color) in [
        ([(0.0, 0.08), (0.73, 0.18), (0.73, 0.63), (0.0, 0.53)], face),
        ([(0.0, 0.08), (0.24, 0.0), (1.0, 0.10), (0.73, 0.18)], face),
        ([(0.73, 0.18), (1.0, 0.10), (1.0, 0.52), (0.73, 0.63)], side),
        (
            [(0.08, 0.16), (0.64, 0.24), (0.64, 0.53), (0.08, 0.45)],
            screen,
        ),
        (
            [(0.24, 0.57), (0.80, 0.65), (0.70, 0.76), (0.14, 0.68)],
            face,
        ),
    ] {
        painter.add(egui::Shape::convex_polygon(
            points.map(|(x, y)| point(x, y)).into(),
            color,
            Stroke::NONE,
        ));
    }
}

impl ReDox {
    pub(super) fn set_win98_theme(&mut self, context: &egui::Context, dark: bool) -> String {
        let id = if dark { DARK_ID } else { ID };
        let Some(pack) = self.ui_catalog.packs.iter().find(|pack| pack.id == id) else {
            return "The requested Windows 98 theme is unavailable.".into();
        };
        if self.active_ui.id != id {
            self.ui_preferences
                .sidebars
                .insert(self.active_ui.id.clone(), self.sidebar_visible);
            self.active_ui = pack.clone();
            self.ui_preferences.ui = id.into();
            self.ui_background = None;
            let mut palette = pack.config.colors().unwrap_or_default();
            if let Some(colors) = self.ui_preferences.text_colors.get(id) {
                palette = colors.apply(palette);
            }
            theme::set_palette(palette);
            apply_style(context);
            context.request_repaint();
        }
        format!("Theme: {}", pack.config.name)
    }

    pub(super) fn win98_title_bar(&mut self, context: &egui::Context) {
        egui::TopBottomPanel::top("win98_window_chrome")
            .exact_height(HEADER_HEIGHT)
            .show_separator_line(false)
            .frame(Frame::NONE.fill(control_face()).inner_margin(3))
            .show(context, |ui| {
                bevel(ui.painter(), ui.max_rect().expand(3.0), false);
                ui.spacing_mut().item_spacing.y = 0.0;
                self.win98_caption(ui, context, true);
                Frame::NONE
                    .inner_margin(Margin {
                        left: 3,
                        right: 1,
                        top: 0,
                        bottom: 0,
                    })
                    .show(ui, |ui| {
                        ui.spacing_mut().interact_size.y = 18.0;
                        ui.spacing_mut().button_padding = vec2(4.0, 1.0);
                        ui.allocate_ui_with_layout(
                            vec2(ui.available_width(), 20.0),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                self.main_menus(ui, context);
                                let terminal = context.viewport_id() == egui::ViewportId::ROOT
                                    && self.ui_preferences.win98_terminal_open;
                                if !terminal || super::win98_terminal::caption_hidden(self) {
                                    let label = icons_toggle_label(
                                        self.ui_preferences.win98_icons_visible(terminal),
                                    );
                                    if button(ui, label, vec2(72.0, 18.0)).clicked() {
                                        self.ui_preferences.toggle_win98_icons(terminal);
                                    }
                                }
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    self.title_ui_picker(ui);
                                });
                            },
                        );
                    });
            });
    }

    pub(super) fn win98_taskbar(&mut self, context: &egui::Context) {
        egui::TopBottomPanel::bottom("win98_taskbar")
            .exact_height(34.0)
            .show_separator_line(false)
            .frame(
                Frame::NONE
                    .fill(theme::rail())
                    .inner_margin(Margin::symmetric(5, 4)),
            )
            .show(context, |ui| {
                let rect = ui.max_rect().expand2(vec2(5.0, 4.0));
                ui.painter()
                    .hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, WHITE));
                ui.horizontal(|ui| {
                    let start = ui
                        .menu_button(RichText::new("       Start").strong(), |ui| {
                            ui.set_min_width(165.0);
                            ui.strong("ReDox");
                            ui.separator();
                            if ui
                                .add_enabled(self.can_new_chat(), egui::Button::new("New chat"))
                                .clicked()
                            {
                                self.dispatch_ui_action(
                                    crate::app::ui_actions::UiAction::NewChat,
                                    context,
                                );
                                ui.close();
                            }
                            for shortcut in WINDOWS.into_iter().chain(SHORTCUTS) {
                                if ui.button(shortcut.label()).clicked() {
                                    shortcut.activate(self, context);
                                    ui.close();
                                }
                            }
                            if ui.button("Usage limits").clicked() {
                                self.open_usage(context);
                                ui.close();
                            }
                        })
                        .response;
                    bevel(ui.painter(), start.rect, start.is_pointer_button_down_on());
                    mark(
                        ui.painter(),
                        Rect::from_center_size(
                            start.rect.left_center() + vec2(13.0, 0.0),
                            vec2(14.0, 12.0),
                        ),
                    );
                    ui.separator();
                    if button(ui, "◀", vec2(25.0, 23.0))
                        .on_hover_text("Previous chat")
                        .clicked()
                    {
                        self.dispatch_ui_action(
                            crate::app::ui_actions::UiAction::Previous,
                            context,
                        );
                    }
                    if button(ui, "▶", vec2(25.0, 23.0))
                        .on_hover_text("Next chat")
                        .clicked()
                    {
                        self.dispatch_ui_action(crate::app::ui_actions::UiAction::Next, context);
                    }
                    ui.separator();
                    for shortcut in WINDOWS {
                        let active = matches!(shortcut, Shortcut::Terminal)
                            == self.ui_preferences.win98_terminal_open;
                        let task = button(ui, shortcut.label(), vec2(88.0, 23.0));
                        if active {
                            bevel(ui.painter(), task.rect, true);
                        }
                        if task.clicked() {
                            shortcut.activate(self, context);
                        }
                    }
                    if super::win98_terminal::is_fullscreen(context)
                        && super::win98_terminal::caption_hidden(self)
                    {
                        let label = icons_toggle_label(self.ui_preferences.win98_icons_visible(true));
                        if button(ui, label, vec2(72.0, 23.0)).clicked() {
                            self.ui_preferences.toggle_win98_icons(true);
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let provider = if self.is_ollama() { "OLLAMA" } else { "CODEX" };
                        let tray = ui.add_sized(
                            [96.0, 23.0],
                            egui::Label::new(RichText::new(format!("▣  {provider}")).small()),
                        );
                        bevel(ui.painter(), tray.rect, true);
                    });
                });
            });
    }

    pub(super) fn win98_desktop(&mut self, context: &egui::Context) {
        if context.content_rect().width() < 750.0 {
            return;
        }
        egui::SidePanel::left("win98_shortcuts")
            .exact_width(100.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(
                Frame::NONE
                    .fill(theme::background())
                    .inner_margin(Margin::symmetric(4, 12)),
            )
            .show(context, |ui| {
                for shortcut in WINDOWS.into_iter().chain(SHORTCUTS) {
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 68.0), Sense::click());
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, shortcut.label())
                    });
                    if response.hovered() || response.has_focus() {
                        ui.painter()
                            .rect_filled(rect.shrink(2.0), 0, selected_fill());
                    }
                    shortcut.paint(ui, pos2(rect.center().x - 16.0, rect.top() + 5.0));
                    ui.painter().text(
                        pos2(rect.center().x, rect.top() + 42.0),
                        egui::Align2::CENTER_TOP,
                        shortcut.label(),
                        egui::FontId::proportional(12.0),
                        WHITE,
                    );
                    if response.clicked() {
                        shortcut.activate(self, context);
                    }
                }
            });
    }

    pub(super) fn win98_conversation(&mut self, context: &egui::Context) {
        let root = context.viewport_id() == egui::ViewportId::ROOT;
        let docked_terminal = root
            && self.ui_preferences.win98_terminal_open
            && self.ui_preferences.win98_terminal_docked;
        let gutter = if root && !docked_terminal {
            Margin {
                top: 10,
                bottom: (context.content_rect().height() * 0.065).clamp(10.0, 52.0) as i8,
                ..Margin::symmetric(18, 0)
            }
        } else {
            Margin::same(4)
        };
        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(theme::background()).inner_margin(gutter))
            .show(context, |ui| {
                if let Some(texture) = &self.ui_background {
                    super::skins::paint_background(
                        ui.painter(),
                        ui.max_rect(),
                        texture,
                        self.active_ui.config.background_opacity,
                    );
                }
                let rect = ui.available_rect_before_wrap();
                if docked_terminal {
                    super::win98_terminal::show(ui, rect, self);
                    return;
                }
                ui.painter().rect_filled(
                    rect.translate(vec2(3.0, 3.0)),
                    0,
                    Color32::from_black_alpha(80),
                );
                ui.painter().rect_filled(rect, 0, theme::sidebar());
                bevel(ui.painter(), rect, false);
                bevel(ui.painter(), rect.shrink(1.0), false);
                let inner = rect.shrink(4.0);
                let mut window = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt(("win98_conversation", context.viewport_id()))
                        .max_rect(inner),
                );
                window.set_clip_rect(ui.clip_rect().intersect(inner));
                let (header_id, header_height) = if root {
                    ("win98_chat_header", 23.0)
                } else {
                    ("win98_branch_header", 27.0)
                };
                egui::TopBottomPanel::top(header_id)
                    .exact_height(header_height)
                    .show_separator_line(false)
                    .frame(Frame::NONE)
                    .show_inside(&mut window, |ui| {
                        if root {
                            let (caption, _) = ui.allocate_exact_size(
                                vec2(ui.available_width(), 20.0),
                                Sense::hover(),
                            );
                            caption_background(ui.painter(), caption);
                            ui.painter().text(
                                caption.left_center() + vec2(6.0, 0.0),
                                egui::Align2::LEFT_CENTER,
                                "Chat",
                                theme::semibold(13.0),
                                WHITE,
                            );
                        } else {
                            self.win98_caption(ui, context, false);
                        }
                    });
                self.composer(context, Some(&mut window));
                egui::CentralPanel::default()
                    .frame(Frame::NONE.fill(pane_fill()))
                    .show_inside(&mut window, |ui| {
                        ui.set_clip_rect(ui.clip_rect().intersect(ui.max_rect()));
                        if let Some(chat) = self
                            .session
                            .active()
                            .filter(|chat| !chat.messages.is_empty())
                        {
                            bevel(ui.painter(), ui.max_rect(), true);
                            self.chat_transcript(ui, ("win98", chat.id), 30.0);
                        } else {
                            self.win98_welcome(ui, context);
                        }
                    });
            });
    }

    fn win98_caption(&self, ui: &mut Ui, context: &egui::Context, root: bool) {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
        let control_width = if root { 56.0 } else { 0.0 };
        let caption = Rect::from_min_max(rect.min, rect.right_bottom() - vec2(control_width, 0.0));
        caption_background(ui.painter(), rect);
        mark(
            ui.painter(),
            Rect::from_center_size(rect.left_center() + vec2(9.0, 0.0), vec2(14.0, 12.0)),
        );
        let title = self
            .session
            .active()
            .filter(|chat| !chat.messages.is_empty())
            .map_or_else(
                || self.active_ui.config.heading.clone(),
                |chat| format!("{} - ReDox", chat.title),
            );
        let galley = ui
            .painter()
            .layout_no_wrap(title, theme::semibold(13.0), WHITE);
        ui.painter().with_clip_rect(caption).galley(
            caption.left_center() + vec2(20.0, -galley.size().y / 2.0),
            galley,
            WHITE,
        );
        if root {
            let drag = ui.interact(caption, ui.id().with("win98_drag"), Sense::click_and_drag());
            window_frame::title_drag(context, &drag);
            let maximized = context.input(|input| input.viewport().maximized.unwrap_or(false));
            for (index, (tooltip, command)) in [
                ("Minimize", egui::ViewportCommand::Minimized(true)),
                (
                    "Maximize / restore",
                    egui::ViewportCommand::Maximized(!maximized),
                ),
                ("Close ReDox", egui::ViewportCommand::Close),
            ]
            .into_iter()
            .enumerate()
            {
                let left = caption.right()
                    + if index == 2 {
                        38.0
                    } else {
                        2.0 + 17.0 * index as f32
                    };
                let bounds = Rect::from_min_size(pos2(left, rect.top() + 3.0), vec2(16.0, 14.0));
                let response = ui
                    .interact(
                        bounds,
                        ui.id().with(("caption_control", index)),
                        Sense::click(),
                    )
                    .on_hover_text(tooltip);
                caption_control(
                    ui.painter(),
                    bounds,
                    index,
                    maximized,
                    response.is_pointer_button_down_on(),
                );
                if response.clicked() {
                    context.send_viewport_cmd(command);
                }
            }
        }
    }

    fn win98_welcome(&mut self, ui: &mut Ui, context: &egui::Context) {
        let rect = ui.max_rect();
        let (sky, horizon, ground, lettering) = if high_contrast() {
            (
                theme::surface(),
                theme::sidebar(),
                pane_fill(),
                theme::blue(),
            )
        } else {
            (
                Color32::from_rgb(175, 208, 239),
                Color32::from_rgb(229, 244, 254),
                WHITE,
                NAVY,
            )
        };
        gradient(ui.painter(), rect, [sky, horizon, ground, ground]);
        welcome_monitor(ui.painter(), rect);
        bevel(ui.painter(), rect, true);
        super::scroll::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                Frame::NONE.inner_margin(20).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (logo, _) = ui.allocate_exact_size(vec2(38.0, 30.0), Sense::hover());
                        mark(ui.painter(), logo);
                        ui.label(RichText::new("ReDox").font(theme::semibold(36.0)));
                        ui.label(RichText::new("98").size(30.0).color(lettering));
                    });
                    ui.label(
                        RichText::new("N A T I V E   W O R K S P A C E")
                            .size(10.0)
                            .color(lettering),
                    );
                    let (stripe, _) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 2.0), Sense::hover());
                    let stripe_colors = if high_contrast() {
                        [WHITE, theme::muted(), theme::border(), theme::surface()]
                    } else {
                        [
                            Color32::RED,
                            Color32::GOLD,
                            Color32::DARK_GREEN,
                            Color32::BLUE,
                        ]
                    };
                    for (index, color) in stripe_colors.into_iter().enumerate() {
                        let width = stripe.width() / 4.0;
                        ui.painter().rect_filled(
                            Rect::from_min_size(
                                stripe.min + vec2(index as f32 * width, 0.0),
                                vec2(width, 2.0),
                            ),
                            0,
                            color,
                        );
                    }
                    ui.add_space(17.0);
                    if ui.available_width() >= 400.0 {
                        ui.horizontal_top(|ui| {
                            let width = (ui.available_width() * 0.32).clamp(128.0, 192.0);
                            ui.allocate_ui_with_layout(
                                vec2(width, 0.0),
                                Layout::top_down(Align::Min),
                                |ui| {
                                    ui.set_width(width);
                                    self.win98_contents(ui, context);
                                },
                            );
                            ui.add_space(15.0);
                            ui.vertical(|ui| self.win98_intro(ui, context));
                        });
                    } else {
                        self.win98_intro(ui, context);
                        ui.add_space(12.0);
                        self.win98_contents(ui, context);
                    }
                });
            });
    }

    fn win98_contents(&mut self, ui: &mut Ui, context: &egui::Context) {
        Frame::NONE
            .fill(BLACK)
            .inner_margin(Margin::symmetric(8, 3))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("C O N T E N T S").color(WHITE).size(10.0));
            });
        for (shortcut, color) in SHORTCUTS.into_iter().zip([
            Color32::from_rgb(36, 119, 219),
            Color32::from_rgb(228, 83, 42),
            Color32::from_rgb(91, 157, 32),
            Color32::from_rgb(228, 174, 27),
        ]) {
            let response = button(
                ui,
                RichText::new(shortcut.label()).strong(),
                vec2(ui.available_width(), 24.0),
            );
            ui.painter().rect_filled(
                Rect::from_min_size(response.rect.min, vec2(3.0, response.rect.height())),
                0,
                color,
            );
            if response.clicked() {
                shortcut.activate(self, context);
            }
        }
    }

    fn win98_intro(&mut self, ui: &mut Ui, context: &egui::Context) {
        ui.label(RichText::new(&self.active_ui.config.heading).font(theme::semibold(25.0)));
        ui.add_space(5.0);
        ui.label("Start a conversation, open a project folder, or connect an Ollama model from the desktop.");
        ui.add_space(8.0);
        ui.label("When you're ready, type a message below. Use Start to switch between tools.");
        ui.add_space(16.0);
        if button(ui, RichText::new("Begin").strong(), vec2(85.0, 27.0)).clicked() {
            self.focus_win98_prompt(context);
        }
    }

    fn focus_win98_prompt(&mut self, context: &egui::Context) {
        if context.viewport_id() == egui::ViewportId::ROOT {
            super::win98_terminal::set_open(self, context, false);
        }
        if let Some(id) = self.prompt_input_id {
            context.memory_mut(|memory| memory.request_focus(id));
        }
        self.focus_prompt = true;
    }
}

#[cfg(test)]
#[path = "../../tests/ui/win98.rs"]
mod tests;
