use crate::app::ui_actions::UiAction;
use eframe::egui;
use redox::codex::ModelInfo;
use redox::domain::{DEFAULT_MODEL, Mode, Provider};
use uuid::Uuid;

use crate::app::{ReDox, SaffyosLoop, SaffyosStep};

use super::super::SettingsPage;

const ACCESS_MODES: [(&str, Mode); 3] = [
    ("full", Mode::FullAccess),
    ("workspace", Mode::Code),
    ("read-only", Mode::Chat),
];

const COMMANDS: &[(&str, &str, &str)] = &[
    ("/help", " [command]", "List commands or show their syntax"),
    ("/saffyOS", "", "Open the default SaffyOS workspace"),
    ("/saffyOS project", "", "Choose a project folder"),
    (
        "/saffyOS locked",
        "",
        "Open a permanently locked SaffyOS chat",
    ),
    (
        "/saffyOS loop",
        "",
        "Run varied follow-ups in a locked SaffyOS chat",
    ),
    (
        "/ollama",
        " [use]",
        "Open Ollama Agents or use the selected model",
    ),
    ("/newchat", "", "Start a new chat"),
    (
        "/settings",
        " [page]",
        "Open Terminal appearance or an app settings page",
    ),
    ("/usage check", "", "Open usage limits"),
    ("/model", " [id]", "List or select a model"),
    (
        "/model effort",
        " [level]",
        "Show or set the current model's reasoning effort",
    ),
    ("/codex", "", "Switch this chat to Codex"),
    ("/chats", "", "Open the chat list"),
    ("/history", "", "Open Codex history"),
    ("/previous", "", "Switch to the previous chat"),
    ("/next", "", "Switch to the next chat"),
    ("/rename", "", "Rename this chat"),
    ("/project", "", "Choose a project folder"),
    (
        "/access",
        " [full|workspace|read-only]",
        "Show or set access for this chat",
    ),
    ("/attach", "", "Attach files"),
    ("/stop", "", "Stop the current response"),
    ("/sidebar", "", "Toggle sidebars alongside Terminal"),
    (
        "/dock",
        "",
        "Switch between the Chat pane and expanded Terminal",
    ),
    ("/fullscreen", "", "Toggle Terminal fullscreen"),
    ("/dark", "", "Use the Windows 98 Dark theme"),
    ("/light", "", "Use the classic Windows 98 theme"),
    ("/theme", " [dark|light]", "Show or change the Windows 98 theme"),
    ("/chat", "", "Return to Chat"),
];

const SETTINGS_PAGES: &[(&str, SettingsPage)] = &[
    ("account", SettingsPage::Account),
    ("models", SettingsPage::Models),
    ("workspace", SettingsPage::Workspace),
    ("saffyos", SettingsPage::SaffyOs),
    ("resources", SettingsPage::Resources),
    ("appearance", SettingsPage::Appearance),
    ("cursors", SettingsPage::Cursors),
    ("advanced", SettingsPage::Advanced),
    ("updates", SettingsPage::Updates),
    ("about", SettingsPage::About),
];

pub(super) struct Suggestion {
    pub input: String,
    pub description: &'static str,
}

pub(super) fn is_command(draft: &str) -> bool {
    let draft = draft.trim();
    let Some(command) = draft.strip_prefix('/') else {
        let (name, args) = command_parts(draft);
        return !draft.contains(['\n', '\r'])
            && is_saffyos(name)
            && SaffyosCommand::parse(args).is_some();
    };
    !command.contains(['\n', '\r'])
        && command.split_whitespace().next().is_none_or(|name| {
            name.bytes()
                .all(|byte| byte.is_ascii_alphabetic() || byte == b'-')
        })
}

fn command_parts(input: &str) -> (&str, &str) {
    let command = input.strip_prefix('/').unwrap_or(input).trim_start();
    let (name, args) = command
        .split_once(char::is_whitespace)
        .unwrap_or((command, ""));
    (name, args.trim())
}

