use eframe::egui;

use crate::app::ReDox;

pub(in crate::app) struct Renderer {
    pub id: &'static str,
    pub config: &'static str,
    pub rail: fn(&mut ReDox, &egui::Context),
    pub conversation: fn(&mut ReDox, &egui::Context),
}

pub(in crate::app) const RENDERERS: &[Renderer] = &[
    Renderer {
        id: super::win98::ID,
        config: include_str!("layouts/win98.json"),
        rail: ReDox::win98_desktop,
        conversation: ReDox::win98_conversation,
    },
    Renderer {
        id: super::wii::ID,
        config: include_str!("layouts/wii.json"),
        rail: ReDox::wii_rail,
        conversation: ReDox::wii_conversation,
    },
    Renderer {
        id: "codex",
        config: include_str!("layouts/codex.json"),
        rail: ReDox::navigation_rail,
        conversation: ReDox::conversation,
    },
];

pub(super) fn resolve(id: &str) -> &'static Renderer {
    RENDERERS
        .iter()
        .find(|renderer| renderer.id == id)
        .unwrap_or(&RENDERERS[0])
}

#[cfg(test)]
#[path = "../../tests/ui/renderers.rs"]
mod tests;
