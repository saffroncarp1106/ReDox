use std::io::{self, BufRead, Write};

use redox::codex::ProviderEvent;
use redox::ollama::{Settings, qualify_model};
use serde_json::json;
use tokio::sync::{mpsc, watch};

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    for line in io::stdin().lock().lines() {
        let response = match serde_json::from_str::<Settings>(&line?) {
            Ok(settings) => match qualify(settings).await {
                Ok(history) => json!({"history":history}),
                Err(error) => json!({"error":error}),
            },
            Err(error) => json!({"error":format!("Invalid qualification settings: {error}")}),
        };
        let mut output = io::stdout().lock();
        writeln!(output, "{response}")?;
        output.flush()?;
    }
    Ok(())
}

async fn qualify(settings: Settings) -> Result<Vec<serde_json::Value>, String> {
    let (sender, mut events) = mpsc::channel(64);
    let (_stop, cancel) = watch::channel(false);
    let (result, ()) = tokio::join!(qualify_model(settings, sender, cancel), async {
        while let Some(event) = events.recv().await {
            if let ProviderEvent::Status(text) = event {
                eprintln!("{text}");
            }
        }
    });
    result
}
