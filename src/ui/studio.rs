use crate::app::ReDox;
use eframe::egui;
use redox_studio::{actions::UiAction, document::Document, presentation::interactive};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum StudioSurface {
    Preview,
    Workspace,
    Welcome,
}

#[derive(Clone)]
struct LiveSkin {
    document: Result<std::sync::Arc<Document>, String>,
    stamp: Option<(std::time::SystemTime, u64)>,
    checked: std::time::Instant,
    warning: Option<String>,
}

#[derive(Clone)]
struct StudioLink {
    previous: Option<std::path::PathBuf>,
    previous_sidebar: bool,
    previous_agents: bool,
    hid_panels: bool,
    linked: Option<std::path::PathBuf>,
    checked: std::time::Instant,
}

impl Default for StudioLink {
    fn default() -> Self {
        Self {
            previous: None,
            previous_sidebar: false,
            previous_agents: false,
            hid_panels: false,
            linked: None,
            checked: std::time::Instant::now() - std::time::Duration::from_secs(1),
        }
    }
}

fn live_link_file() -> std::path::PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".local/share"))
        })
        .unwrap_or_else(std::env::temp_dir)
        .join("ReDox/Studio/live-link.json")
}

fn linked_document(file: &std::path::Path) -> Option<std::path::PathBuf> {
    let modified = std::fs::metadata(file).ok()?.modified().ok()?;
    if modified.elapsed().ok()? > std::time::Duration::from_secs(4) {
        return None;
    }
    let data: serde_json::Value = serde_json::from_slice(&std::fs::read(file).ok()?).ok()?;
    if data["version"].as_u64() != Some(1) {
        return None;
    }
    let path = std::path::PathBuf::from(data["path"].as_str()?);
    if !path.is_absolute()
        || !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("redoxskin"))
    {
        return None;
    }
    Some(path)
}

fn stamp(path: &std::path::Path) -> Option<(std::time::SystemTime, u64)> {
    let metadata = std::fs::metadata(path).ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}

fn has_live_shell(document: &Document) -> bool {
    let Some(page) = document.pages.first() else {
        return false;
    };
    [UiAction::Composer, UiAction::Conversation]
        .into_iter()
        .all(|action| {
            page.nodes.iter().any(|node| {
                redox_studio::presentation::scene::visible(page, node.id)
                    && node.ui_script == [action]
            })
        })
}

fn welcome_source(document: &Document) -> Option<&redox_studio::scenes::Source> {
    document.source.as_ref().filter(|source| {
        source.scene == redox_studio::scenes::SceneId::Welcome
            && redox_studio::scenes::THEMES.contains(&source.theme.as_str())
    })
}

fn welcome_has_composer(document: &Document, viewport: egui::Rect) -> bool {
    use redox_studio::presentation::scene;

    document.pages.first().is_some_and(|page| {
        let bounds = redox_studio::editor::union(
            page.nodes
                .iter()
                .filter(|node| node.parent.is_none() && node.visible)
                .filter_map(|node| page.world_rect(node.id)),
        );
        bounds.is_some_and(|bounds| {
            let (view, surface) = scene::fit(bounds, viewport);
            page.nodes.iter().any(|node| {
                node.ui_script == [UiAction::Composer]
                    && scene::visible(page, node.id)
                    && scene::opacity(page, node.id) > 0.
                    && page.world_rect(node.id).is_some_and(|bounds| {
                        scene::clip_rect(page, node.id, view, surface)
                            .intersect(view.rect(bounds))
                            .is_positive()
                    })
            })
        })
    })
}

impl LiveSkin {
    fn load(path: &std::path::Path) -> Self {
        Self {
            document: Document::load(path).map(std::sync::Arc::new),
            stamp: stamp(path),
            checked: std::time::Instant::now(),
            warning: None,
        }
    }
    fn refresh(&mut self, path: &std::path::Path, force: bool) {
        if !force && self.checked.elapsed() < std::time::Duration::from_millis(500) {
            return;
        }
        self.checked = std::time::Instant::now();
        let updated = stamp(path);
        if !force && self.stamp == updated {
            return;
        }
        match Document::load(path) {
            Ok(document) => {
                self.document = Ok(std::sync::Arc::new(document));
                self.stamp = updated;
                self.warning = None;
            }
            Err(error) => {
                self.warning = Some(if self.document.is_ok() {
                    format!("Live reload: {error}. Keeping the last valid design.")
                } else {
                    format!("Live reload: {error}")
                })
            }
        }
    }
}

