use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::attachments::Attachment;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const DEFAULT_MODEL: &str = "Account default";
pub const MAX_CONTEXT_BYTES: usize = 128 * 1024;

#[derive(Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, Debug)]
pub enum Provider {
    #[default]
    Codex,
    Ollama,
}

#[derive(Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum Mode {
    Chat,
    #[default]
    Code,
    FullAccess,
}

impl Mode {
    pub fn sandbox(self) -> &'static str {
        match self {
            Self::Chat => "read-only",
            Self::Code => "workspace-write",
            Self::FullAccess => "danger-full-access",
        }
    }

    pub fn approval_policy(self) -> &'static str {
        if self == Self::FullAccess {
            "never"
        } else {
            "untrusted"
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Chat => "Read-only workspace",
            Self::Code => "Workspace access",
            Self::FullAccess => "Full access",
        }
    }
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Debug)]
pub enum MessageStatus {
    Queued,
    Streaming,
    Complete,
    Cancelled,
    Failed,
    Interrupted,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Message {
    pub id: Uuid,
    pub role: Role,
    pub content: String,
    pub status: MessageStatus,
    pub error: Option<String>,
    pub model: String,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(
        default,
        skip_serializing_if = "crate::activity::TurnActivity::is_unrecorded"
    )]
    pub activity: crate::activity::TurnActivity,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RunId {
    pub id: Uuid,
    pub chat: Uuid,
    pub message: Uuid,
}

#[derive(Clone, Copy)]
pub struct PromptEdit<'a> {
    pub chat: Uuid,
    pub message: Uuid,
    pub content: &'a str,
    pub removed_attachments: &'a [Uuid],
}

impl Message {
    pub fn is_editable_prompt(&self) -> bool {
        self.role == Role::User && self.status == MessageStatus::Complete
    }

    fn text_bytes(&self) -> usize {
        self.content.len()
            + self
                .attachments
                .iter()
                .map(Attachment::text_bytes)
                .sum::<usize>()
    }
}

#[derive(Deserialize, Serialize)]
pub struct Chat {
    #[serde(default)]
    saffyos_locked: bool,
    #[serde(default)]
    pub imported_history: bool,
    #[serde(skip)]
    pub send_after: BTreeMap<Uuid, Instant>,
    #[serde(default)]
    pub desktop_tools: bool,
    #[serde(default)]
    pub delegation_tools: bool,
    #[serde(default)]
    pub delegation_tool_version: u32,
    #[serde(default)]
    pub delegated_tasks: Vec<crate::delegation::Task>,
    #[serde(default)]
    pub provider: Provider,
    #[serde(default)]
    pub ollama_model: String,
    #[serde(default)]
    pub ollama_usage: crate::ollama::Usage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ollama_checkpoint: Option<Uuid>,
    pub id: Uuid,
    pub title: String,
    pub draft: String,
    pub archived: bool,
    pub workspace: Option<PathBuf>,
    #[serde(default)]
    pub messages: Vec<Message>,
    #[serde(default)]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_start: Option<Uuid>,
    #[serde(default)]
    pub mode: Mode,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(default)]
    pub previous_thread_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    prompt_versions: BTreeMap<Uuid, PromptVersions>,
}

#[derive(Default, Deserialize, Serialize)]
struct PromptVersions {
    // The active tail is in Chat::messages. The end of each stack is the adjacent version.
    older: Vec<VersionTail>,
    newer: Vec<VersionTail>,
}

#[derive(Deserialize, Serialize)]
struct VersionTail {
    messages: Vec<Message>,
    #[serde(skip)]
    send_after: BTreeMap<Uuid, Instant>,
    delegated_tasks: Vec<crate::delegation::Task>,
    thread_id: Option<String>,
    thread_start: Option<Uuid>,
    desktop_tools: bool,
    delegation_tools: bool,
    delegation_tool_version: u32,
    imported_history: bool,
    ollama_checkpoint: Option<Uuid>,
}

impl Chat {
    pub fn is_saffyos_locked(&self) -> bool {
        self.saffyos_locked
    }

