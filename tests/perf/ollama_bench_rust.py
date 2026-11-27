"""Build model-generated Rust through a benchmark-owned Cargo crate."""

from dataclasses import dataclass
import os
import re
import signal
import subprocess
import time


MANIFEST = """[package]
name = "redox-rust-benchmark"
version = "0.1.0"
edition = "2021"

[workspace]

[profile.dev]
incremental = false
"""

CODE_BLOCK = re.compile(r"^[ \t]*```([^\n`]*)\n(.*?)^[ \t]*```[ \t]*$", re.MULTILINE | re.DOTALL)


def rust_source(reply):
    text = reply.strip().replace("\r\n", "\n")
    blocks = list(CODE_BLOCK.finditer(text))
    if blocks:
        if len(blocks) != 1 or blocks[0][1].strip().lower() not in ("", "rust", "rs"):
            raise ValueError("Return one complete Rust source block.")
        text = blocks[0][2].strip()
    elif text.startswith("```"):
        raise ValueError("The Rust source block is incomplete.")
    if not text:
        raise ValueError("The model returned no Rust source.")
    return text + "\n"


@dataclass(frozen=True)
class BuildResult:
    status: str
    seconds: float | None
    diagnostic: str = ""

    @property
    def scored(self):
        return self.status in ("compiled", "compile_error", "invalid_submission")


def terminate_build(process):
    if process.poll() is not None:
        return
    if os.name == "nt":
        try:
            subprocess.run(["taskkill", "/F", "/T", "/PID", str(process.pid)],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                           creationflags=subprocess.CREATE_NO_WINDOW, timeout=5)
        except (OSError, subprocess.TimeoutExpired):
            pass
    else:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    if process.poll() is None:
        process.kill()
    process.wait()


class RustBuild:
    def __init__(self, workspace, target, toolchain_directory):
        self.workspace = workspace.resolve()
        self.source = self.workspace / "src"
        self.target = target.resolve()
        self.toolchain_directory = toolchain_directory
        self.source.mkdir(parents=True)
        (self.workspace / "Cargo.toml").write_text(MANIFEST, encoding="utf-8")

    def check(self, reply, harness, timeout, stop):
        try:
            source = rust_source(reply)
        except ValueError as error:
            return BuildResult("invalid_submission", None, str(error))

        (self.source / "main.rs").write_text(harness, encoding="utf-8")
        (self.source / "solution.rs").write_text(source, encoding="utf-8")
        if stop.is_set():
            return BuildResult("cancelled", None)
        started = time.monotonic()
        deadline = started + timeout
        log_path = self.workspace / "cargo-output.txt"
        with log_path.open("wb") as log:
            try:
                process = subprocess.Popen(
                    ["cargo", "build", "--offline", "--color", "never", "--message-format", "short",
                     "--manifest-path", self.workspace / "Cargo.toml", "--target-dir", self.target],
                    cwd=self.toolchain_directory, stdin=subprocess.DEVNULL, stdout=log,
                    stderr=subprocess.STDOUT, start_new_session=os.name != "nt",
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0,
                )
            except OSError as error:
                return BuildResult("tool_error", None, str(error))

            try:
                while True:
                    if stop.is_set():
                        return BuildResult("cancelled", time.monotonic() - started)
                    remaining = deadline - time.monotonic()
                    if remaining <= 0:
                        return BuildResult("timeout", time.monotonic() - started,
                                           f"Cargo build exceeded {timeout} seconds.")
                    try:
                        code = process.wait(timeout=min(remaining, 0.20))
                        break
                    except subprocess.TimeoutExpired:
                        continue
            finally:
                terminate_build(process)

        elapsed = time.monotonic() - started
        if code == 0:
            return BuildResult("compiled", elapsed)
        with log_path.open("rb") as log:
            diagnostic = log.read(4000).decode("utf-8", errors="replace").strip()
        return BuildResult("compile_error", elapsed, diagnostic or f"Cargo exited with status {code}.")