fn conversation(ui: &mut egui::Ui, control: &interactive::Control<'_>, app: &mut ReDox) {
    let Some(chat) = app.session.active() else {
        return;
    };
    let chat_id = chat.id;
    let empty = chat.messages.is_empty();
    interactive::content(ui, control, |ui| {
        let mut palette = super::theme::palette();
        if control.node.fill[3] > 0 {
            palette.background = egui::Color32::from_rgba_unmultiplied(
                control.node.fill[0],
                control.node.fill[1],
                control.node.fill[2],
                control.node.fill[3],
            );
        }
        palette.text = control.text_color;
        palette.surface = if ui.visuals().dark_mode {
            egui::Color32::from_gray(42)
        } else {
            egui::Color32::from_gray(240)
        };
        palette.input_background = palette.surface;
        super::theme::with_palette(palette, || {
            if empty {
                ui.label(&control.node.text);
            } else {
                app.chat_transcript(ui, ("skin_conversation", control.response.id, chat_id), 0.);
            }
        });
    });
}

impl ReDox {
    pub(super) fn sync_studio_link(&mut self, ctx: &egui::Context) {
        self.sync_studio_link_from(ctx, &live_link_file());
    }

    fn sync_studio_link_from(&mut self, ctx: &egui::Context, file: &std::path::Path) {
        let key = egui::Id::new("studio_live_link");
        let mut link = ctx
            .data_mut(|data| data.get_temp::<StudioLink>(key))
            .unwrap_or_default();
        if !self.ui_preferences.follow_studio && link.linked.is_none() {
            return;
        }
        if link.checked.elapsed() < std::time::Duration::from_millis(500) {
            return;
        }
        link.checked = std::time::Instant::now();
        let published = self
            .ui_preferences
            .follow_studio
            .then(|| linked_document(file))
            .flatten();
        match published {
            Some(path) if link.linked.as_ref() != Some(&path) => {
                let Ok(document) = Document::load(&path) else {
                    ctx.data_mut(|data| data.insert_temp(key, link));
                    ctx.request_repaint_after(std::time::Duration::from_millis(500));
                    return;
                };
                if document.source.is_some() && welcome_source(&document).is_none() {
                    ctx.data_mut(|data| data.insert_temp(key, link));
                    ctx.request_repaint_after(std::time::Duration::from_millis(500));
                    return;
                }
                if link.linked.is_none() {
                    link.previous = self.ui_preferences.studio_skin.clone();
                }
                let should_hide_panels = document.source.is_none() && has_live_shell(&document);
                if should_hide_panels {
                    if !link.hid_panels {
                        link.previous_sidebar = self.sidebar_visible;
                        link.previous_agents = self.ui_preferences.agents_open;
                    }
                    self.sidebar_visible = false;
                    self.ui_preferences.agents_open = false;
                } else if link.hid_panels {
                    self.sidebar_visible = link.previous_sidebar;
                    self.ui_preferences.agents_open = link.previous_agents;
                }
                link.hid_panels = should_hide_panels;
                self.ui_preferences.studio_skin = Some(path.clone());
                link.linked = Some(path);
            }
            None if link.linked.is_some() => {
                if self.ui_preferences.studio_skin == link.linked {
                    self.ui_preferences.studio_skin = link.previous.take();
                    if link.hid_panels {
                        self.sidebar_visible = link.previous_sidebar;
                        self.ui_preferences.agents_open = link.previous_agents;
                    }
                }
                link.linked = None;
                link.hid_panels = false;
            }
            _ => {}
        }
        ctx.data_mut(|data| data.insert_temp(key, link));
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }
    pub(super) fn studio_surface(&self, ctx: &egui::Context) -> Option<StudioSurface> {
        let path = self.ui_preferences.studio_skin.as_ref()?;
        let key = egui::Id::new(("studio_document", path));
        let mut cached = ctx
            .data_mut(|data| data.get_temp::<LiveSkin>(key))
            .unwrap_or_else(|| LiveSkin::load(path));
        cached.refresh(path, false);
        let surface = match &cached.document {
            Ok(document) => {
                if let Some(source) = welcome_source(document) {
                    let builtin_sidebar = source.theme != "win98";
                    (source.theme == self.active_ui.id
                        && welcome_has_composer(document, ctx.content_rect())
                        && !self.ui_preferences.agents_open
                        && self.sidebar_visible == builtin_sidebar
                        && self
                            .session
                            .active()
                            .is_none_or(|chat| chat.messages.is_empty()))
                    .then_some(StudioSurface::Welcome)
                } else if has_live_shell(document) {
                    Some(StudioSurface::Workspace)
                } else {
                    Some(StudioSurface::Preview)
                }
            }
            Err(_) => Some(StudioSurface::Preview),
        };
        ctx.data_mut(|data| data.insert_temp(key, cached));
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
        surface
    }

