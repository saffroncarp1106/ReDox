use std::{borrow::Cow, path::PathBuf};

use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::attachments::Attachment;
use crate::codex::Rpc;
use crate::domain::{Chat, Message, MessageStatus, Role, Session};

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub name: Option<String>,
    #[serde(default)]
    pub preview: String,
    pub cwd: PathBuf,
    pub model: Option<String>,
}

impl HistoryEntry {
    pub fn title(&self) -> String {
        self.name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(&self.preview)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(100)
            .collect::<String>()
    }
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub data: Vec<HistoryEntry>,
    pub next_cursor: Option<String>,
}

pub struct ConversationPage {
    pub entry: HistoryEntry,
    pub messages: Vec<Message>,
    pub next_cursor: Option<String>,
    pub archived: bool,
}

pub struct CutoffFork {
    pub source: String,
    pub before_turn: String,
    pub thread: String,
}

pub async fn fork_before(
    executable: &str,
    source: String,
    before_turn: String,
) -> Result<CutoffFork, String> {
    let mut rpc = Rpc::connect(executable).await?;
    let result = async {
        let previous = previous_turn(&mut rpc, &source, &before_turn).await?;
        let fork = rpc
            .call(
                "thread/fork",
                json!({
                    "threadId":source, "beforeTurnId":before_turn, "excludeTurns":true
                }),
            )
            .await?;
        let thread = fork["thread"]["id"]
            .as_str()
            .filter(|id| !id.is_empty() && *id != source)
            .ok_or("Codex did not return a separate thread for this edit.")?;
        // Older servers can ignore unknown fields. Verify the cutoff before sending any prompt.
        let verified = turn_ids(&mut rpc, thread, None, 1).await.is_ok_and(|page| {
            page["data"].as_array().is_some_and(|turns| match previous {
                Some(id) => turns.first().and_then(|turn| turn["id"].as_str()) == Some(id.as_str()),
                None => turns.is_empty(),
            })
        });
        if !verified {
            let _ = rpc.call("thread/archive", json!({"threadId":thread})).await;
            return Err(
                "Codex could not verify the rewind. Update the Codex CLI and try again.".into(),
            );
        }
        Ok(CutoffFork {
            source,
            before_turn,
            thread: thread.to_owned(),
        })
    }
    .await;
    rpc.close().await;
    result
}

async fn previous_turn(
    rpc: &mut Rpc,
    thread: &str,
    before: &str,
) -> Result<Option<String>, String> {
    let mut cursor = None;
    let mut seen = false;
    loop {
        let page = turn_ids(rpc, thread, cursor.as_deref(), 64).await?;
        for turn in page["data"]
            .as_array()
            .ok_or("Codex did not return conversation turns.")?
        {
            let id = turn["id"]
                .as_str()
                .ok_or("Codex returned a turn without an ID.")?;
            if seen {
                return Ok(Some(id.to_owned()));
            }
            seen = id == before;
        }
        let Some(next) = page["nextCursor"].as_str() else {
            return if seen {
                Ok(None)
            } else {
                Err("This turn is no longer in the saved Codex thread. Reload the conversation to edit it.".into())
            };
        };
        if Some(next) == cursor.as_deref() {
            return Err("Codex could not page through the saved conversation.".into());
        }
        cursor = Some(next.to_owned());
    }
}

async fn turn_ids(
    rpc: &mut Rpc,
    thread: &str,
    cursor: Option<&str>,
    limit: u32,
) -> Result<Value, String> {
    rpc.call(
        "thread/turns/list",
        json!({
            "threadId":thread, "cursor":cursor, "limit":limit, "itemsView":"notLoaded",
            "sortDirection":"desc",
        }),
    )
    .await
}

pub async fn list(
    executable: &str,
    cursor: Option<String>,
    archived: bool,
    search: &str,
) -> Result<HistoryPage, String> {
    let mut rpc = Rpc::connect(executable).await?;
    let result = rpc
        .call(
            "thread/list",
            json!({
                "limit":50, "cursor":cursor, "archived":archived,
                "sortKey":"updated_at", "sortDirection":"desc", "modelProviders":[],
                "searchTerm":if search.trim().is_empty() { None } else { Some(search.trim()) },
            }),
        )
        .await;
    rpc.close().await;
    serde_json::from_value(result?)
        .map_err(|error| format!("Could not read Codex history: {error}"))
}

pub async fn read(
    executable: &str,
    thread: &str,
    cursor: Option<String>,
    archived: bool,
) -> Result<ConversationPage, String> {
    let mut rpc = Rpc::connect(executable).await?;
    let result = async {
        let metadata = rpc
            .call(
                "thread/read",
                json!({"threadId":thread,"includeTurns":false}),
            )
            .await?;
        let entry: HistoryEntry = serde_json::from_value(metadata["thread"].clone())
            .map_err(|error| format!("Could not read conversation details: {error}"))?;
        let turns = rpc.call("thread/turns/list", json!({
            "threadId":thread,"cursor":cursor,"limit":20,"itemsView":"full","sortDirection":"desc",
        })).await?;
        let mut messages = parse_turns(&turns, entry.model.as_deref().unwrap_or("Codex"))?;
        for attachment in messages
            .iter_mut()
            .flat_map(|message| &mut message.attachments)
        {
            if attachment.path.is_relative() {
                attachment.path = entry.cwd.join(&attachment.path);
            }
        }
        Ok(ConversationPage {
            entry,
            messages,
            next_cursor: turns["nextCursor"].as_str().map(str::to_owned),
            archived,
        })
    }
    .await;
    rpc.close().await;
    result
}