fn is_saffyos(name: &str) -> bool {
    name.eq_ignore_ascii_case("saffyos") || name.eq_ignore_ascii_case("saffos")
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SaffyosCommand {
    Workspace,
    Project,
    Locked,
    Loop,
}

impl SaffyosCommand {
    fn parse(args: &str) -> Option<Self> {
        match args.to_ascii_lowercase().as_str() {
            "" => Some(Self::Workspace),
            "project" => Some(Self::Project),
            "locked" | "locked forever" => Some(Self::Locked),
            "loop" => Some(Self::Loop),
            _ => None,
        }
    }
}

pub(super) fn suggestions(app: &ReDox, draft: &str) -> Vec<Suggestion> {
    if !is_command(draft) {
        return Vec::new();
    }
    let prefix = draft.trim_start();
    if let Some((command, args)) = prefix.split_once(char::is_whitespace) {
        let args = args.trim_start();
        match command.to_ascii_lowercase().as_str() {
            "/model" => {
                if let Some((part, level)) = args.split_once(char::is_whitespace)
                    && part.eq_ignore_ascii_case("effort")
                {
                    return effort_suggestions(app, "/model effort", level.trim_start());
                }
                let mut models = available_models(app);
                if !app.is_ollama() {
                    models.insert(0, "effort");
                }
                return complete_values("/model", args, models, "Model or model setting");
            }
            "/effort" => return effort_suggestions(app, "/effort", args),
            "/settings" | "/setting" => {
                let pages =
                    std::iter::once("terminal").chain(SETTINGS_PAGES.iter().map(|&(name, _)| name));
                return complete_values("/settings", args, pages, "Settings page");
            }
            command if is_saffyos(command.trim_start_matches('/')) => {
                let prefix = format!("/saffyOS {args}").to_ascii_lowercase();
                return COMMANDS
                    .iter()
                    .filter_map(|&(command, _, description)| {
                        command
                            .to_ascii_lowercase()
                            .starts_with(&prefix)
                            .then(|| Suggestion {
                                input: command.into(),
                                description,
                            })
                    })
                    .collect();
            }
            "/ollama" => {
                return complete_values("/ollama", args, ["use"], "Use the selected Ollama model");
            }
            "/access" => {
                return complete_values(
                    "/access",
                    args,
                    ACCESS_MODES.iter().map(|&(name, _)| name),
                    "Chat access mode",
                );
            }
            "/theme" => {
                return complete_values("/theme", args, ["dark", "light"], "Windows 98 theme");
            }
            "/help" => {
                let names = COMMANDS
                    .iter()
                    .map(|(name, _, _)| name.trim_start_matches('/'));
                return complete_values(
                    "/help",
                    args.trim_start_matches('/'),
                    names,
                    "Show command syntax",
                );
            }
            _ => {}
        }
    }
    COMMANDS
        .iter()
        .filter(|(name, _, _)| {
            name.len() > prefix.len()
                && name
                    .get(..prefix.len())
                    .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
        })
        .map(|&(name, _, description)| Suggestion {
            input: name.into(),
            description,
        })
        .take(6)
        .collect()
}

fn complete_values<'a>(
    command: &str,
    typed: &str,
    choices: impl IntoIterator<Item = &'a str>,
    description: &'static str,
) -> Vec<Suggestion> {
    choices
        .into_iter()
        .filter(|choice| {
            *choice != typed
                && choice
                    .get(..typed.len())
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case(typed))
        })
        .take(6)
        .map(|choice| Suggestion {
            input: format!("{command} {choice}"),
            description,
        })
        .collect()
}

fn effort_suggestions(app: &ReDox, command: &str, typed: &str) -> Vec<Suggestion> {
    if !app.is_ollama()
        && let Some(model) = app.selected_model_info()
    {
        return complete_values(
            command,
            typed,
            available_efforts(model),
            "Supported reasoning effort",
        );
    }
    Vec::new()
}

