pub mod activity;
pub mod attachments;
pub mod clipboard;
pub mod codex;
pub mod delegation;
pub mod desktop;
pub mod domain;
pub mod history;
pub mod media;
pub mod notifications;
pub mod ollama;
pub mod resources;
pub mod run;
pub mod updates;
pub mod usage;

pub mod discord_presence;

#[cfg(test)]
#[path = "../tests/perf/measure.rs"]
mod perf_measure;