    pub fn is_blank(&self) -> bool {
        !self.archived
            && self.title == "New chat"
            && self.messages.is_empty()
            && self.draft.trim().is_empty()
            && self.attachments.is_empty()
            && self.thread_id.is_none()
            && self.previous_thread_ids.is_empty()
            && self.delegated_tasks.is_empty()
    }

    pub fn new(workspace: Option<PathBuf>) -> Self {
        let mode = if workspace.is_some() {
            Mode::Code
        } else {
            Mode::Chat
        };
        Self {
            saffyos_locked: false,
            imported_history: false,
            send_after: BTreeMap::new(),
            desktop_tools: false,
            delegation_tools: false,
            delegation_tool_version: 0,
            delegated_tasks: Vec::new(),
            provider: Provider::Codex,
            ollama_model: String::new(),
            ollama_usage: crate::ollama::Usage::default(),
            ollama_checkpoint: None,
            id: Uuid::new_v4(),
            title: "New chat".into(),
            draft: String::new(),
            archived: false,
            workspace,
            messages: Vec::new(),
            thread_id: None,
            thread_start: None,
            mode,
            attachments: Vec::new(),
            previous_thread_ids: Vec::new(),
            prompt_versions: BTreeMap::new(),
        }
    }

    pub fn has_prompt_versions(&self) -> bool {
        !self.prompt_versions.is_empty()
    }

    pub fn prompt_version(&self, message: Uuid) -> Option<(usize, usize)> {
        let versions = self.prompt_versions.get(&message)?;
        Some((
            versions.older.len() + 1,
            versions.older.len() + 1 + versions.newer.len(),
        ))
    }

    pub fn inactive_threads(&self) -> impl Iterator<Item = &str> {
        self.prompt_versions.values().flat_map(|versions| {
            versions
                .older
                .iter()
                .chain(&versions.newer)
                .filter_map(|tail| tail.thread_id.as_deref())
        })
    }

    fn take_tail(&mut self, index: usize) -> VersionTail {
        let messages = self.messages.split_off(index);
        let send_after = messages
            .iter()
            .filter_map(|message| {
                self.send_after
                    .remove(&message.id)
                    .map(|deadline| (message.id, deadline))
            })
            .collect();
        let delegated_tasks = self
            .delegated_tasks
            .extract_if(.., |task| {
                task.parent_message
                    .is_none_or(|id| messages.iter().any(|m| m.id == id))
            })
            .collect();
        VersionTail {
            messages,
            send_after,
            delegated_tasks,
            thread_id: self.thread_id.take(),
            thread_start: self.thread_start.take(),
            desktop_tools: std::mem::take(&mut self.desktop_tools),
            delegation_tools: std::mem::take(&mut self.delegation_tools),
            delegation_tool_version: std::mem::take(&mut self.delegation_tool_version),
            imported_history: std::mem::take(&mut self.imported_history),
            ollama_checkpoint: self.ollama_checkpoint.take(),
        }
    }

    fn restore_tail(&mut self, tail: VersionTail) {
        self.messages.extend(tail.messages);
        self.send_after.extend(tail.send_after);
        self.delegated_tasks.extend(tail.delegated_tasks);
        self.thread_id = tail.thread_id;
        self.thread_start = tail.thread_start;
        self.desktop_tools = tail.desktop_tools;
        self.delegation_tools = tail.delegation_tools;
        self.delegation_tool_version = tail.delegation_tool_version;
        self.imported_history = tail.imported_history;
        self.ollama_checkpoint = tail.ollama_checkpoint;
    }

    fn has_compacted_history(&self) -> bool {
        self.provider == Provider::Ollama
            && (self.saffyos_locked || crate::ollama::is_ainsley_interactive(&self.ollama_model))
    }

    pub fn ollama_context(&self) -> Option<crate::ollama::ContextUsage> {
        let (id, usage) = self.ollama_usage.context?;
        let reply = self
            .messages
            .iter()
            .rfind(|message| message.role == Role::Assistant)?;
        (reply.id == id
            && reply.model.strip_prefix("Ollama · ") == Some(self.ollama_model.as_str()))
        .then_some(usage)
    }

    fn reset_variant_contexts(&mut self) {
        for versions in self.prompt_versions.values_mut() {
            for tail in versions.older.iter_mut().chain(&mut versions.newer) {
                self.previous_thread_ids.extend(tail.thread_id.take());
                tail.thread_start = None;
                tail.ollama_checkpoint = Some(Uuid::new_v4());
            }
        }
    }

