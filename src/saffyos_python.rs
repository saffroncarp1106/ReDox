use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use cap_std::fs::Dir;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use super::{saffyos_tools, tools};

const SCRIPT_TIMEOUT: Duration = Duration::from_secs(15);
const TOTAL_TIMEOUT: Duration = Duration::from_secs(25);
const INPUT_LIMIT: u64 = 32 * 1024 * 1024;

#[derive(Deserialize)]
struct Run {
    path: String,
    #[serde(default)]
    inputs: Vec<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    stdin: String,
}

pub(super) fn runtime_folder() -> Option<PathBuf> {
    let data = dirs::data_local_dir()?;
    let data = data.canonicalize().unwrap_or(data);
    let folder = data.join("ReDox/saffyos-runtime");
    Some(folder.canonicalize().unwrap_or(folder))
}

pub(super) async fn execute(folder: &Dir, arguments: &Value) -> Result<String, String> {
    let run: Run = match arguments.as_str() {
        Some(text) => serde_json::from_str(text),
        None => Run::deserialize(arguments),
    }
    .map_err(|error| format!("Invalid run_python arguments: {error}"))?;
    let script = saffyos_tools::relative_path(&run.path)?;
    if !script
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("py"))
    {
        return Err("run_python requires a workspace-relative .py file.".into());
    }
    if run.inputs.len() > 16
        || run.args.len() > 16
        || run.args.iter().any(|argument| argument.len() > 1024)
        || run.stdin.len() > 16 * 1024
    {
        return Err(
            "Pass at most 16 input files, 16 arguments of up to 1 KiB, and 16 KiB of stdin.".into(),
        );
    }
    let inputs = run
        .inputs
        .iter()
        .map(|path| saffyos_tools::relative_path(path))
        .collect::<Result<Vec<_>, _>>()?;

    let runtime = runtime_folder().ok_or("Application data folder unavailable.")?;
    let engine = runtime.join(format!("wasmtime{}", std::env::consts::EXE_SUFFIX));
    let python = runtime.join("python.wasm");
    if !engine.is_file() || !python.is_file() {
        return Err("Sandboxed Python is unavailable. Install the SaffyOS Python runtime described in the ReDox README.".into());
    }

    let stage = tempfile::Builder::new()
        .prefix("redox-saffyos-")
        .tempdir()
        .map_err(|error| format!("Cannot create Python sandbox: {error}"))?;
    let mut remaining = INPUT_LIMIT;
    for path in std::iter::once(&script).chain(&inputs) {
        saffyos_tools::stage_input(folder, path, stage.path(), &mut remaining)?;
    }

    let mut mapping = OsString::from(stage.path());
    mapping.push("::.");
    let mut command = Command::new(engine);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command
        .current_dir(&runtime)
        .env_clear()
        .arg("run")
        .args(["-C", "cache=n", "-S",
            "inherit-network=n,allow-ip-name-lookup=n,tcp=n,udp=n,http=n,inherit-env=n,hostcall-fuel=100000000",
        ])
        .arg("-W")
        .arg(format!("timeout={}s,max-memory-size=268435456", SCRIPT_TIMEOUT.as_secs()))
        .arg("--dir")
        .arg(mapping)
        .arg(python)
        .args(["-B", "-u"])
        .arg(Path::new(".").join(&script).to_string_lossy().replace('\\', "/"))
        .args(&run.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("Cannot start sandboxed Python: {error}"))?;

    let mut stdin = child.stdin.take().ok_or("Python stdin unavailable.")?;
    let stdout = child.stdout.take().ok_or("Python stdout unavailable.")?;
    let stderr = child.stderr.take().ok_or("Python stderr unavailable.")?;
    let feed_input = async move {
        match stdin.write_all(run.stdin.as_bytes()).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    };
    let (status, stdout, stderr, input) = tokio::time::timeout(TOTAL_TIMEOUT, async {
        tokio::join!(
            child.wait(),
            tools::read_output(stdout),
            tools::read_output(stderr),
            feed_input
        )
    })
    .await
    .map_err(|_| "Sandboxed Python exceeded its 25-second total time limit.")?;
    input?;
    Ok(json!({
        "exit_code": status.map_err(|error| error.to_string())?.code(),
        "stdout": stdout?,
        "stderr": stderr?,
    })
    .to_string())
}
