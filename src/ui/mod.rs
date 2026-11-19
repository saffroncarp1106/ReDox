use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::app::ReDox;

mod about;
mod activity;
mod agents;
mod capture;
mod claude;
pub(super) mod clipboard;
mod controls;
mod conversation;
#[cfg(feature = "design-export")]
pub(super) mod design_export;
pub(super) mod desktop_overlay;
mod dialogs;
mod gemini;
mod icons;
mod markdown;
mod media;
pub(super) mod packs;
mod picker;
pub(super) mod project_window;
mod queue;
mod renderers;
mod resources;
mod response_text;
mod scroll;
mod settings;
mod shell;
pub(super) mod skins;
pub(super) mod startup;
mod studio;
pub(super) mod theme;
mod updates;
mod usage;
pub(super) mod wii;
mod win98;
mod window_frame;

pub(super) const PREFERENCES_KEY: &str = "redox.ui.v1";
pub(super) use about::About;

pub(super) fn request_animation_frame(context: &egui::Context) {
    use std::time::Duration;

    // egui subtracts its predicted frame time from delayed repaint requests.
    let predicted = context.input(|input| input.predicted_dt);
    let frame = Duration::from_secs_f64(1.0 / 60.0);
    context.request_repaint_after(frame + Duration::from_secs_f32(predicted));
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub(super) struct Preferences {
    pub studio_skin: Option<std::path::PathBuf>,
    pub follow_studio: bool,
    pub hide_update_prompts: bool,
    pub discord_presence: redox::discord_presence::Settings,
    pub codex_mode: bool,
    pub project_glow: project_window::Glow,
    pub text_colors: std::collections::BTreeMap<String, theme::TextColors>,
    pub ui: String,
    pub sidebars: std::collections::BTreeMap<String, bool>,
    pub zoom: f32,
    pub sidebar_visible: bool,
    pub startup: Option<startup::Config>,
    pub agents_open: bool,
    pub agent_task: String,
    pub floating_windows: bool,
    pub win98_unlocked: bool,
    pub wii: wii::Settings,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            studio_skin: None,
            follow_studio: true,
            hide_update_prompts: false,
            discord_presence: redox::discord_presence::Settings::default(),
            codex_mode: false,
            project_glow: project_window::Glow::default(),
            text_colors: std::collections::BTreeMap::new(),
            ui: "codex".into(),
            sidebars: std::collections::BTreeMap::new(),
            zoom: 1.0,
            sidebar_visible: true,
            startup: None,
            agents_open: false,
            agent_task: String::new(),
            floating_windows: true,
            win98_unlocked: false,
            wii: wii::Settings::default(),
        }
    }
}

impl Preferences {
    pub fn normalize(&mut self) {
        self.win98_unlocked |= self.ui == win98::ID || self.sidebars.contains_key(win98::ID);
        self.zoom = if self.zoom.is_finite() {
            self.zoom.clamp(0.8, 1.5)
        } else {
            1.0
        };
    }

    pub fn shows_ui(&self, id: &str) -> bool {
        id != win98::ID || self.win98_unlocked
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum SettingsPage {
    #[default]
    Account,
    Models,
    Workspace,
    Resources,
    Appearance,
    Cursors,
    Advanced,
    Updates,
    About,
}

impl ReDox {
    pub(super) fn draw_ui(&mut self, context: &egui::Context) {
        use studio::StudioSurface;

        self.sync_studio_link(context);
        let mut palette = self.active_ui.config.colors().unwrap_or_default();
        if let Some(colors) = self.ui_preferences.text_colors.get(&self.active_ui.id) {
            palette = colors.apply(palette);
        }

        if self.active_ui.config.layout == wii::ID && !self.ui_preferences.wii.motion {
            self.startup.finish();
        }

        palette = palette.with_native_appearance(
            &conversation::appearance::current(context),
            &self.active_ui.id,
        );
        theme::update_colors(context, palette);
        if self.ui_preferences.studio_skin.is_some()
            && context.input(|input| {
                input.modifiers.ctrl && input.modifiers.shift && input.key_pressed(egui::Key::U)
            })
        {
            self.ui_preferences.studio_skin = None;
            self.ui_preferences.follow_studio = false;
        }
        let studio_surface = self.studio_surface(context);
        if let Some(surface @ (StudioSurface::Workspace | StudioSurface::Welcome)) = studio_surface {
            if surface == StudioSurface::Workspace {
                self.title_bar(context);
                self.agents_panel(context);
                if self.sidebar_visible {
                    self.sidebar(context);
                }
            }
            self.studio_controls(context, surface);
            self.rename_dialog(context);
            self.settings(context);
            self.approval_dialog(context);
            self.usage_dialog(context);
            self.window_interactions(context);
            self.ui_drop_hint(context);
            self.project_windows(context);
            self.capture_frame(context);
            self.update_prompt(context);
            return;
        }
        let renderer = renderers::resolve(&self.active_ui.config.layout);
        let windows_desktop = renderer.id == win98::ID;

        if windows_desktop {
            self.win98_title_bar(context);
            self.win98_taskbar(context);
        } else if renderer.id == wii::ID {
            self.wii_title_bar(context);
        } else {
            self.title_bar(context);
        }

        if renderer.id == wii::ID {
            (renderer.rail)(self, context);
        }
        self.agents_panel(context);
        if !windows_desktop && renderer.id != wii::ID {
            (renderer.rail)(self, context);
        }
        if self.sidebar_visible
            && (!self.ui_preferences.agents_open || context.viewport_rect().width() >= 1280.0)
        {
            self.sidebar(context);
        }
        if windows_desktop {
            (renderer.rail)(self, context);
        }

        (renderer.conversation)(self, context);

        if studio_surface == Some(StudioSurface::Preview) {
            self.studio_controls(context, StudioSurface::Preview);
        }
        self.rename_dialog(context);
        self.settings(context);
        self.approval_dialog(context);
        self.usage_dialog(context);

        self.window_interactions(context);
        self.startup.draw(
            context,
            if windows_desktop {
                win98::HEADER_HEIGHT
            } else if renderer.id == wii::ID {
                48.0
            } else {
                42.0
            },
        );

        self.ui_drop_hint(context);
        self.project_windows(context);
        self.capture_frame(context);
        self.update_prompt(context);
        if renderer.id == wii::ID {
            self.wii_feedback(context);
        }
    }
}