    pub fn ready_queued(&self, now: Instant) -> Option<Uuid> {
        self.messages
            .iter()
            .find(|message| {
                message.status == MessageStatus::Queued
                    && self
                        .send_after
                        .get(&message.id)
                        .is_none_or(|due| *due <= now)
            })
            .map(|message| message.id)
    }

    pub fn cancel_queued(&mut self, id: Uuid, reason: String) {
        if let Some(message) = self
            .messages
            .iter_mut()
            .find(|message| message.id == id && message.status == MessageStatus::Queued)
        {
            self.send_after.remove(&id);
            message.status = MessageStatus::Interrupted;
            message.error = Some(reason);
        }
    }

    pub(crate) fn interrupt_queue(&mut self, reason: &str) {
        self.send_after.clear();
        interrupt_queued_messages(&mut self.messages, reason);
    }

    fn interrupt_saved_queues(&mut self, reason: &str) {
        for versions in self.prompt_versions.values_mut() {
            for tail in versions.older.iter_mut().chain(&mut versions.newer) {
                tail.send_after.clear();
                interrupt_queued_messages(&mut tail.messages, reason);
            }
        }
    }

    pub fn set_mode(&mut self, mode: Mode) {
        if self.saffyos_locked {
            return;
        }
        let mode = if self.workspace.is_some() {
            mode
        } else {
            Mode::Chat
        };
        if self.mode != mode {
            self.reset_server_thread();
            self.reset_variant_contexts();
            self.mode = mode;
        }
    }

    pub fn set_workspace(&mut self, workspace: Option<PathBuf>) {
        if self.saffyos_locked {
            return;
        }
        if self.workspace != workspace {
            if workspace.is_none() {
                self.mode = Mode::Chat;
            } else if self.workspace.is_none() {
                self.mode = Mode::FullAccess;
            }
            self.reset_server_thread();
            self.reset_variant_contexts();
            self.workspace = workspace;
        }
    }

    pub fn set_provider(&mut self, provider: Provider, model: String) {
        if self.saffyos_locked && provider != Provider::Ollama {
            return;
        }
        if self.provider != provider {
            self.reset_server_thread();
            self.reset_variant_contexts();
        }
        self.provider = provider;
        if provider == Provider::Ollama {
            self.ollama_model = model;
        }
    }

    fn reset_server_thread(&mut self) {
        self.thread_start = None;
        self.desktop_tools = false;
        self.delegation_tools = false;
        self.delegation_tool_version = 0;
        if let Some(thread) = self.thread_id.take() {
            self.previous_thread_ids.push(thread);
        }
    }
}

fn interrupt_queued_messages(messages: &mut [Message], reason: &str) {
    for message in messages
        .iter_mut()
        .filter(|message| message.status == MessageStatus::Queued)
    {
        message.status = MessageStatus::Interrupted;
        message.error = Some(reason.into());
    }
}

fn check_prompt_budget(history: usize, prompt: &str, attachments: usize) -> Result<(), String> {
    if history
        .saturating_add(prompt.len())
        .saturating_add(attachments)
        > MAX_CONTEXT_BYTES
    {
        return Err(
            "This chat exceeds the 128 KiB text budget. Start a new chat; your draft is kept."
                .into(),
        );
    }
    Ok(())
}

fn append_reply(chat: &mut Chat, model: String) -> RunId {
    let message = Uuid::new_v4();
    chat.messages.push(Message {
        id: message,
        role: Role::Assistant,
        content: String::new(),
        status: MessageStatus::Streaming,
        error: None,
        model,
        attachments: Vec::new(),
        activity: Default::default(),
    });
    RunId {
        id: Uuid::new_v4(),
        chat: chat.id,
        message,
    }
}

