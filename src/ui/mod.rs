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
pub(super) mod desktop_overlay;
mod dialogs;
mod gemini;
mod icons;
mod markdown;
pub(super) mod media;
mod message_edit;
pub(super) mod packs;
mod picker;
pub(super) mod project_window;
mod queue;
pub(super) mod renderers;
mod resources;
mod response_text;
mod scroll;
mod settings;
mod shell;
pub(super) mod skins;
pub(super) mod startup;
pub(super) mod theme;
mod transcript;
mod typography;
mod updates;
mod usage;
pub(super) mod wii;
mod win98;
mod win98_terminal;
mod window_frame;

pub(super) const PREFERENCES_KEY: &str = "redox.ui.v1";
pub(super) use about::About;

const DEFAULT_SAFFYOS_FOLDER: &str = r"C:\Users\llkin\Documents\ReDoxWorkingDirectory\SaffyOSMem";

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
    saffyos_folder: String,
    saffyos_model: String,
    pub agent_task: String,
    pub floating_windows: bool,
    pub win98_unlocked: bool,
    win98_chat_icons_visible: bool,
    win98_terminal_icons_visible: Option<bool>,
    pub win98_terminal_open: bool,
    pub win98_terminal_docked: bool,
    pub win98_terminal_zoom: u8,
    win98_terminal_appearance: win98_terminal::Appearance,
    pub wii: wii::Settings,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
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
            saffyos_folder: DEFAULT_SAFFYOS_FOLDER.into(),
            saffyos_model: String::new(),
            agent_task: String::new(),
            floating_windows: true,
            win98_unlocked: false,
            win98_chat_icons_visible: true,
            win98_terminal_icons_visible: None,
            win98_terminal_open: false,
            win98_terminal_docked: false,
            win98_terminal_zoom: 125,
            win98_terminal_appearance: Default::default(),
            wii: wii::Settings::default(),
        }
    }
}

impl Preferences {
    fn win98_icons_visible(&self, terminal: bool) -> bool {
        if terminal {
            self.win98_terminal_icons_visible
                .unwrap_or(self.win98_terminal_docked)
        } else {
            self.win98_chat_icons_visible
        }
    }

    fn toggle_win98_icons(&mut self, terminal: bool) {
        let visible = !self.win98_icons_visible(terminal);
        if terminal {
            self.win98_terminal_icons_visible = Some(visible);
        } else {
            self.win98_chat_icons_visible = visible;
        }
    }

    pub fn normalize(&mut self) {
        self.win98_unlocked |= win98::is_builtin(&self.ui)
            || [win98::ID, win98::DARK_ID]
                .iter()
                .any(|id| self.sidebars.contains_key(*id));
        self.zoom = if self.zoom.is_finite() {
            self.zoom.clamp(0.8, 1.5)
        } else {
            1.0
        };
    }

    pub fn shows_ui(&self, id: &str) -> bool {
        !win98::is_builtin(id) || self.win98_unlocked
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum SettingsPage {
    #[default]
    Account,
    Models,
    Workspace,
    SaffyOs,
    Resources,
    Appearance,
    Cursors,
    Advanced,
    Updates,
    About,
}

impl ReDox {
    pub(super) fn terminal_is_open(&self, context: &egui::Context) -> bool {
        context.viewport_id() == egui::ViewportId::ROOT
            && self.ui_preferences.win98_terminal_open
            && renderers::resolve(&self.active_ui.config.layout).id == win98::ID
    }

    pub(super) fn draw_ui(&mut self, context: &egui::Context) {
        if self.message_edit.is_some() && !self.is_editing_message() {
            self.close_message_edit(context);
        }
        let mut palette = self.active_ui.config.colors().unwrap_or_default();
        if let Some(colors) = self.ui_preferences.text_colors.get(&self.active_ui.id) {
            palette = colors.apply(palette);
        }

        if self.active_ui.config.layout == wii::ID && !self.ui_preferences.wii.motion {
            self.startup.finish();
        }

        theme::update_colors(context, palette);
        let renderer = renderers::resolve(&self.active_ui.config.layout);
        let windows_desktop = renderer.id == win98::ID;
        let terminal_fullscreen =
            win98_terminal::fullscreen_for_frame(self, context, windows_desktop);
        let terminal_open = windows_desktop
            && context.viewport_id() == egui::ViewportId::ROOT
            && self.ui_preferences.win98_terminal_open;
        if terminal_open && (terminal_fullscreen || !self.ui_preferences.win98_terminal_docked) {
            if !terminal_fullscreen && win98_terminal::caption_hidden(self) {
                self.win98_title_bar(context);
            }
            if terminal_fullscreen {
                self.win98_taskbar(context);
            }
            let show_sidebars = if terminal_fullscreen {
                win98_terminal::sidebar_override(context) == Some(true)
            } else {
                win98_terminal::sidebars_visible(self, context)
            };
            if show_sidebars {
                self.agents_panel(context);
                if !self.ui_preferences.agents_open || context.content_rect().width() >= 1280.0 {
                    self.sidebar(context);
                }
            }
            if self.ui_preferences.win98_icons_visible(true) {
                (renderer.rail)(self, context);
            }
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(egui::Color32::BLACK))
                .show(context, |ui| {
                    win98_terminal::show(ui, ui.available_rect_before_wrap(), self);
                });
        } else {
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
            let terminal_sidebars = if terminal_open {
                win98_terminal::sidebar_override(context)
            } else {
                None
            };
            if terminal_sidebars != Some(false) {
                self.agents_panel(context);
            }
            if !windows_desktop && renderer.id != wii::ID {
                (renderer.rail)(self, context);
            }
            if terminal_sidebars.unwrap_or(self.sidebar_visible)
                && (!self.ui_preferences.agents_open || context.viewport_rect().width() >= 1280.0)
            {
                self.sidebar(context);
            }
            if windows_desktop && self.ui_preferences.win98_icons_visible(terminal_open) {
                (renderer.rail)(self, context);
            }

            if !windows_desktop
                || !self.ui_preferences.win98_terminal_open
                || self.ui_preferences.win98_terminal_docked
                || context.viewport_id() != egui::ViewportId::ROOT
            {
                (renderer.conversation)(self, context);
            }
        }

        self.rename_dialog(context);
        self.settings(context);
        self.approval_dialog(context);
        self.usage_dialog(context);

        if !terminal_fullscreen {
            self.window_interactions(context);
        }
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