    pub(in crate::app) fn load_studio_skin(&mut self, path: std::path::PathBuf) {
        match Document::load(&path) {
            Ok(document) => {
                self.ui_preferences.studio_skin = Some(path);
                self.ui_preferences.follow_studio = false;
                if document.source.is_none() && has_live_shell(&document) {
                    self.sidebar_visible = false;
                    self.ui_preferences.agents_open = false;
                }
                self.ui_error = None;
            }
            Err(error) => {
                self.ui_error = Some(error);
                self.settings_page = super::SettingsPage::Appearance;
                self.settings_open = true;
            }
        }
    }

    pub(super) fn studio_skin_settings(&mut self, ui: &mut egui::Ui) {
        ui.label("Active Studio skin (.redoxskin)");
        super::theme::checkbox(ui, &mut self.ui_preferences.follow_studio, "Follow Studio live")
            .on_hover_text(
                "Show the design linked from Studio and refresh it as Studio saves changes.",
            );
        ui.horizontal(|ui| {
            if ui.button("Load skin…").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("Studio skin", &["redoxskin"])
                    .pick_file()
            {
                self.load_studio_skin(path);
            }
            if ui.button("Use built-in UI").clicked() {
                self.ui_preferences.studio_skin = None;
                self.ui_preferences.follow_studio = false;
            }
        });
        ui.label("Complete skins replace the workspace. Skins without a live conversation and composer open as previews. Studio saves reload automatically. Press Ctrl+Shift+U to return to the built-in UI.");
    }

    fn studio_model_choices(&mut self, ui: &mut egui::Ui) {
        ui.add_enabled_ui(self.running.is_none(), |ui| {
            if self.is_ollama() {
                let models = self
                    .ollama_models
                    .iter()
                    .map(|m| m.name.clone())
                    .collect::<Vec<_>>();
                if let Some(chat) = self.session.active_mut() {
                    for model in models {
                        if ui
                            .selectable_value(&mut chat.ollama_model, model.clone(), model)
                            .clicked()
                        {
                            ui.close();
                        }
                    }
                }
            } else {
                let mut models = vec![redox::domain::DEFAULT_MODEL.to_owned()];
                models.extend(
                    self.models
                        .iter()
                        .filter(|name| name.as_str() != redox::domain::DEFAULT_MODEL)
                        .cloned(),
                );
                for model in models {
                    if ui
                        .selectable_value(
                            &mut self.session.model,
                            model.clone(),
                            super::controls::model_label(&model),
                        )
                        .clicked()
                    {
                        ui.close();
                    }
                }
            }
        });
    }

    fn studio_effort_choices(&mut self, ui: &mut egui::Ui) {
        if self.is_ollama() {
            ui.label("Ollama model settings");
            return;
        }
        let Some(model) = self.selected_model_info().cloned() else {
            ui.label("Refresh models to load reasoning levels");
            return;
        };
        ui.add_enabled_ui(self.running.is_none(), |ui| {
            if ui
                .selectable_label(
                    !self.session.model_efforts.contains_key(&model.model),
                    "Model default",
                )
                .clicked()
            {
                self.session.model_efforts.remove(&model.model);
                ui.close();
            }
            for option in &model.supported_reasoning_efforts {
                let selected =
                    self.session.model_efforts.get(&model.model) == Some(&option.reasoning_effort);
                if ui
                    .selectable_label(selected, &option.reasoning_effort)
                    .on_hover_text(&option.description)
                    .clicked()
                {
                    self.session
                        .model_efforts
                        .insert(model.model.clone(), option.reasoning_effort.clone());
                    ui.close();
                }
            }
        });
    }