#[derive(Deserialize, Serialize)]
#[serde(default)]
pub struct Session {
    pub resources: crate::resources::Settings,
    pub desktop: crate::desktop::Settings,
    pub archived_projects: Vec<PathBuf>,
    pub local_archive_overrides: BTreeMap<String, bool>,
    pub ollama: crate::ollama::Settings,
    pub chats: Vec<Chat>,
    pub active_chat: Option<Uuid>,
    pub workspace: Option<PathBuf>,
    pub codex_path: String,
    pub model: String,
    #[serde(default)]
    pub tool_access_version: u32,
    #[serde(default)]
    pub model_efforts: BTreeMap<String, String>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            resources: crate::resources::Settings::default(),
            desktop: crate::desktop::Settings::default(),
            archived_projects: Vec::new(),
            local_archive_overrides: BTreeMap::new(),
            ollama: crate::ollama::Settings::default(),
            chats: Vec::new(),
            active_chat: None,
            workspace: None,
            codex_path: crate::codex::find_executable(),
            model: DEFAULT_MODEL.into(),
            tool_access_version: 3,
            model_efforts: BTreeMap::new(),
        }
    }
}

impl Session {
    pub fn switch_prompt_version(&mut self, message: Uuid, older: bool) -> Result<(), String> {
        self.check_idle()?;
        let chat = self
            .active_mut()
            .ok_or("Open the chat to switch versions.")?;
        let index = chat
            .messages
            .iter()
            .position(|m| m.id == message)
            .ok_or("That message is no longer available.")?;
        let versions = chat
            .prompt_versions
            .get_mut(&message)
            .ok_or("That version is unavailable.")?;
        let target = if older {
            versions.older.pop()
        } else {
            versions.newer.pop()
        }
        .ok_or("That version is unavailable.")?;
        let current = chat.take_tail(index);
        chat.restore_tail(target);
        let versions = chat.prompt_versions.get_mut(&message).unwrap();
        if older {
            versions.newer.push(current);
        } else {
            versions.older.push(current);
        }
        Ok(())
    }

    pub fn branch_project_chat(&mut self, workspace: &Path) -> Uuid {
        let belongs = |chat: &&Chat| {
            !chat.saffyos_locked && !chat.archived && chat.workspace.as_deref() == Some(workspace)
        };
        let source = self
            .active()
            .filter(belongs)
            .or_else(|| self.chats.iter().rev().find(belongs));
        let mut branch = Chat::new(Some(workspace.to_path_buf()));
        if let Some(source) = source {
            branch.provider = source.provider;
            branch.ollama_model = source.ollama_model.clone();
            branch.mode = source.mode;
            branch.messages = source
                .messages
                .iter()
                .filter(|m| m.status == MessageStatus::Complete)
                .cloned()
                .map(|mut message| {
                    message.id = Uuid::new_v4();
                    message
                })
                .collect();
            if !branch.messages.is_empty() {
                branch.title = format!("Branch · {}", source.title);
            }
        }
        let id = branch.id;
        self.chats.push(branch);
        id
    }

    pub fn queue_prompt(&mut self) -> Result<Uuid, String> {
        let chat = self.active_mut().ok_or("Create a chat first.")?;
        let content = chat.draft.trim().to_owned();
        Self::enqueue(chat, content, true)
    }

    pub fn queue_saffyos_continuation(&mut self, content: &str) -> Result<Uuid, String> {
        let chat = self
            .active_mut()
            .filter(|chat| chat.saffyos_locked)
            .ok_or("Open a locked SaffyOS chat first.")?;
        Self::enqueue(chat, content.into(), false)
    }

    fn enqueue(chat: &mut Chat, content: String, use_draft: bool) -> Result<Uuid, String> {
        let attachments = if use_draft {
            chat.attachments.as_slice()
        } else {
            &[]
        };
        if chat.archived || (content.is_empty() && attachments.is_empty()) {
            return Err("Enter a prompt in an unarchived chat.".into());
        }
        let bytes = content.len()
            + attachments
                .iter()
                .map(Attachment::text_bytes)
                .sum::<usize>()
            + chat
                .messages
                .iter()
                .filter(|m| m.status == MessageStatus::Queued)
                .map(|m| {
                    m.content.len()
                        + m.attachments
                            .iter()
                            .map(Attachment::text_bytes)
                            .sum::<usize>()
                })
                .sum::<usize>();
        if bytes > MAX_CONTEXT_BYTES {
            return Err(
                "Queued messages exceed the 128 KiB text budget. Your draft is kept.".into(),
            );
        }
        let id = Uuid::new_v4();
        let attachments = if use_draft {
            chat.draft.clear();
            std::mem::take(&mut chat.attachments)
        } else {
            Vec::new()
        };
        chat.messages.push(Message {
            id,
            role: Role::User,
            content,
            status: MessageStatus::Queued,
            error: None,
            model: String::new(),
            attachments,
            activity: Default::default(),
        });
        Ok(id)
    }