fn available_models(app: &ReDox) -> Vec<&str> {
    if app.is_ollama() {
        app.ollama_models
            .iter()
            .map(|model| model.name.as_str())
            .collect()
    } else {
        std::iter::once("default")
            .chain(
                app.models
                    .iter()
                    .map(String::as_str)
                    .filter(|name| *name != DEFAULT_MODEL),
            )
            .collect()
    }
}

fn available_efforts(model: &ModelInfo) -> impl Iterator<Item = &str> {
    std::iter::once("default").chain(
        model
            .supported_reasoning_efforts
            .iter()
            .map(|option| option.reasoning_effort.as_str()),
    )
}

fn help(args: &str) -> String {
    let name = args
        .trim_start_matches('/')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    let name = match name.as_str() {
        "effort" => "model effort",
        "setting" => "settings",
        "usage" => "usage check",
        "saffos" => "saffyos",
        "saffos project" => "saffyos project",
        "saffos locked" | "saffos locked forever" | "saffyos locked forever" => "saffyos locked",
        "saffos loop" => "saffyos loop",
        other => other,
    };
    if name.is_empty() {
        return COMMANDS
            .iter()
            .map(|(command, parameters, description)| {
                let usage = format!("{command}{parameters}");
                format!("{usage:<25} {description}")
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    COMMANDS
        .iter()
        .find(|(command, _, _)| command.strip_prefix('/') == Some(name))
        .map_or_else(
            || format!("Unknown command /{name}. Type /help for the list."),
            |(command, parameters, description)| format!("{command}{parameters}\n{description}"),
        )
}

pub(super) fn output(context: &egui::Context, chat: Uuid) -> Option<String> {
    context.data(|data| data.get_temp(output_id(chat)))
}

pub(super) fn clear_output(context: &egui::Context, chat: Uuid) {
    context.data_mut(|data| data.remove::<String>(output_id(chat)));
}

fn output_id(chat: Uuid) -> egui::Id {
    egui::Id::new(("win98_terminal_command_output", chat))
}

pub(super) fn run(app: &mut ReDox, context: &egui::Context, input: &str) {
    let input = input.trim();
    let (name, args) = command_parts(input);
    let result = execute(app, context, &name.to_ascii_lowercase(), args);
    if let Some(chat) = app.session.active() {
        let output = if result.is_empty() {
            format!("> {input}")
        } else {
            format!("> {input}\n{result}")
        };
        context.data_mut(|data| data.insert_temp(output_id(chat.id), output));
    }
}

fn execute(app: &mut ReDox, context: &egui::Context, name: &str, args: &str) -> String {
    let action = match name {
        "" | "help" => return help(args),
        "saffyos" | "saffos" if SaffyosCommand::parse(args) == Some(SaffyosCommand::Project) => {
            UiAction::OpenProject
        }
        "saffyos" | "saffos" => {
            let Some(command) = SaffyosCommand::parse(args) else {
                return "Usage: /saffyOS [project | locked | loop]".into();
            };
            let locked = matches!(command, SaffyosCommand::Locked | SaffyosCommand::Loop);
            if app.running.is_some() || app.history_reading.is_some() {
                return "Wait for the current request or chat load to finish.".into();
            }
            let folder = std::path::Path::new(app.ui_preferences.saffyos_folder.trim());
            if !folder.is_absolute() || !folder.is_dir() {
                return "Set an existing default folder in /settings saffyos.".into();
            }
            let Ok(folder) = folder.canonicalize() else {
                return "Set an existing default folder in /settings saffyos.".into();
            };
            if locked {
                app.session.open_saffyos(folder.clone());
                app.show_archived = false;
                app.focus_prompt = true;
            } else if app.session.active().is_none_or(|chat| {
                chat.archived
                    || chat.is_saffyos_locked()
                    || chat.workspace.as_ref() != Some(&folder)
            }) {
                app.new_project_chat(folder.clone());
            }
            app.session.workspace = Some(folder);
            if app.runtime.is_none() {
                return "Opened SaffyOS. Agent runtime is unavailable.".into();
            }
            if command == SaffyosCommand::Loop {
                app.saffyos_loop = app.session.active_chat.map(SaffyosLoop::new);
                if let Some(state) = &mut app.saffyos_loop {
                    state.due = Some(std::time::Instant::now());
                }
            }
            app.request_ollama_models(context, app.session.active_chat);
            return if command == SaffyosCommand::Loop {
                "Looking up the SaffyOS model. Auto-loop will keep trying if Ollama is unavailable. Use /stop to cancel.".into()
            } else {
                "Looking up the SaffyOS model in Ollama...".into()
            };
        }
        "ollama" => {
            match args.to_ascii_lowercase().as_str() {
                "" => {
                    super::set_sidebars_visible(context, true);
                    app.ui_preferences.agents_open = true;
                }
                "use" => {
                    if app.running.is_some() || app.history_reading.is_some() {
                        return "Wait for the current request or chat load to finish.".into();
                    }
                    app.ollama_error = None;
                    app.use_ollama(false);
                    return app
                        .ollama_error
                        .clone()
                        .unwrap_or_else(|| app.status.clone());
                }
                _ => return "Usage: /ollama [use]".into(),
            }
            return String::new();
        }
        "codex" => {
            if !args.is_empty() {
                return "Usage: /codex".into();
            }
            if app.running.is_some() {
                return "Stop the response before switching providers.".into();
            }
            app.use_codex();
            return app.status.clone();
        }
        "settings" | "setting" if args.is_empty() || args.eq_ignore_ascii_case("terminal") => {
            super::settings::open(context, app.ui_preferences.win98_terminal_appearance);
            return String::new();
        }
        "settings" | "setting" => return settings(app, args),
        "usage" if args.is_empty() || args.eq_ignore_ascii_case("check") => {
            app.open_usage(context);
            return String::new();
        }
        "usage" => return "Usage: /usage check".into(),
        "model" => return model(app, args),
        "effort" => return effort(app, args),
        "access" => return access(app, args),
        "dark" | "light" if args.is_empty() || args.eq_ignore_ascii_case("mode") => {
            return app.set_win98_theme(context, name == "dark");
        }
        "dark" | "light" => return format!("Usage: /{name}"),
        "theme" => {
            return match args.to_ascii_lowercase().as_str() {
                "" => format!("Current theme: {}", app.active_ui.config.name),
                "dark" => app.set_win98_theme(context, true),
                "light" => app.set_win98_theme(context, false),
                _ => "Usage: /theme [dark|light]".into(),
            };
        }
        "fullscreen" => {
            if !args.is_empty() {
                return "Usage: /fullscreen".into();
            }
            super::set_fullscreen(context, !super::is_fullscreen(context));
            return String::new();
        }
        "sidebar" => {
            if !args.is_empty() {
                return "Usage: /sidebar".into();
            }
            let visible = super::is_fullscreen(context) || !super::sidebars_visible(app, context);
            super::set_sidebars_visible(context, visible);
            return String::new();
        }
        "dock" => {
            if !args.is_empty() {
                return "Usage: /dock".into();
            }
            super::toggle_docked(app, context);
            return String::new();
        }
        "chat" => {
            if !args.is_empty() {
                return "Usage: /chat".into();
            }
            super::set_open(app, context, false);
            return String::new();
        }
        "stop" => {
            if !args.is_empty() {
                return "Usage: /stop".into();
            }
            let armed = app
                .saffyos_loop
                .as_ref()
                .is_some_and(|state| Some(state.chat) == app.session.active_chat);
            if armed && app.running.is_some() && !app.active_chat_running() {
                app.saffyos_loop = None;
                return "SaffyOS auto-loop stopped.".into();
            }
            if !armed && !app.active_chat_running() {
                return "No response is running in this chat.".into();
            }
            app.dispatch_ui_action(UiAction::Stop, context);
            return app.status.clone();
        }
        _ => {
            let action = match name {
                "newchat" => UiAction::NewChat,
                "chats" => UiAction::Chats,
                "history" => UiAction::History,
                "previous" => UiAction::Previous,
                "next" => UiAction::Next,
                "rename" => UiAction::Rename,
                "project" => UiAction::OpenProject,
                "attach" => UiAction::AddFiles,
                _ => return format!("Unknown command /{name}. Type /help for the list."),
            };
            if !args.is_empty() {
                return format!("Usage: /{name}");
            }
            action
        }
    };
    if matches!(action, UiAction::Chats | UiAction::History) {
        super::set_sidebars_visible(context, true);
    }
    app.dispatch_ui_action(action, context);
    String::new()
}

impl ReDox {
    pub(in crate::app) fn finish_terminal_redox(
        &mut self,
        context: &egui::Context,
        chat_id: Uuid,
        endpoint_matches: bool,
    ) {
        let configured_model = self.ui_preferences.saffyos_model.trim();
        let mut models = self.ollama_models.iter().map(|model| model.name.as_str());
        let selected_model = if configured_model.is_empty() {
            saffyos_model(models)
        } else {
            models.find(|model| model.eq_ignore_ascii_case(configured_model))
        };
        let mut ready = false;
        let looping = self
            .saffyos_loop
            .as_ref()
            .is_some_and(|state| state.chat == chat_id);
        let mut chat = self
            .session
            .chats
            .iter_mut()
            .find(|chat| chat.id == chat_id && !chat.archived)
            .filter(|_| looping || self.session.active_chat == Some(chat_id))
            .filter(|_| self.running.is_none() && self.history_reading.is_none());
        let mut result = if !endpoint_matches {
            "The Ollama endpoint changed. Run /saffyOS again.".into()
        } else if let Some(error) = &self.ollama_error {
            format!("Could not list Ollama models: {error}")
        } else if let Some(chat) = chat.as_deref_mut()
            && let Some(model) = selected_model
        {
            let label = if chat.is_saffyos_locked() {
                "Locked SaffyOS"
            } else {
                "SaffyOS"
            };
            chat.set_provider(Provider::Ollama, model.into());
            ready = true;
            let continuation = if looping {
                " Auto-loop armed. Use /stop to end it."
            } else {
                ""
            };
            self.status = format!("{label} · {model}. Ready for your request.{continuation}");
            self.status.clone()
        } else if chat.is_none() {
            "The chat changed or became busy. Run /saffyOS again.".into()
        } else if !configured_model.is_empty() {
            format!("Ollama model {configured_model} was not found. Check /settings saffyos.")
        } else {
            "Ainsley Ox BBB 1.6 or 1.5 was not found in Ollama. Install Aisnley-Ox-1.6-bbb:35b or a supported 1.5 candidate.".into()
        };
        if let Some(state) = self
            .saffyos_loop
            .as_mut()
            .filter(|state| state.chat == chat_id)
        {
            if ready {
                state.step = SaffyosStep::Continue;
                state.due = None;
                state.failures = 0;
            } else {
                state.step = SaffyosStep::Discover;
                let delay = state.wait_after_failure();
                result = format!(
                    "SaffyOS auto-loop will retry in {}s. {result}",
                    delay.as_secs()
                );
                self.status = result.clone();
                context.request_repaint_after(delay);
            }
        }
        context.data_mut(|data| {
            let id = output_id(chat_id);
            if let Some(previous) = data.get_temp::<String>(id)
                && let Some(command) = previous.lines().next()
                && let Some(input) = command.strip_prefix("> ")
                && let (name, args) = command_parts(input)
                && is_saffyos(name)
                && matches!(
                    SaffyosCommand::parse(args),
                    Some(SaffyosCommand::Workspace | SaffyosCommand::Locked | SaffyosCommand::Loop)
                )
            {
                data.insert_temp(id, format!("{command}\n{result}"));
            }
        });
    }
}

fn saffyos_model<'a>(models: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    models
        .filter_map(|model| {
            let (version, tag) = redox::ollama::versioned_ainsley(model)?;
            let tag_priority = match tag {
                "35b" => 5,
                "" | "latest" => 4,
                "exp-invariants" => 3,
                "exp-loop-check" => 2,
                "exp-coding-sampling" => 1,
                _ => return None,
            };
            matches!(version, 5 | 6).then_some((version, tag_priority, model))
        })
        .max()
        .map(|(_, _, model)| model)
}

fn settings(app: &mut ReDox, page: &str) -> String {
    let Some(&(_, selected)) = SETTINGS_PAGES
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(page))
    else {
        let pages = std::iter::once("terminal")
            .chain(SETTINGS_PAGES.iter().map(|&(name, _)| name))
            .collect::<Vec<_>>()
            .join(", ");
        return format!("Settings pages: {pages}");
    };
    app.settings_page = selected;
    app.settings_open = true;
    String::new()
}

fn access(app: &mut ReDox, args: &str) -> String {
    let Some(chat) = app.session.active_mut() else {
        return "Open a chat to manage access.".into();
    };
    if chat.is_saffyos_locked() {
        return "SaffyOS can work with files in its locked folder and test Python scripts offline. Shell and desktop access are disabled.".into();
    }
    if args.is_empty() {
        return format!("Current access: {}", chat.mode.label());
    }
    let Some(&(_, mode)) = ACCESS_MODES
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(args))
    else {
        return help("access");
    };
    if app.running.is_some() {
        return "Stop the response before changing access.".into();
    }
    if chat.workspace.is_none() && mode != Mode::Chat {
        return "Choose a project with /project before increasing access.".into();
    }
    chat.set_mode(mode);
    format!("Access: {}", chat.mode.label())
}

