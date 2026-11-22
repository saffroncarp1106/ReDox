use eframe::egui;
use redox::codex::ModelInfo;
use redox::domain::{DEFAULT_MODEL, Mode};
use redox_studio::actions::UiAction;
use uuid::Uuid;

use crate::app::ReDox;

use super::super::SettingsPage;

const ACCESS_MODES: [(&str, Mode); 3] = [
    ("full", Mode::FullAccess),
    ("workspace", Mode::Code),
    ("read-only", Mode::Chat),
];

const COMMANDS: &[(&str, &str, &str)] = &[
    ("/help", " [command]", "List commands or show their syntax"),
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
    ("/fullscreen", "", "Toggle Terminal fullscreen"),
    ("/chat", "", "Return to Chat"),
];

const SETTINGS_PAGES: &[(&str, SettingsPage)] = &[
    ("account", SettingsPage::Account),
    ("models", SettingsPage::Models),
    ("workspace", SettingsPage::Workspace),
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
    let Some(command) = draft.trim().strip_prefix('/') else {
        return false;
    };
    !command.contains(['\n', '\r'])
        && command.split_whitespace().next().is_none_or(|name| {
            name.bytes()
                .all(|byte| byte.is_ascii_alphabetic() || byte == b'-')
        })
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
    let prefix = prefix.to_ascii_lowercase();
    COMMANDS
        .iter()
        .filter(|(name, _, _)| name.starts_with(&prefix) && *name != prefix.as_str())
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
    let command = input.strip_prefix('/').unwrap_or_default().trim_start();
    let (name, args) = command
        .split_once(char::is_whitespace)
        .unwrap_or((command, ""));
    let result = execute(app, context, &name.to_ascii_lowercase(), args.trim());
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
        "ollama" => {
            match args.to_ascii_lowercase().as_str() {
                "" => {
                    leave_fullscreen(context);
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
        "fullscreen" => {
            if !args.is_empty() {
                return "Usage: /fullscreen".into();
            }
            super::set_fullscreen(context, !super::is_fullscreen(context));
            return String::new();
        }
        "chat" => {
            if !args.is_empty() {
                return "Usage: /chat".into();
            }
            super::set_open(context, false);
            return String::new();
        }
        "stop" => {
            if !args.is_empty() {
                return "Usage: /stop".into();
            }
            if !app.active_chat_running() {
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
        leave_fullscreen(context);
    }
    app.dispatch_ui_action(action, context);
    String::new()
}

fn leave_fullscreen(context: &egui::Context) {
    if super::is_fullscreen(context) {
        super::set_fullscreen(context, false);
    }
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