    pub fn queue_prompt_after(&mut self, delay: Duration) -> Result<Uuid, String> {
        let deadline = Instant::now()
            .checked_add(delay)
            .ok_or("Delay is too long.")?;
        let id = self.queue_prompt()?;
        if !delay.is_zero()
            && let Some(chat) = self.active_mut()
        {
            chat.send_after.insert(id, deadline);
        }
        Ok(id)
    }

    pub fn stop_queued(&mut self, chat_id: Uuid) {
        if let Some(chat) = self.chats.iter_mut().find(|chat| chat.id == chat_id) {
            chat.interrupt_queue("Queued message was not sent. Copy it to send again.");
        }
    }

    pub fn recover(&mut self) {
        if self.model.contains("qwen")
            || self.model.contains("huihui")
            || self.model.contains(":latest")
        {
            self.model = DEFAULT_MODEL.into();
        }

        for chat in &mut self.chats {
            if let Some(thread) = &chat.thread_id {
                if chat.archived {
                    self.local_archive_overrides
                        .entry(thread.clone())
                        .or_insert(true);
                }
                if let Some(archived) = self.local_archive_overrides.get(thread) {
                    chat.archived = *archived;
                }
            }
            for task in &mut chat.delegated_tasks {
                if task.status == crate::delegation::Status::Running {
                    task.status = crate::delegation::Status::Interrupted;
                    task.progress.clear();
                }
            }
            if self.tool_access_version < 2 {
                chat.mode = Mode::Code;
                chat.reset_server_thread();
            } else if self.tool_access_version < 3 && chat.desktop_tools {
                chat.reset_server_thread();
            }
            if chat.workspace.is_none() {
                chat.set_mode(Mode::Chat);
            }
            for message in &mut chat.messages {
                if message.status == MessageStatus::Streaming {
                    message.status = MessageStatus::Interrupted;
                }
            }
            chat.interrupt_queue(
                "ReDox closed before this message was sent. Copy it to send again.",
            );
            chat.interrupt_saved_queues(
                "ReDox closed before this message was sent. Copy it to send again.",
            );
        }
        self.tool_access_version = 3;

        if self.active().is_none() {
            self.active_chat = self
                .chats
                .iter()
                .find(|chat| !chat.archived)
                .map(|chat| chat.id);
        }
    }

    pub fn prepare_run(&mut self) -> Result<RunId, String> {
        let queued = self
            .active()
            .and_then(|chat| chat.ready_queued(Instant::now()));
        self.prepare_selected_run(queued)
    }

