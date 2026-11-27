use eframe::egui;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use crate::codex::{self, Approval, ChatRequest, ProviderEvent};
use crate::domain::{MessageStatus, RunId, Session};

pub enum Update {
    Activity {
        turn_id: String,
        update: crate::activity::ActivityUpdate,
    },
    Feedback(uuid::Uuid, Result<(), String>),
    Delegation(crate::delegation::Event),
    Metrics(crate::ollama::Stats),
    Context(crate::ollama::ContextUsage),
    Qualification(crate::ollama::QualificationTiming),
    OllamaStage(&'static str),
    Content(String),
    ReplaceContent {
        offset: usize,
        text: String,
    },
    Complete,
    Cancelled,
    Failed(String),
    Thread(String),
    Turn(String),
    Status(String),
    Approval(Approval),
}

pub struct RunEvent {
    pub run: RunId,
    pub update: Update,
}

pub struct Running {
    feedback: mpsc::UnboundedSender<codex::Feedback>,
    pub id: RunId,
    task: JoinHandle<()>,
    cancel: watch::Sender<bool>,
    pub desktop_activity: crate::desktop::Activity,
}

pub enum Request {
    Codex {
        request: ChatRequest,
        executable: String,
    },
    Ollama(crate::ollama::Request),
}

impl Request {
    pub fn desktop(&self) -> (&crate::desktop::Settings, &crate::desktop::Activity) {
        match self {
            Self::Codex { request, .. } => (&request.desktop, &request.desktop_activity),
            Self::Ollama(request) => (&request.desktop, &request.desktop_activity),
        }
    }
}

impl Running {
    pub fn start(
        runtime: &tokio::runtime::Runtime,
        id: RunId,
        request: Request,
        sender: mpsc::Sender<RunEvent>,
        context: egui::Context,
    ) -> Self {
        let (cancel, cancellation) = watch::channel(false);
        let (feedback, feedback_receiver) = mpsc::unbounded_channel();
        let desktop_activity = request.desktop().1.clone();
        let poll_escape = request.desktop().0.enabled && crate::desktop::SUPPORTED;
        let activity = desktop_activity.clone();
        let stop = cancel.clone();
        let task = runtime.spawn(async move {
            let (provider_sender, mut provider_receiver) = mpsc::channel(128);
            let producer = tokio::spawn(async move {
                match request {
                    Request::Codex {
                        request,
                        executable,
                    } => {
                        codex::stream_chat_with_feedback(
                            &executable,
                            request,
                            provider_sender,
                            cancellation,
                            feedback_receiver,
                        )
                        .await
                    }
                    Request::Ollama(request) => {
                        crate::ollama::stream_qualified_chat(request, provider_sender, cancellation)
                            .await
                    }
                }
            });

            let mut escape_poll = tokio::time::interval(std::time::Duration::from_millis(8));
            loop {
                let event = tokio::select! {
                    event = provider_receiver.recv() => match event {
                        Some(event) => event,
                        None => break,
                    },
                    _ = escape_poll.tick(), if poll_escape => {
                        if activity.observe_escape(crate::desktop::escape_pressed()) {
                            let already_cancelled = *stop.borrow();
                            if !already_cancelled { let _ = stop.send(true); }
                            context.request_repaint_of(egui::ViewportId::ROOT);
                        }
                        continue;
                    }
                };
                let update = match event {
                    ProviderEvent::Activity { turn_id, update } => {
                        Update::Activity { turn_id, update }
                    }
                    ProviderEvent::Feedback(id, result) => Update::Feedback(id, result),
                    ProviderEvent::Delegation(event) => Update::Delegation(event),
                    ProviderEvent::Metrics(stats) => Update::Metrics(stats),
                    ProviderEvent::Context(usage) => Update::Context(usage),
                    ProviderEvent::Qualification(timing) => Update::Qualification(timing),
                    ProviderEvent::OllamaStage(stage) => Update::OllamaStage(stage),
                    ProviderEvent::Thread(thread) => Update::Thread(thread),
                    ProviderEvent::Turn(turn) => Update::Turn(turn),
                    ProviderEvent::Content(text) => Update::Content(text),
                    ProviderEvent::ReplaceContent { offset, text } => {
                        Update::ReplaceContent { offset, text }
                    }
                    ProviderEvent::Status(text) => Update::Status(text),
                    ProviderEvent::Approval(approval) => Update::Approval(approval),
                };

                if sender.send(RunEvent { run: id, update }).await.is_err() {
                    activity.stop();
                    let _ = stop.send(true);
                    producer.abort();
                    return;
                }

                context.request_repaint_of(egui::ViewportId::ROOT);
            }

            let result = producer
                .await
                .unwrap_or_else(|error| Err(format!("Assistant worker stopped: {error}")));

            let cancelled = activity.snapshot().stopped;
            activity.finish();

            let update = match result {
                _ if cancelled => Update::Cancelled,
                Ok(()) => Update::Complete,
                Err(error) => Update::Failed(error),
            };

            let _ = sender.send(RunEvent { run: id, update }).await;
            context.request_repaint_of(egui::ViewportId::ROOT);
        });

        Self {
            feedback,
            id,
            task,
            cancel,
            desktop_activity,
        }
    }

