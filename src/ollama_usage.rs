use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static USAGE_LOCK: Mutex<()> = Mutex::new(());

fn usage_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|root| root.join("ReDox").join("ollama-lifetime-tokens"))
}

fn read_total(path: &Path) -> io::Result<u64> {
    match std::fs::read_to_string(path) {
        Ok(total) => total
            .parse()
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(err) => Err(err),
    }
}

pub fn lifetime_tokens() -> io::Result<u64> {
    usage_path().map_or(Ok(0), |path| read_total(&path))
}

fn add_tokens(path: &Path, tokens: u64) -> io::Result<()> {
    let _guard = USAGE_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let total = read_total(path)?.saturating_add(tokens);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let pending = path.with_extension(uuid::Uuid::new_v4().to_string());
    let result =
        std::fs::write(&pending, total.to_string()).and_then(|()| std::fs::rename(&pending, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&pending);
    }
    result
}

pub(super) fn record_reported_tokens(stats: &super::Stats) {
    if cfg!(test) {
        return;
    }
    let tokens = stats.prompt_tokens.saturating_add(stats.output_tokens);
    if tokens != 0
        && let Some(path) = usage_path()
        && let Err(err) = add_tokens(&path, tokens)
    {
        eprintln!("Could not save Ollama lifetime token usage: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reported_tokens_accumulate_on_disk() {
        let directory = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        let path = directory.join("usage");
        assert_eq!(read_total(&path).unwrap(), 0);
        add_tokens(&path, 42).unwrap();
        add_tokens(&path, 18).unwrap();
        assert_eq!(read_total(&path).unwrap(), 60);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