    pub fn prepare_selected_run(&mut self, queued: Option<Uuid>) -> Result<RunId, String> {
        if let Some(chat) = self.active_mut()
            && chat.imported_history
            && chat.has_prompt_versions()
        {
            chat.imported_history = false;
            chat.reset_server_thread();
        }
        if self.active().is_some_and(|chat| chat.imported_history) {
            let source = self.active_mut().unwrap();
            let mut branch = Chat::new(source.workspace.clone());
            branch.title = format!("Branch ? {}", source.title);
            branch.messages = source.messages.clone();
            branch.mode = source.mode;
            branch.provider = source.provider;
            branch.draft = std::mem::take(&mut source.draft);
            branch.attachments = std::mem::take(&mut source.attachments);
            branch.send_after = std::mem::take(&mut source.send_after);
            self.active_chat = Some(branch.id);
            self.chats.push(branch);
        }
        if self
            .active()
            .is_some_and(|chat| chat.provider == Provider::Codex && !self.can_resume_codex(chat))
        {
            self.active_mut().unwrap().reset_server_thread();
        }
        self.check_idle()?;
        let model = self.active_run_model()?;
        let chat = self.active_mut().ok_or("Create a chat first.")?;
        let queued = queued
            .map(|id| {
                chat.messages
                    .iter()
                    .position(|m| m.id == id && m.status == MessageStatus::Queued)
                    .filter(|_| {
                        chat.send_after
                            .get(&id)
                            .is_none_or(|due| *due <= Instant::now())
                    })
                    .ok_or("That queued message is no longer ready to send.")
            })
            .transpose()?;
        let prompt = queued.map_or_else(
            || chat.draft.trim().to_owned(),
            |i| chat.messages[i].content.clone(),
        );

        if chat.archived || (queued.is_none() && prompt.is_empty() && chat.attachments.is_empty()) {
            return Err("Enter a prompt in an unarchived chat.".into());
        }

        let history_bytes: usize = if chat.has_compacted_history()
            || (chat.provider == Provider::Codex && chat.thread_id.is_some())
        {
            0
        } else {
            chat.messages
                .iter()
                .filter(|message| message.status == MessageStatus::Complete)
                .map(Message::text_bytes)
                .sum()
        };

        let attachments = queued.map_or(&chat.attachments, |i| &chat.messages[i].attachments);
        let attachment_bytes: usize = attachments.iter().map(Attachment::text_bytes).sum();
        check_prompt_budget(history_bytes, &prompt, attachment_bytes)?;

        if chat.title == "New chat"
            && !chat
                .messages
                .iter()
                .any(|message| message.status == MessageStatus::Complete)
        {
            let title = if prompt.is_empty() {
                attachments[0].name()
            } else {
                prompt.clone()
            };
            chat.title = title
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(48)
                .collect();
        }

        if let Some(index) = queued {
            let mut message = chat.messages.remove(index);
            chat.send_after.remove(&message.id);
            message.status = MessageStatus::Complete;
            message.error = None;
            message.model = model.clone();
            chat.messages.push(message);
        } else {
            chat.messages.push(Message {
                id: Uuid::new_v4(),
                role: Role::User,
                content: prompt,
                status: MessageStatus::Complete,
                error: None,
                model: model.clone(),
                attachments: std::mem::take(&mut chat.attachments),
                activity: Default::default(),
            });
            chat.draft.clear();
        }

        Ok(append_reply(chat, model))
    }

    fn check_idle(&self) -> Result<(), String> {
        if self.chats.iter().any(|chat| {
            chat.messages
                .iter()
                .any(|message| message.status == MessageStatus::Streaming)
        }) {
            return Err("Stop the current response before starting another.".into());
        }
        Ok(())
    }

    fn active_run_model(&self) -> Result<String, String> {
        if let Some(chat) = self
            .active()
            .filter(|chat| chat.provider == Provider::Ollama)
        {
            if chat.ollama_model.trim().is_empty() {
                return Err("Choose an Ollama model in Agents first.".into());
            }
            return Ok(format!("Ollama · {}", chat.ollama_model));
        }
        let model = self.model.trim();
        if model.is_empty() {
            return Err("Choose a model in settings first.".into());
        }
        Ok(model.to_owned())
    }

    fn can_resume_codex(&self, chat: &Chat) -> bool {
        let desktop = self.desktop.enabled && crate::desktop::SUPPORTED;
        let delegate =
            self.ollama.codex_delegation && !self.ollama.preferred_model.trim().is_empty();
        chat.desktop_tools == desktop
            && (!delegate
                || (chat.delegation_tools
                    && chat.delegation_tool_version == crate::delegation::TOOL_VERSION))
    }

