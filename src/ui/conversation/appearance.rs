use eframe::egui::{self, Color32};
use redox_studio::native_appearance::{NativeAppearance, NativeAppearanceProperty};
use std::{
    io::ErrorKind,
    path::PathBuf,
    time::{Duration, Instant},
};

const RELOAD_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone)]
struct AppearanceCache {
    path: PathBuf,
    checked: Option<Instant>,
    appearance: NativeAppearance,
}

fn cache_id() -> egui::Id {
    egui::Id::new("native_conversation_appearance")
}

#[cfg(feature = "design-export")]
pub(in crate::app::ui) fn set_preview_appearance(
    context: &egui::Context,
    appearance: NativeAppearance,
) {
    context.data_mut(|data| {
        data.insert_temp(egui::Id::new("export_appearance_override"), appearance);
    });
}

pub(in crate::app::ui) fn current(context: &egui::Context) -> NativeAppearance {
    #[cfg(feature = "design-export")]
    if let Some(appearance) =
        context.data(|data| data.get_temp(egui::Id::new("export_appearance_override")))
    {
        return appearance;
    }
    let mut cache = context
        .data(|data| data.get_temp::<AppearanceCache>(cache_id()))
        .unwrap_or_else(|| AppearanceCache {
            path: NativeAppearance::path(),
            checked: None,
            appearance: NativeAppearance::default(),
        });
    if cache
        .checked
        .is_none_or(|checked| checked.elapsed() >= RELOAD_INTERVAL)
    {
        match NativeAppearance::load(&cache.path) {
            Ok(appearance) => cache.appearance = appearance,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                cache.appearance = NativeAppearance::default();
            }
            Err(_) => {}
        }
        cache.checked = Some(Instant::now());
        context.data_mut(|data| data.insert_temp(cache_id(), cache.clone()));
    }
    context.request_repaint_after(RELOAD_INTERVAL);
    cache.appearance
}

pub(in crate::app::ui) fn color(
    context: &egui::Context,
    property: NativeAppearanceProperty,
) -> Option<Color32> {
    current(context)
        .color(property)
        .map(|[red, green, blue]| Color32::from_rgb(red, green, blue))
}

pub(super) fn transcript_surface(context: &egui::Context) -> Option<Color32> {
    color(context, NativeAppearanceProperty::TranscriptSurface)
}

#[cfg(all(test, feature = "design-export"))]
#[path = "../../../tests/ui/native_conversation_appearance.rs"]
mod tests;
