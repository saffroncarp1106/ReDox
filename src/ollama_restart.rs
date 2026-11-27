use std::time::Duration;

use super::api_url;

pub(super) fn supported(endpoint: &str) -> bool {
    cfg!(windows)
        && api_url(endpoint, "version").is_ok_and(|url| {
            url.scheme() == "http"
                && url.port() == Some(11434)
                && url.path() == "/api/version"
                && matches!(url.host_str(), Some("127.0.0.1" | "[::1]"))
        })
}

#[cfg(windows)]
async fn restart_process() -> Result<(), String> {
    use std::ffi::OsString;
    use std::path::PathBuf;
    use std::process::Stdio;
    use tokio::process::Command;
    use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

    static RESTART: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    // Complete the stop/start pair even if the user cancels the waiting turn.
    tokio::spawn(async {
        let _guard = RESTART.lock().await;
        let powershell = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .ok_or("Could not locate Windows PowerShell.")?
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let mut script = OsString::from("$ErrorActionPreference = 'Stop'\n");
        script.push(include_str!("ollama_restart.ps1"));
        let output = tokio::time::timeout(
            Duration::from_secs(30),
            Command::new(powershell)
                .args(["-NoProfile", "-NonInteractive", "-Command"])
                .arg(script)
                .stdin(Stdio::null())
                .creation_flags(CREATE_NO_WINDOW)
                .kill_on_drop(true)
                .output(),
        )
        .await
        .map_err(|_| "Timed out restarting Ollama.".to_owned())?
        .map_err(|error| format!("Could not start Ollama recovery: {error}"))?;
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(error.trim().chars().take(600).collect());
        }
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(not(windows))]
async fn restart_process() -> Result<(), String> {
    Err("Local Ollama restart is available on Windows.".into())
}

pub(super) async fn local_ollama(endpoint: &str) -> Result<(), String> {
    let url = api_url(endpoint, "version")?;
    let client = super::saffyos_client(endpoint)?;
    restart_process().await?;
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let version = client
                .get(url.clone())
                .timeout(Duration::from_secs(1))
                .send()
                .await;
            if let Ok(response) = version
                && response.status().is_success()
                && let Ok(body) = response.json::<serde_json::Value>().await
                && body["version"].as_str().is_some()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    })
    .await
    .map_err(|_| "Ollama did not become ready after restart.".into())
}

#[cfg(test)]
#[path = "../tests/transport/ollama_restart.rs"]
mod tests;