    fn check_edit(&self, edit: PromptEdit<'_>) -> Result<(usize, Option<(&str, &str)>), String> {
        self.check_idle()?;
        let chat = self
            .active()
            .filter(|chat| chat.id == edit.chat && !chat.archived)
            .ok_or("Open the original, unarchived chat to send this edit.")?;
        let index = chat
            .messages
            .iter()
            .position(|message| message.id == edit.message && message.is_editable_prompt())
            .ok_or("That sent message is no longer available.")?;
        let message = &chat.messages[index];
        let earlier = &chat.messages[..index];
        let source = chat
            .thread_id
            .as_deref()
            .filter(|_| {
                chat.provider == Provider::Codex
                    && self.can_resume_codex(chat)
                    && !message.activity.turn_id.is_empty()
                    && chat
                        .thread_start
                        .is_none_or(|start| earlier.iter().any(|m| m.id == start))
            })
            .map(|thread| (thread, message.activity.turn_id.as_str()));

        let attachments = || {
            message
                .attachments
                .iter()
                .filter(|file| !edit.removed_attachments.contains(&file.id))
        };
        let prompt = edit.content.trim();
        if prompt.is_empty() && attachments().next().is_none() {
            return Err("Enter a message or keep an attachment.".into());
        }
        if let Some(file) = attachments().find(|file| file.text.is_none() && !file.path.is_file()) {
            return Err(format!(
                "{} is unavailable. Remove it or restore the file.",
                file.name()
            ));
        }
        let history = if source.is_some() || chat.has_compacted_history() {
            0
        } else {
            earlier
                .iter()
                .filter(|m| m.status == MessageStatus::Complete)
                .map(Message::text_bytes)
                .sum()
        };
        check_prompt_budget(
            history,
            prompt,
            attachments().map(Attachment::text_bytes).sum(),
        )?;
        Ok((index, source))
    }

    pub fn edit_fork_source(&self, edit: PromptEdit<'_>) -> Result<Option<(&str, &str)>, String> {
        self.active_run_model()?;
        Ok(self.check_edit(edit)?.1)
    }

    pub fn prepare_edited_run(
        &mut self,
        edit: PromptEdit<'_>,
        fork: Option<crate::history::CutoffFork>,
    ) -> Result<RunId, String> {
        let model = self.active_run_model()?;
        let (index, source) = self.check_edit(edit)?;
        match (source, &fork) {
            (None, None) => {}
            (Some((thread, turn)), Some(fork))
                if fork.source == thread
                    && fork.before_turn == turn
                    && !fork.thread.is_empty()
                    && fork.thread != thread => {}
            _ => {
                return Err(
                    "The chat changed while preparing this edit. Review it and send again.".into(),
                );
            }
        }

        let chat = self.active_mut().unwrap();
        let old = chat.take_tail(index);
        chat.messages.push(Message {
            id: edit.message,
            role: Role::User,
            content: edit.content.trim().to_owned(),
            status: MessageStatus::Complete,
            error: None,
            model: model.clone(),
            attachments: old.messages[0]
                .attachments
                .iter()
                .filter(|file| !edit.removed_attachments.contains(&file.id))
                .cloned()
                .collect(),
            activity: Default::default(),
        });
        chat.ollama_checkpoint = Some(Uuid::new_v4());
        if let Some(fork) = fork {
            chat.thread_id = Some(fork.thread);
            chat.thread_start = old.thread_start;
            chat.desktop_tools = old.desktop_tools;
            chat.delegation_tools = old.delegation_tools;
            chat.delegation_tool_version = old.delegation_tool_version;
        }
        let versions = chat.prompt_versions.entry(edit.message).or_default();
        versions.older.push(old);
        versions.older.extend(versions.newer.drain(..).rev());
        Ok(append_reply(chat, model))
    }

    pub fn retry_saffyos_turn(&mut self, failed: RunId) -> Result<RunId, String> {
        self.check_idle()?;
        let model = self.active_run_model()?;
        let chat = self
            .active_mut()
            .filter(|chat| chat.id == failed.chat && chat.saffyos_locked && !chat.archived)
            .ok_or("Open the original locked SaffyOS chat to retry the turn.")?;
        let reply = chat
            .messages
            .iter_mut()
            .rev()
            .find(|message| {
                matches!(
                    message.status,
                    MessageStatus::Failed | MessageStatus::Complete
                )
            })
            .filter(|message| {
                message.id == failed.message && message.status == MessageStatus::Failed
            })
            .ok_or("The failed SaffyOS turn is no longer the latest response.")?;

        reply.status = MessageStatus::Streaming;
        reply.error = None;
        reply.content.clear();
        reply.model = model;
        reply.activity = Default::default();
        Ok(RunId {
            id: Uuid::new_v4(),
            ..failed
        })
    }

    pub fn run_message_mut(&mut self, run: RunId) -> Option<&mut Message> {
        self.chats
            .iter_mut()
            .rev()
            .find(|chat| chat.id == run.chat)?
            .messages
            .iter_mut()
            .rev()
            .find(|message| message.id == run.message)
            .filter(|message| message.status == MessageStatus::Streaming)
    }

