use super::*;
use crate::history::CutoffFork;
use crate::run::{RunEvent, Update, apply_event};

fn round(session: &mut Session, prompt: &str, reply: &str) -> (RunId, Uuid) {
    session.active_mut().unwrap().draft = prompt.into();
    let run = session.prepare_selected_run(None).unwrap();
    let chat = session.active().unwrap();
    let id = chat.messages[chat.messages.len() - 2].id;
    session.run_message_mut(run).unwrap().content = reply.into();
    (run, id)
}

fn finish(session: &mut Session, run: RunId) {
    session.finish_run(run, MessageStatus::Complete, None);
}

fn edit<'a>(session: &Session, message: Uuid, content: &'a str) -> PromptEdit<'a> {
    PromptEdit {
        chat: session.active_chat.unwrap(),
        message,
        content,
        removed_attachments: &[],
    }
}

fn visible(session: &Session) -> Vec<&str> {
    session
        .active()
        .unwrap()
        .messages
        .iter()
        .map(|m| m.content.as_str())
        .collect()
}

#[test]
fn editing_and_switching_versions_moves_only_the_affected_tail() {
    let mut session = Session::default();
    session.new_chat();
    let (first, first_prompt) = round(&mut session, "Original first", "Shared reply");
    finish(&mut session, first);
    let (second, changed) = round(&mut session, "Original second", "Original reply");
    finish(&mut session, second);
    let chat = session.active_mut().unwrap();
    chat.draft = "Unsent draft".into();
    let shared_pointer = chat.messages[0].content.as_ptr();
    let original_pointer = chat.messages[2].content.as_ptr();
    let draft = edit(&session, changed, "Revised second");
    let response = session.prepare_edited_run(draft, None).unwrap();
    assert!(session.switch_prompt_version(changed, true).is_err());
    session.run_message_mut(response).unwrap().content = "Revised reply".into();
    finish(&mut session, response);
    assert_eq!(
        visible(&session),
        [
            "Original first",
            "Shared reply",
            "Revised second",
            "Revised reply"
        ]
    );
    let chat = session.active().unwrap();
    assert_eq!(chat.messages[0].content.as_ptr(), shared_pointer);
    assert_eq!(chat.draft, "Unsent draft");
    assert_eq!(chat.prompt_version(changed), Some((2, 2)));

    session.switch_prompt_version(changed, true).unwrap();
    assert_eq!(
        visible(&session),
        [
            "Original first",
            "Shared reply",
            "Original second",
            "Original reply"
        ]
    );
    assert_eq!(
        session.active().unwrap().messages[2].content.as_ptr(),
        original_pointer
    );
    let third = edit(&session, changed, "Third second");
    let response = session.prepare_edited_run(third, None).unwrap();
    finish(&mut session, response);
    assert_eq!(
        session.active().unwrap().prompt_version(changed),
        Some((3, 3))
    );
    session.switch_prompt_version(changed, true).unwrap();
    assert_eq!(visible(&session)[2], "Revised second");
    let ancestor = edit(&session, first_prompt, "Revised first");
    let response = session.prepare_edited_run(ancestor, None).unwrap();
    finish(&mut session, response);
    assert_eq!(visible(&session), ["Revised first", ""]);
    session.switch_prompt_version(first_prompt, true).unwrap();
    assert_eq!(visible(&session)[2], "Revised second");
    assert_eq!(
        session.active().unwrap().prompt_version(changed),
        Some((2, 3))
    );
}

