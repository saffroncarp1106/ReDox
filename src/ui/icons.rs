use eframe::egui::{
    self, Color32, Painter, Pos2, Rect, Response, Sense, TextureOptions, Ui,
    load::{SizeHint, TexturePoll},
    pos2, vec2,
};

#[derive(Clone, Copy)]
pub enum Icon {
    Back,
    Forward,
    Panel,
    Home,
    Codex,
    Chat,
    Clock,
    Books,
    Grid,
    NewChat,
    Folder,
    Bell,
    Search,
    More,
    Agent,
    Bolt,
    Shield,
    Mic,
    ArrowUp,
    Copy,
    Edit,
    Minimize,
    Maximize,
    Close,
    Chevron,
    Settings,
    Reticle,
    Return,
    Waveform,
    ClaudeMark,
    Stop,
    Check,
    Plus,
    UsageMark,
}

pub fn button(ui: &mut Ui, icon: Icon, tooltip: &str) -> Response {
    let response = button_with_sense(ui, icon, Sense::click()).on_hover_text(tooltip);
    #[cfg(feature = "design-export")]
    if let Some(action) = redox_studio::actions::UiAction::from_label(tooltip) {
        super::design_export::record(ui.ctx(), response.rect, action);
    }
    response
}

pub(super) fn button_with_sense(ui: &mut Ui, icon: Icon, sense: Sense) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(30.0, 30.0), sense);
    if super::theme::is_wii(ui.ctx()) {
        super::wii::paint_surface(ui, &response, false);
    } else if response.hovered() && ui.is_enabled() {
        ui.painter().rect_filled(rect, 7, super::theme::surface());
    }
    let color = if !ui.is_enabled() {
        Color32::from_gray(85)
    } else if response.hovered() {
        super::theme::text()
    } else {
        super::theme::muted()
    };
    paint(ui.painter(), rect.shrink(7.0), icon, color);
    response
}

macro_rules! icon_svg {
    ($name:literal) => {
        egui::include_image!(concat!("assets/icons/", $name, ".svg"))
    };
}
pub(super) use icon_svg;

impl Icon {
    fn source(self) -> egui::ImageSource<'static> {
        match self {
            Self::Back => icon_svg!("back"),
            Self::Forward => icon_svg!("forward"),
            Self::Panel => icon_svg!("panel"),
            Self::Home => icon_svg!("home"),
            Self::Codex => icon_svg!("codex"),
            Self::Chat => icon_svg!("chat"),
            Self::Clock => icon_svg!("clock"),
            Self::Books => icon_svg!("books"),
            Self::Grid => icon_svg!("grid"),
            Self::NewChat => icon_svg!("new-chat"),
            Self::Folder => icon_svg!("folder"),
            Self::Bell => icon_svg!("bell"),
            Self::Search => icon_svg!("search"),
            Self::More => icon_svg!("more"),
            Self::Agent => icon_svg!("agent"),
            Self::Bolt => icon_svg!("bolt"),
            Self::Shield => icon_svg!("shield"),
            Self::Mic => icon_svg!("mic"),
            Self::ArrowUp => icon_svg!("arrow-up"),
            Self::Copy => icon_svg!("copy"),
            Self::Edit => icon_svg!("edit"),
            Self::Minimize => icon_svg!("minimize"),
            Self::Maximize => icon_svg!("maximize"),
            Self::Close => icon_svg!("close"),
            Self::Chevron => icon_svg!("chevron"),
            Self::Settings => icon_svg!("settings"),
            Self::Reticle => icon_svg!("reticle"),
            Self::Return => icon_svg!("return"),
            Self::Waveform => icon_svg!("waveform"),
            Self::ClaudeMark => icon_svg!("claude-mark"),
            Self::Stop => icon_svg!("stop"),
            Self::Check => icon_svg!("check"),
            Self::Plus => icon_svg!("plus"),
            Self::UsageMark => icon_svg!("usage-mark"),
        }
    }
}

pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let square = Rect::from_center_size(rect.center(), egui::Vec2::splat(rect.size().min_elem()));
    paint_svg(painter, square, icon.source(), color);
}

pub(super) fn paint_svg(
    painter: &Painter,
    rect: Rect,
    source: egui::ImageSource<'static>,
    tint: Color32,
) {
    if tint == Color32::TRANSPARENT || !painter.clip_rect().intersects(rect) {
        return;
    }

    // Rasterize at the final pixel size and keep that size independent of subpixel placement.
    let context = painter.ctx();
    let scale = context.pixels_per_point();
    let pixels = (rect.size() * scale).round();
    if pixels.min_elem() < 1.0 {
        return;
    }
    let min = ((rect.center().to_vec2() * scale - pixels * 0.5).round() / scale).to_pos2();
    let bounds = Rect::from_min_size(min, pixels / scale);
    let hint = SizeHint::Size {
        width: pixels.x as u32,
        height: pixels.y as u32,
        maintain_aspect_ratio: false,
    };
    if let Ok(TexturePoll::Ready { texture }) = source.load(context, TextureOptions::NEAREST, hint)
    {
        let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
        painter.image(texture.id, bounds, uv, tint);
    }
}

#[cfg(test)]
#[path = "../../tests/ui/icons.rs"]
mod tests;