    pub(super) fn studio_controls(&mut self, ctx: &egui::Context, surface: StudioSurface) {
        let Some(path) = self.ui_preferences.studio_skin.clone() else {
            return;
        };
        let key = egui::Id::new(("studio_document", &path));
        let mut cached = ctx
            .data_mut(|d| d.get_temp::<LiveSkin>(key))
            .unwrap_or_else(|| LiveSkin::load(&path));
        cached.refresh(&path, false);
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
        let mut calls = vec![];
        let mut submitted = false;
        let full = surface != StudioSurface::Preview;
        let focus_prompt = full && std::mem::take(&mut self.focus_prompt);
        let mut render = |ui: &mut egui::Ui| {
            if let Some(warning) = &cached.warning {
                ui.colored_label(egui::Color32::LIGHT_RED, warning);
            }
            match &cached.document {
                Ok(document) => {
                    let model = if self.is_ollama() {
                        self.session
                            .active()
                            .map(|chat| chat.ollama_model.clone())
                            .unwrap_or_default()
                    } else {
                        super::controls::model_label(&self.session.model)
                    };
                    let effort = self.selected_effort().unwrap_or_else(|| "Default".into());
                    let combined = format!("{} / {}", model, effort);
                    let values = interactive::Values {
                        model: Some(&model),
                        effort: Some(&effort),
                        model_effort: Some(&combined),
                    };
                    let mut native = |ui: &mut egui::Ui, control: &interactive::Control<'_>| {
                        match control.action {
                            UiAction::Composer => {
                                if let Some(chat) = self.session.active_mut() {
                                    let result = interactive::composer(
                                        ui,
                                        control,
                                        &mut chat.draft,
                                        egui::Id::new(chat.id),
                                        focus_prompt,
                                    );
                                    if result.submitted {
                                        submitted = true;
                                    }
                                }
                            }
                            UiAction::Conversation => {
                                conversation(ui, control, self);
                            }
                            UiAction::Model | UiAction::ModelEffort => {
                                egui::Popup::menu(&control.response).width(220.).show(|ui| {
                                    self.studio_model_choices(ui);
                                    if control.action == UiAction::ModelEffort {
                                        ui.separator();
                                        self.studio_effort_choices(ui);
                                    }
                                });
                            }
                            UiAction::Effort => {
                                egui::Popup::menu(&control.response)
                                    .width(200.)
                                    .show(|ui| self.studio_effort_choices(ui));
                            }
                            UiAction::Menus => {
                                egui::Popup::menu(&control.response)
                                    .show(|ui| self.main_menus(ui, ctx));
                            }
                            UiAction::WindowDrag => {
                                super::window_frame::title_drag(ctx, &control.response);
                            }
                            _ => {}
                        }
                    };
                    calls = if full {
                        interactive::show_fitted_with_values(ui, document, values, &mut native)
                    } else {
                        interactive::show_with_values(ui, document, values, &mut native)
                    };
                }
                Err(error) => {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
            }
        };
        let mut open = true;
        if full {
            let frame = if surface == StudioSurface::Welcome {
                egui::Frame::NONE.fill(super::theme::background())
            } else {
                egui::Frame::NONE
            };
            egui::CentralPanel::default()
                .frame(frame)
                .show(ctx, &mut render);
        } else {
            egui::Window::new(
                "Studio skin preview — add a conversation and composer binding to activate",
            )
            .id(egui::Id::new("studio_controls"))
            .open(&mut open)
            .default_width(720.)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, &mut render);
            });
        }
        ctx.data_mut(|d| d.insert_temp(key, cached));
        if !open {
            self.ui_preferences.studio_skin = None;
        }
        if submitted {
            calls.push(UiAction::Send);
        }
        for action in calls {
            self.dispatch_ui_action(action, ctx);
        }
    }
}

#[cfg(test)]
#[path = "../../tests/ui/studio.rs"]
mod tests;