#[test]
fn edits_use_a_verified_codex_fork_and_preserve_the_original_thread() {
    let mut session = Session::default();
    session.new_chat();
    let (first, first_prompt) = round(&mut session, "First", "Prior reply");
    for update in [
        Update::Thread("original".into()),
        Update::Turn("one".into()),
    ] {
        apply_event(&mut session, Some(first), RunEvent { run: first, update });
    }
    finish(&mut session, first);
    let (second, changed) = round(&mut session, "Second", "Original reply");
    apply_event(
        &mut session,
        Some(second),
        RunEvent {
            run: second,
            update: Update::Turn("two".into()),
        },
    );
    finish(&mut session, second);
    assert_eq!(
        session
            .edit_fork_source(edit(&session, first_prompt, "Revised"))
            .unwrap(),
        None
    );
    let draft = edit(&session, changed, "Revised");
    assert_eq!(
        session.edit_fork_source(draft).unwrap(),
        Some(("original", "two"))
    );
    assert!(session.prepare_edited_run(draft, None).is_err());
    assert!(
        session
            .prepare_edited_run(
                draft,
                Some(CutoffFork {
                    source: "other".into(),
                    before_turn: "two".into(),
                    thread: "wrong".into(),
                })
            )
            .is_err()
    );
    assert_eq!(visible(&session)[2], "Second");
    let reply = session
        .prepare_edited_run(
            draft,
            Some(CutoffFork {
                source: "original".into(),
                before_turn: "two".into(),
                thread: "revised".into(),
            }),
        )
        .unwrap();
    let chat = session.active().unwrap();
    let request = crate::codex::ChatRequest::from_chat(chat, "test".into());
    assert_eq!(request.thread_id.as_deref(), Some("revised"));
    assert_eq!(request.prompt, "Revised");
    assert!(chat.inactive_threads().any(|thread| thread == "original"));
    finish(&mut session, reply);
    let mut session: Session =
        serde_json::from_slice(&serde_json::to_vec(&session).unwrap()).unwrap();
    session.recover();
    session.switch_prompt_version(changed, true).unwrap();
    assert_eq!(
        session.active().unwrap().thread_id.as_deref(),
        Some("original")
    );
    assert_eq!(
        session
            .edit_fork_source(edit(&session, changed, "Again"))
            .unwrap(),
        Some(("original", "two"))
    );
}

#[test]
fn an_ollama_edit_rotates_its_checkpoint_and_keeps_queued_work_with_its_version() {
    let mut session = Session::default();
    session.new_chat();
    session
        .active_mut()
        .unwrap()
        .set_provider(Provider::Ollama, "model".into());
    let (first, changed) = round(&mut session, "First", "Old output");
    finish(&mut session, first);
    let old_checkpoint = Uuid::new_v4();
    let chat = session.active_mut().unwrap();
    chat.ollama_checkpoint = Some(old_checkpoint);
    chat.draft = "Queued on the original version".into();
    let queued = session
        .queue_prompt_after(Duration::from_secs(600))
        .unwrap();
    let invalid = edit(&session, changed, "  ");
    assert!(session.prepare_edited_run(invalid, None).is_err());
    let draft = edit(&session, changed, "Fresh task");
    let reply = session.prepare_edited_run(draft, None).unwrap();
    let chat = session.active().unwrap();
    assert_ne!(chat.ollama_checkpoint, Some(old_checkpoint));
    assert!(!chat.send_after.contains_key(&queued));
    assert_eq!(visible(&session), ["Fresh task", ""]);
    finish(&mut session, reply);
    session.switch_prompt_version(changed, true).unwrap();
    assert_eq!(
        session.active().unwrap().ollama_checkpoint,
        Some(old_checkpoint)
    );
    assert!(session.active().unwrap().send_after.contains_key(&queued));
    session.switch_prompt_version(changed, false).unwrap();
    let mut session: Session =
        serde_json::from_slice(&serde_json::to_vec(&session).unwrap()).unwrap();
    session.recover();
    session.switch_prompt_version(changed, true).unwrap();
    let queued = session
        .active()
        .unwrap()
        .messages
        .iter()
        .find(|m| m.id == queued)
        .unwrap();
    assert_eq!(queued.status, MessageStatus::Interrupted);
}

#[test]
fn an_edit_can_remove_a_missing_image_without_changing_the_original_attachments() {
    let mut session = Session::default();
    session.new_chat();
    let (run, message) = round(&mut session, "Review these files", "Old reply");
    finish(&mut session, run);
    let missing = Uuid::new_v4();
    session.active_mut().unwrap().messages[0].attachments = vec![
        Attachment {
            id: missing,
            path: PathBuf::from(missing.to_string()).with_extension("png"),
            text: None,
        },
        Attachment {
            id: Uuid::new_v4(),
            path: "notes.txt".into(),
            text: Some("Captured notes".into()),
        },
    ];
    assert!(
        session
            .prepare_edited_run(edit(&session, message, "Review notes only"), None)
            .is_err()
    );
    let draft = PromptEdit {
        removed_attachments: &[missing],
        ..edit(&session, message, "Review notes only")
    };
    let run = session.prepare_edited_run(draft, None).unwrap();
    assert_eq!(session.active().unwrap().messages[0].attachments.len(), 1);
    finish(&mut session, run);
    session.switch_prompt_version(message, true).unwrap();
    assert_eq!(session.active().unwrap().messages[0].attachments.len(), 2);
}