pub fn parse_turns(page: &Value, model: &str) -> Result<Vec<Message>, String> {
    let turns = page["data"]
        .as_array()
        .ok_or("Codex did not return conversation turns.")?;
    let mut messages = Vec::new();
    for turn in turns.iter().rev() {
        let items = turn["items"]
            .as_array()
            .ok_or("Codex returned a turn without items.")?;
        let turn_id = turn["id"].as_str().unwrap_or_default();
        let mut activity = crate::activity::TurnActivity::default();
        let mut last_reply = None;
        let mut first_user = true;
        for item in items {
            if let Some(command) = crate::activity::CommandActivity::from_item(item) {
                activity.record_command(command);
                continue;
            }
            let mut attachments = Vec::new();
            let (role, content) = match item["type"].as_str() {
                Some("userMessage") => (Role::User, user_content(item, &mut attachments)),
                Some("agentMessage") => (
                    Role::Assistant,
                    item["text"].as_str().unwrap_or_default().to_owned(),
                ),
                Some("imageGeneration") => (
                    Role::Assistant,
                    crate::media::generated_image(item).unwrap_or_default(),
                ),
                _ => continue,
            };
            if content.is_empty() && attachments.is_empty() {
                if role == Role::User {
                    first_user = false;
                }
                continue;
            }
            let failed = role == Role::Assistant && turn["status"] == "failed";
            let activity = if role == Role::Assistant {
                last_reply = Some(messages.len());
                std::mem::take(&mut activity)
            } else if std::mem::take(&mut first_user) {
                crate::activity::TurnActivity {
                    turn_id: turn_id.to_owned(),
                    ..Default::default()
                }
            } else {
                Default::default()
            };
            messages.push(Message {
                id: Uuid::new_v4(),
                role,
                content,
                status: if failed {
                    MessageStatus::Failed
                } else if role == Role::Assistant
                    && (turn["status"] == "interrupted" || turn["status"] == "inProgress")
                {
                    MessageStatus::Interrupted
                } else {
                    MessageStatus::Complete
                },
                error: if failed {
                    turn["error"]["message"].as_str().map(str::to_owned)
                } else {
                    None
                },
                model: model.to_owned(),
                attachments,
                activity,
            });
        }
        if let Some(index) = last_reply {
            let last = &mut messages[index];
            last.activity.turn_id = turn_id.to_owned();
            for mut command in activity.commands {
                command.offset = last.content.len();
                last.activity.record_command(command);
            }
        }
    }
    Ok(messages)
}

fn user_content(item: &Value, attachments: &mut Vec<Attachment>) -> String {
    let mut content = String::new();
    for part in item["content"].as_array().into_iter().flatten() {
        let text = match part["type"].as_str() {
            Some("text") => part["text"].as_str().map(Cow::Borrowed),
            Some("localImage") => {
                if let Some(path) = part["path"].as_str().filter(|path| !path.is_empty()) {
                    attachments.push(Attachment {
                        id: Uuid::new_v4(),
                        path: path.into(),
                        text: None,
                    });
                }
                None
            }
            Some("image") => Some(
                part["url"]
                    .as_str()
                    .map_or(Cow::Borrowed("[Image attachment unavailable]"), |url| {
                        Cow::Owned(crate::media::image_markdown(url))
                    }),
            ),
            _ => None,
        };
        if let Some(text) = text {
            if !content.is_empty() {
                content.push('\n');
            }
            content.push_str(&text);
        }
    }
    content
}

pub fn import(session: &mut Session, page: ConversationPage, older: bool) -> Uuid {
    let locally_archived = session.local_archive_overrides.get(&page.entry.id).copied();
    let index = session
        .chats
        .iter()
        .position(|chat| chat.thread_id.as_deref() == Some(&page.entry.id))
        .unwrap_or_else(|| {
            let mut chat = Chat::new(Some(page.entry.cwd.clone()));
            chat.imported_history = true;
            chat.thread_id = Some(page.entry.id.clone());
            session.chats.push(chat);
            session.chats.len() - 1
        });
    let chat = &mut session.chats[index];
    if !older && chat.has_prompt_versions() {
        return chat.id;
    }
    if older {
        let mut messages = page.messages;
        messages.append(&mut chat.messages);
        chat.messages = messages;
    } else {
        chat.thread_start = None;
        chat.title = page.entry.title();
        if chat.title.is_empty() {
            chat.title = "Codex conversation".into();
        }
        chat.archived = locally_archived.unwrap_or(page.archived);
        let mut messages = page.messages;
        for mut local in chat.messages.drain(..) {
            if local.role == Role::User
                && matches!(
                    local.status,
                    MessageStatus::Queued | MessageStatus::Interrupted
                )
            {
                messages.push(local);
            } else if local.role == Role::Assistant
                && !local.activity.turn_id.is_empty()
                && let Some(imported) = messages.iter_mut().find(|message| {
                    message.role == Role::Assistant
                        && message.activity.turn_id == local.activity.turn_id
                })
            {
                imported.activity.files = std::mem::take(&mut local.activity.files);
                imported.activity.preview_limited = local.activity.preview_limited;
            }
        }
        chat.messages = messages;
        if chat.archived {
            chat.interrupt_queue("Chat was archived before this message was sent.");
        }
    }
    chat.id
}