fn model(app: &mut ReDox, args: &str) -> String {
    let (name, rest) = args.split_once(char::is_whitespace).unwrap_or((args, ""));
    if name.eq_ignore_ascii_case("effort") {
        return effort(app, rest.trim());
    }
    let local = app.is_ollama();
    if args.is_empty() {
        let current = if local {
            app.session
                .active()
                .map_or("", |chat| chat.ollama_model.as_str())
        } else {
            app.effective_model()
        };
        let choices = available_models(app);
        if choices.is_empty() {
            return "No Ollama models are loaded. Open /ollama to connect.".into();
        }
        return format!(
            "Current model: {current}\nAvailable: {}",
            choices.join(", ")
        );
    }
    if app.running.is_some() {
        return "Stop the response before changing models.".into();
    }
    if local {
        if !app.ollama_models.iter().any(|model| model.name == args) {
            return "Unknown Ollama model. Use /model to list installed models.".into();
        }
        if let Some(chat) = app.session.active_mut() {
            chat.ollama_model = args.into();
        }
    } else if args.eq_ignore_ascii_case("default") {
        app.session.model = DEFAULT_MODEL.into();
        return format!("Model: {DEFAULT_MODEL}");
    } else if app.models.iter().any(|model| model == args) {
        app.session.model = args.into();
    } else {
        return "Unknown model. Use /model to list available models.".into();
    }
    format!("Model: {args}")
}

fn effort(app: &mut ReDox, level: &str) -> String {
    if app.is_ollama() {
        return "Reasoning effort is available for Codex models here.".into();
    }
    let Some(model) = app.selected_model_info() else {
        return "No effort options are loaded. Check /settings models.".into();
    };
    let options = &model.supported_reasoning_efforts;
    let levels = available_efforts(model).collect::<Vec<_>>().join(", ");
    if level.is_empty() {
        let current = app.selected_effort().unwrap_or_else(|| "default".into());
        return format!("Current effort: {current}\nAvailable: {levels}");
    }
    if app.running.is_some() {
        return "Stop the response before changing effort.".into();
    }
    let model_id = model.model.clone();
    if level.eq_ignore_ascii_case("default") {
        app.session.model_efforts.remove(&model_id);
        return "Effort: model default".into();
    }
    let Some(option) = options
        .iter()
        .find(|option| option.reasoning_effort.eq_ignore_ascii_case(level))
    else {
        return format!("Unknown effort. Available: {levels}");
    };
    let selected = option.reasoning_effort.clone();
    app.session.model_efforts.insert(model_id, selected.clone());
    format!("Effort: {selected}")
}
