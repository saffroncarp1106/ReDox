use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::domain::Mode;

pub(crate) const REPORT_BYTES: usize = 3200;
pub(crate) type SharedReport = Arc<Mutex<Report>>;

#[derive(Default, Serialize)]
pub(crate) struct Report {
    pub status: String,
    pub summary: String,
    pub summary_truncated: bool,
    pub tool_calls: usize,
    pub evidence: Vec<Value>,
}

pub(crate) fn clipped(text: &str, limit: usize) -> String {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

#[derive(Deserialize, Serialize)]
pub(crate) struct Identity {
    pub workspace: Option<PathBuf>,
    pub mode: Mode,
    pub model: String,
}

pub(crate) struct Guard {
    history: Vec<u64>,
    warned: bool,
    progress_since_warning: usize,
    pub calls: usize,
    truncations: usize,
    truncated_output: u64,
}

impl Guard {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            warned: false,
            progress_since_warning: 0,
            calls: 0,
            truncations: 0,
            truncated_output: 0,
        }
    }

    pub fn truncated(&mut self, content: &str) -> bool {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        content.trim().hash(&mut hash);
        let output = hash.finish();
        self.truncations = if self.truncated_output == output {
            self.truncations + 1
        } else {
            1
        };
        self.truncated_output = output;
        self.truncations >= 3
    }

    pub fn observe(&mut self, name: &str, arguments: &Value, result: &str) -> Option<&'static str> {
        self.calls += 1;
        self.truncations = 0;
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        name.hash(&mut hash);
        arguments.to_string().hash(&mut hash);
        result.hash(&mut hash);
        if self.history.len() == 12 {
            self.history.remove(0);
        }
        self.history.push(hash.finish());
        let history = &self.history;
        let repeated = (1..=4).any(|width| {
            history.len() >= width * 3 && {
                let end = history.len();
                history[end - width..] == history[end - width * 2..end - width]
                    && history[end - width..] == history[end - width * 3..end - width * 2]
            }
        });
        if !repeated {
            if self.warned {
                self.progress_since_warning += 1;
                if self.progress_since_warning >= 12 {
                    self.warned = false;
                }
            }
            return None;
        }
        self.progress_since_warning = 0;
        if self.warned {
            Some("stalled")
        } else {
            self.warned = true;
            Some("recover")
        }
    }
}

pub(crate) fn evidence(name: &str, arguments: &Value, result: &Result<String, String>) -> Value {
    match name {
        "run_command" => {
            json!({"command": clipped(arguments["command"].as_str().unwrap_or_default(), 160),
            "result": clipped(result.as_ref().map(String::as_str).unwrap_or_else(|error| error.as_str()).lines().next().unwrap_or_default(), 160)})
        }
        _ => {
            json!({"tool":name,"path":clipped(arguments["path"].as_str().unwrap_or_default(), 240),"ok":result.is_ok()})
        }
    }
}

pub(crate) async fn snapshot(root: &Path) -> Result<BTreeMap<String, u64>, String> {
    tokio::time::timeout(std::time::Duration::from_secs(10), snapshot_inner(root))
        .await
        .map_err(|_| "Git change inventory timed out.".to_owned())?
}

async fn snapshot_inner(root: &Path) -> Result<BTreeMap<String, u64>, String> {
    let mut paths = Vec::new();
    for args in [
        vec![
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--name-only",
            "-z",
            "HEAD",
            "--",
        ],
        vec!["ls-files", "--others", "--exclude-standard", "-z"],
    ] {
        let mut command = tokio::process::Command::new("git");
        command.args(args).current_dir(root).kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let output = tokio::time::timeout(std::time::Duration::from_secs(10), command.output())
            .await
            .map_err(|_| "Git change inventory timed out.")?
            .map_err(|error| error.to_string())?;
        if !output.status.success() {
            return Err("Git change inventory unavailable; inspect the workspace manually.".into());
        }
        paths.extend(
            output
                .stdout
                .split(|byte| *byte == 0)
                .filter(|path| !path.is_empty())
                .map(|path| String::from_utf8_lossy(path).into_owned()),
        );
    }
    let mut result = BTreeMap::new();
    for path in paths {
        let full = root.join(&path);
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        match tokio::fs::File::open(&full).await {
            Ok(mut file) => {
                if !tokio::fs::canonicalize(&full)
                    .await
                    .map_err(|error| error.to_string())?
                    .starts_with(root)
                {
                    return Err(format!("Change inventory skipped external link: {path}"));
                }
                use tokio::io::AsyncReadExt;
                let mut buffer = [0u8; 16384];
                loop {
                    let count = file
                        .read(&mut buffer)
                        .await
                        .map_err(|error| error.to_string())?;
                    if count == 0 {
                        break;
                    }
                    hash.write(&buffer[..count]);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => "deleted".hash(&mut hash),
            Err(error) => return Err(format!("Cannot inventory {path}: {error}")),
        }
        result.insert(path, hash.finish());
    }
    Ok(result)
}

#[cfg(test)]
#[path = "../tests/transport/delegation_worker.rs"]
mod tests;
