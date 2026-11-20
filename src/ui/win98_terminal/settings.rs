use eframe::egui::{self, Color32};
use serde::{Deserialize, Serialize};

use crate::app::ReDox;

use super::{crt, terminal_font};

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub(super) enum Caption {
    #[default]
    Classic,
    Solid([u8; 3]),
    Hidden,
}

impl Caption {
    pub(super) fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if value.is_empty() {
            return Some(Self::Hidden);
        }
        if value.eq_ignore_ascii_case("default") {
            return Some(Self::Classic);
        }
        let hex = value.strip_prefix('#').unwrap_or(value);
        if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let color = u32::from_str_radix(hex, 16).ok()?;
        let [_, r, g, b] = color.to_be_bytes();
        Some(Self::Solid([r, g, b]))
    }

    fn setting_text(self) -> String {
        match self {
            Self::Classic => "default".into(),
            Self::Solid([r, g, b]) => format!("#{r:02X}{g:02X}{b:02X}"),
            Self::Hidden => String::new(),
        }
    }

    pub(super) fn paint(self, ui: &egui::Ui, rect: egui::Rect) -> Color32 {
        match self {
            Self::Solid([r, g, b]) => {
                let color = Color32::from_rgb(r, g, b);
                ui.painter().rect_filled(rect, 0, color);
                let [r, g, b, _] = egui::Rgba::from(color).to_array();
                if 0.2126 * r + 0.7152 * g + 0.0722 * b >= 0.179 {
                    Color32::BLACK
                } else {
                    Color32::WHITE
                }
            }
            Self::Classic | Self::Hidden => {
                super::win98::caption_background(ui.painter(), rect);
                Color32::WHITE
            }
        }
    }
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(default)]
pub(in crate::app::ui) struct Appearance {
    pub(super) caption: Caption,
    pub(super) caret_width: u8,
    pub(super) crt: crt::Settings,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            caption: Caption::Classic,
            caret_width: 1,
            crt: crt::Settings::default(),
        }
    }
}

const MAX_CARET_WIDTH: u8 = 6;
const ID: &str = "win98_terminal_settings";

#[derive(Clone)]
struct Draft {
    hex: String,
    caret_width: u8,
    crt: crt::Settings,
    focus_hex: bool,
}

pub(super) fn open(context: &egui::Context, appearance: Appearance) {
    context.data_mut(|data| {
        data.insert_temp(
            egui::Id::new(ID),
            Draft {
                hex: appearance.caption.setting_text(),
                caret_width: appearance.caret_width.min(MAX_CARET_WIDTH),
                crt: appearance.crt,
                focus_hex: true,
            },
        );
    });
}

pub(super) fn is_open(context: &egui::Context) -> bool {
    context.data(|data| data.get_temp::<Draft>(egui::Id::new(ID)).is_some())
}

pub(super) fn close(context: &egui::Context) {
    context.data_mut(|data| data.remove::<Draft>(egui::Id::new(ID)));
}

pub(super) fn show(context: &egui::Context, app: &mut ReDox) {
    let id = egui::Id::new(ID);
    let Some(mut draft) = context.data(|data| data.get_temp::<Draft>(id)) else {
        return;
    };
    let modal = egui::Modal::new(id).show(context, |ui| {
        ui.set_width(345.0);
        ui.heading("Terminal Settings");
        ui.horizontal(|ui| {
            let label = ui.label("Title bar color");
            let field = ui
                .add(egui::TextEdit::singleline(&mut draft.hex).desired_width(108.0))
                .labelled_by(label.id);
            if std::mem::take(&mut draft.focus_hex) {
                field.request_focus();
            }
        });
        let caption = Caption::parse(&draft.hex);
        ui.label("Enter #RRGGBB, use default for the theme color, or leave blank to hide.");
        if caption.is_none() {
            ui.label("Enter six hexadecimal digits, for example #800080.");
        }
        ui.add(egui::Slider::new(&mut draft.caret_width, 0..=MAX_CARET_WIDTH).text("Caret width"));
        ui.label("Set the width to 0 to hide the caret.");
        ui.separator();
        draft.crt.controls(ui);
        let (preview, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 56.0), egui::Sense::hover());
        ui.painter().rect_filled(preview, 0, Color32::BLACK);
        let text_start = draft.crt.text_start(ui);
        let painter = ui.painter().with_clip_rect(preview);
        let sample =
            painter.layout_no_wrap("ReDox Terminal".into(), terminal_font(16.0), Color32::WHITE);
        let origin = preview.left_center() + egui::vec2(8.0, -sample.size().y * 0.5);
        crt::draw_galley(&painter, origin, sample, draft.crt.curvature(preview));
        draft.crt.paint(ui, preview, text_start);
        ui.horizontal(|ui| {
            let apply = ui.add_enabled(caption.is_some(), egui::Button::new("Apply"));
            let cancel = ui.button("Cancel").clicked();
            (apply.clicked().then_some(caption).flatten(), cancel)
        })
        .inner
    });

    let (caption, cancel) = modal.inner;
    if let Some(caption) = caption {
        app.ui_preferences.win98_terminal_appearance = Appearance {
            caption,
            caret_width: draft.caret_width,
            crt: draft.crt,
        };
    }
    if caption.is_some() || cancel || modal.should_close() {
        close(context);
        app.focus_prompt = true;
    } else {
        context.data_mut(|data| data.insert_temp(id, draft));
    }
}

pub(super) fn caret_width(appearance: Appearance) -> f32 {
    f32::from(appearance.caret_width.min(MAX_CARET_WIDTH))
}