    pub fn cancel(&self) {
        self.desktop_activity.stop();
        let _ = self.cancel.send(true);
    }

    pub fn feedback(&self, input: codex::Feedback) -> bool {
        self.feedback.send(input).is_ok()
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.desktop_activity.stop();
        let _ = self.cancel.send(true);
        if self.task.is_finished() {
            self.task.abort();
        }
    }
}

pub fn apply_event(session: &mut Session, active: Option<RunId>, event: RunEvent) -> bool {
    if active != Some(event.run) {
        return false;
    }
    let Some(message) = session.run_message_mut(event.run) else {
        return false;
    };

    match event.update {
        Update::Feedback(id, result) => {
            if let Some(message) = session
                .chats
                .iter_mut()
                .find(|c| c.id == event.run.chat)
                .and_then(|chat| chat.messages.iter_mut().find(|m| m.id == id))
            {
                match result {
                    Ok(()) => {
                        message.status = MessageStatus::Complete;
                        message.error = None;
                    }
                    Err(error) => {
                        message.error = Some(format!("Queued for the next response: {error}"));
                    }
                }
            }
            false
        }
        Update::Delegation(mut update) => {
            if let crate::delegation::Event::Started(task) = &mut update {
                task.parent_message = Some(event.run.message);
            }
            if let Some(chat) = session
                .chats
                .iter_mut()
                .find(|chat| chat.id == event.run.chat)
            {
                crate::delegation::apply(&mut chat.delegated_tasks, update);
            }
            false
        }
        Update::Metrics(stats) => {
            if let Some(chat) = session
                .chats
                .iter_mut()
                .find(|chat| chat.id == event.run.chat)
            {
                chat.ollama_usage.record(stats);
            }
            false
        }
        Update::Context(usage) => {
            if let Some(chat) = session
                .chats
                .iter_mut()
                .find(|chat| chat.id == event.run.chat)
            {
                chat.ollama_usage.context = Some((event.run.message, usage));
            }
            false
        }
        Update::Qualification(timing) => {
            if let Some(chat) = session
                .chats
                .iter_mut()
                .find(|chat| chat.id == event.run.chat)
            {
                chat.ollama_usage.qualification = Some((event.run.message, timing));
            }
            false
        }
        Update::Thread(thread) => {
            let desktop = session.desktop.enabled && crate::desktop::SUPPORTED;
            let delegation = session.ollama.codex_delegation
                && !session.ollama.preferred_model.trim().is_empty();
            if let Some(chat) = session
                .chats
                .iter_mut()
                .find(|chat| chat.id == event.run.chat)
            {
                if chat.thread_id.as_deref() != Some(&thread) {
                    chat.thread_start =
                        prompt_for_reply(chat, event.run.message).map(|prompt| prompt.id);
                    chat.thread_id = Some(thread);
                }
                chat.desktop_tools = desktop;
                chat.delegation_tools = delegation;
                chat.delegation_tool_version = if delegation {
                    crate::delegation::TOOL_VERSION
                } else {
                    0
                };
            }
            false
        }
        Update::Turn(turn) => {
            if let Some(chat) = session
                .chats
                .iter_mut()
                .rev()
                .find(|chat| chat.id == event.run.chat)
                && let Some(prompt) = prompt_for_reply(chat, event.run.message)
            {
                prompt.activity.turn_id = turn;
            }
            false
        }
        Update::Status(_) | Update::OllamaStage(_) | Update::Approval(_) => false,
        Update::Activity { turn_id, update } => {
            message.activity.turn_id = turn_id;
            match update {
                crate::activity::ActivityUpdate::Command(mut command) => {
                    command.offset = message.content.len();
                    message.activity.record_command(command);
                }
                crate::activity::ActivityUpdate::Diff(diff) => message.activity.replace_diff(&diff),
            }
            false
        }
        Update::Content(text) => {
            message.content.push_str(&text);
            false
        }
        Update::ReplaceContent { offset, text } => {
            if message.content.is_char_boundary(offset) {
                message.content.truncate(offset);
                message.content.push_str(&text);
            }
            false
        }
        Update::Complete => {
            session.finish_run(event.run, MessageStatus::Complete, None);
            true
        }
        Update::Cancelled => {
            session.finish_run(
                event.run,
                MessageStatus::Cancelled,
                Some("Cancelled (Stop or Escape). Partial response kept.".into()),
            );
            true
        }
        Update::Failed(error) => {
            session.finish_run(event.run, MessageStatus::Failed, Some(error));
            true
        }
    }
}

fn prompt_for_reply(
    chat: &mut crate::domain::Chat,
    reply: uuid::Uuid,
) -> Option<&mut crate::domain::Message> {
    let previous = chat
        .messages
        .iter()
        .rposition(|message| message.id == reply)?
        .checked_sub(1)?;
    chat.messages
        .get_mut(previous)
        .filter(|message| message.is_editable_prompt())
}
