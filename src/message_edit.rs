use eframe::egui;
use redox::domain::PromptEdit;
use redox::history::{self, CutoffFork};
use uuid::Uuid;

use super::{Event, ReDox, codex};

pub(super) struct MessageEdit {
    pub chat: Uuid,
    pub message: Uuid,
    pub text: String,
    pub removed_attachments: Vec<Uuid>,
    pub focus: bool,
    pub input_id: Option<egui::Id>,
    pub error: Option<String>,
    pending: Option<(Uuid, tokio::task::JoinHandle<()>)>,
}

#[cfg(all(test, feature = "design-export"))]
#[path = "../tests/ui/message_edit.rs"]
mod tests;

impl MessageEdit {
    pub fn waiting(&self) -> bool {
        self.pending.is_some()
    }

    fn prompt(&self) -> PromptEdit<'_> {
        PromptEdit {
            chat: self.chat,
            message: self.message,
            content: &self.text,
            removed_attachments: &self.removed_attachments,
        }
    }
}

impl Drop for MessageEdit {
    fn drop(&mut self) {
        if let Some((_, task)) = &self.pending {
            task.abort();
        }
    }
}

impl ReDox {
    pub(super) fn is_editing_message(&self) -> bool {
        self.message_edit
            .as_ref()
            .is_some_and(|edit| Some(edit.chat) == self.session.active_chat)
    }

    pub(super) fn preparing_message_edit(&self) -> bool {
        self.message_edit.as_ref().is_some_and(MessageEdit::waiting)
    }

    pub(super) fn close_message_edit(&mut self, ctx: &egui::Context) {
        if let Some(edit) = self.message_edit.take()
            && let Some(id) = edit.input_id
        {
            ctx.memory_mut(|memory| memory.surrender_focus(id));
            ctx.data_mut(|data| data.remove::<egui::text_edit::TextEditState>(id));
        }
    }

    pub(super) fn can_edit_messages(&self) -> bool {
        !self.startup.is_active()
            && self.running.is_none()
            && !self.logging_out
            && !self.picking_files
            && self.history_reading.is_none()
    }

    pub(super) fn begin_message_edit(&mut self, id: Uuid) {
        if !self.can_edit_messages() || self.is_editing_message() {
            return;
        }
        let Some(chat) = self.session.active().filter(|chat| !chat.archived) else {
            return;
        };
        let Some(message) = chat
            .messages
            .iter()
            .find(|message| message.id == id && message.is_editable_prompt())
        else {
            return;
        };
        self.focus_prompt = false;
        self.message_edit = Some(MessageEdit {
            chat: chat.id,
            message: id,
            text: message.content.clone(),
            removed_attachments: Vec::new(),
            focus: true,
            input_id: None,
            error: None,
            pending: None,
        });
    }

    pub(super) fn submit_message_edit(&mut self, ctx: &egui::Context) {
        if !self.can_start_prompt() || !self.is_editing_message() || self.preparing_message_edit() {
            return;
        }
        let edit = self.message_edit.as_mut().unwrap();
        edit.error = None;
        let source = codex::validate_workspace(
            self.session
                .active()
                .and_then(|chat| chat.workspace.as_deref()),
        )
        .and_then(|()| self.session.edit_fork_source(edit.prompt()));
        match source {
            Ok(Some((thread, turn))) => {
                let (thread, turn) = (thread.to_owned(), turn.to_owned());
                let executable = self.session.codex_path.clone();
                let sender = self.sender.clone();
                let context = ctx.clone();
                let id = Uuid::new_v4();
                let task = self.runtime.as_ref().unwrap().spawn(async move {
                    let result = history::fork_before(&executable, thread, turn).await;
                    let _ = sender.send(Event::MessageEditFork(id, result));
                    context.request_repaint_of(egui::ViewportId::ROOT);
                });
                edit.pending = Some((id, task));
            }
            Ok(None) => self.commit_message_edit(ctx, None),
            Err(error) => edit.error = Some(error),
        }
    }

    pub(super) fn finish_message_edit_fork(
        &mut self,
        ctx: &egui::Context,
        id: Uuid,
        result: Result<CutoffFork, String>,
    ) {
        let can_commit = self.can_start_prompt();
        let Some(edit) = self.message_edit.as_mut().filter(|edit| {
            Some(edit.chat) == self.session.active_chat
                && edit
                    .pending
                    .as_ref()
                    .is_some_and(|(pending, _)| *pending == id)
        }) else {
            return;
        };
        edit.pending = None;
        match result {
            Ok(fork) if can_commit => self.commit_message_edit(ctx, Some(fork)),
            Ok(_) => {
                self.message_edit.as_mut().unwrap().error =
                    Some("The chat is busy. Try sending the edit again.".into())
            }
            Err(error) => edit.error = Some(error),
        }
    }

    fn commit_message_edit(&mut self, ctx: &egui::Context, fork: Option<CutoffFork>) {
        let edit = self.message_edit.as_mut().unwrap();
        let result = codex::validate_workspace(
            self.session
                .active()
                .and_then(|chat| chat.workspace.as_deref()),
        )
        .and_then(|()| self.session.prepare_edited_run(edit.prompt(), fork));
        match result {
            Ok(id) => {
                self.close_message_edit(ctx);
                self.follow_edited_chat = Some(id.chat);
                self.focus_prompt = true;
                self.attachment_error = None;
                self.launch_prepared_prompt(ctx, id);
            }
            Err(error) => edit.error = Some(error),
        }
        ctx.request_repaint();
    }
}