    pub fn finish_run(&mut self, run: RunId, status: MessageStatus, error: Option<String>) {
        if let Some(message) = self.run_message_mut(run) {
            message.status = status;
            message.error = error;
        }
        if status != MessageStatus::Complete
            && let Some(chat) = self.chats.iter_mut().find(|chat| chat.id == run.chat)
        {
            for task in &mut chat.delegated_tasks {
                if task.status == crate::delegation::Status::Running {
                    task.status = if status == MessageStatus::Cancelled {
                        crate::delegation::Status::Cancelled
                    } else {
                        crate::delegation::Status::Interrupted
                    };
                    task.progress.clear();
                }
            }
        }
    }

    pub fn new_chat(&mut self) {
        self.new_chat_in(None);
    }

    pub fn open_saffyos(&mut self, workspace: PathBuf) {
        if self.active().is_some_and(|chat| {
            chat.saffyos_locked && !chat.archived && chat.workspace.as_ref() == Some(&workspace)
        }) {
            return;
        }
        let mut chat = Chat::new(Some(workspace));
        chat.mode = Mode::Chat;
        chat.provider = Provider::Ollama;
        chat.saffyos_locked = true;
        self.active_chat = Some(chat.id);
        self.chats.push(chat);
    }

    pub fn new_project_chat(&mut self, workspace: PathBuf) {
        self.new_chat_in(Some(workspace));
    }

    fn new_chat_in(&mut self, workspace: Option<PathBuf>) {
        let (provider, model) = self
            .active()
            .map_or((Provider::Codex, String::new()), |chat| {
                (chat.provider, chat.ollama_model.clone())
            });
        let matches = |chat: &&Chat| {
            !chat.saffyos_locked
                && chat.is_blank()
                && chat.workspace == workspace
                && chat.provider == provider
                && (provider != Provider::Ollama || chat.ollama_model == model)
        };
        let reusable = self
            .active()
            .filter(matches)
            .or_else(|| self.chats.iter().rev().find(matches))
            .map(|chat| chat.id);
        if let Some(id) = reusable {
            self.active_chat = Some(id);
            return;
        }
        let mut chat = Chat::new(workspace);
        chat.set_mode(Mode::FullAccess);
        if let Some(active) = self.active() {
            chat.provider = active.provider;
            chat.ollama_model = active.ollama_model.clone();
        }
        self.active_chat = Some(chat.id);
        self.chats.push(chat);
    }

    pub fn set_chat_archived(&mut self, id: Uuid, archived: bool) -> bool {
        let Some(target) = self.chats.iter().find(|chat| chat.id == id) else {
            return false;
        };
        let thread = target.thread_id.clone();
        let same = |chat: &Chat| {
            chat.id == id
                || thread
                    .as_ref()
                    .is_some_and(|thread| chat.thread_id.as_ref() == Some(thread))
        };
        if self.chats.iter().filter(|chat| same(chat)).any(|chat| {
            chat.messages
                .iter()
                .any(|message| message.status == MessageStatus::Streaming)
        }) {
            return false;
        }
        for chat in self.chats.iter_mut().filter(|chat| same(chat)) {
            chat.archived = archived;
            if archived {
                chat.interrupt_queue("Chat was archived before this message was sent.");
                chat.interrupt_saved_queues("Chat was archived before this message was sent.");
            }
            for thread in chat.inactive_threads() {
                self.local_archive_overrides
                    .insert(thread.to_owned(), archived);
            }
        }
        if let Some(thread) = thread {
            self.local_archive_overrides.insert(thread, archived);
        }
        true
    }

    pub fn active(&self) -> Option<&Chat> {
        self.chats
            .iter()
            .find(|chat| Some(chat.id) == self.active_chat)
    }

    pub fn active_mut(&mut self) -> Option<&mut Chat> {
        self.chats
            .iter_mut()
            .find(|chat| Some(chat.id) == self.active_chat)
    }
}

#[cfg(test)]
#[path = "../tests/transport/chat_lifecycle.rs"]
mod lifecycle_tests;

#[cfg(test)]
#[path = "../tests/transport/message_edits.rs"]
mod edit_tests;
