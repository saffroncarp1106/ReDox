"""Ollama-compatible local endpoint that serves Aisnley-Ox after qualification."""

import argparse
from contextlib import contextmanager
from http.server import ThreadingHTTPServer
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
from urllib.parse import urlsplit

from ollama_http import make_handler


MODEL = "Aisnley-Ox-1-bbb:35b"
ALIASES = {MODEL.casefold(), "redox-qwen-rust:35b"}
DEFAULT_CONTEXT = 64000
DEFAULT_OUTPUT = 8192
KEEP_WARM = 900


class NativeQualifier:
    def __init__(self, executable, upstream):
        self.executable = executable
        self.upstream = upstream
        self.process = None
        self._lifecycle = threading.Lock()
        self._closed = False

    def __call__(self, context, cpu_only):
        with self._lifecycle:
            if self._closed:
                raise RuntimeError("Qualification service is shutting down.")
            if self.process is None or self.process.poll() is not None:
                if self.process is not None:
                    self._stop(self.process)
                self.process = subprocess.Popen(
                    [self.executable], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                    text=True, encoding="utf-8", bufsize=1,
                )
            process = self.process
        try:
            settings = {
                "endpoint": self.upstream, "preferred_model": MODEL,
                "context_tokens": context, "output_tokens": DEFAULT_OUTPUT, "cpu_only": cpu_only,
            }
            process.stdin.write(json.dumps(settings) + "\n")
            process.stdin.flush()
            line = process.stdout.readline()
        except (OSError, ValueError) as error:
            self.close()
            raise RuntimeError(f"Qualification runner disconnected: {error}") from error
        if not line:
            self.close()
            raise RuntimeError("Qualification runner exited before reporting a result.")
        result = json.loads(line)
        if result.get("error"):
            raise RuntimeError(result["error"])
        history = result.get("history")
        if not isinstance(history, list) or len(history) != 22:
            raise RuntimeError("Qualification runner did not return a passing 11-task transcript.")
        return history

    def close(self, permanent=False):
        with self._lifecycle:
            self._closed |= permanent
            process = self.process
            self.process = None
        if process is not None:
            self._stop(process)

    @staticmethod
    def _stop(process):
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        process.stdin.close()
        process.stdout.close()


def is_profile(model):
    return isinstance(model, str) and model.casefold() in ALIASES


def is_unload(path, body):
    return (path in ("/api/chat", "/api/generate")
            and body.get("keep_alive") in (0, "0", "0s")
            and not body.get("messages") and not body.get("prompt"))


def insert_history(messages, history):
    index = 0
    while index < len(messages) and isinstance(messages[index], dict) and messages[index].get("role") in ("system", "developer"):
        index += 1
    return [*messages[:index], *history, *messages[index:]]


class QualifiedRequests:
    def __init__(self, qualify):
        self.qualify = qualify
        self._lock = threading.Lock()

    @contextmanager
    def inference(self, path, body):
        if not is_profile(body.get("model")):
            yield body
            return
        # Keep the qualified runner selected until forwarding finishes.
        with self._lock:
            yield self.prepare(path, body)

    def prepare(self, path, body):
        body["model"] = MODEL
        if is_unload(path, body):
            return body
        options = body.get("options", {})
        if not isinstance(options, dict):
            raise ValueError("Ollama options must be a JSON object.")
        native = path.startswith("/api/") or path == "/_redox/qualify"
        context = options.get("num_ctx", DEFAULT_CONTEXT) if native else DEFAULT_CONTEXT
        if type(context) is not int or not 2048 <= context <= 262144:
            raise ValueError("Aisnley-Ox context size must be from 2048 to 262144 tokens.")
        history = self.qualify(context, native and options.get("num_gpu") == 0)
        if native:
            body["options"] = {**options, "num_ctx": context}
            body["keep_alive"] = KEEP_WARM

        messages = body.get("messages")
        if isinstance(messages, list) and messages:
            body["messages"] = insert_history(messages, history)
        elif path == "/v1/responses":
            input_value = body.get("input")
            if isinstance(input_value, str):
                body["input"] = [*history, {"role": "user", "content": input_value}]
            elif isinstance(input_value, list):
                body["input"] = insert_history(input_value, history)
        elif path in ("/api/generate", "/v1/completions") and not body.get("context"):
            prompt = body.get("prompt")
            if isinstance(prompt, str) and prompt:
                transcript = "\n".join(f"{item['role']}: {item['content']}" for item in history)
                body["prompt"] = f"Prior qualification conversation:\n{transcript}\n\nCurrent prompt:\n{prompt}"
        return body


def main():
    root = Path(__file__).resolve().parents[1]
    suffix = ".exe" if os.name == "nt" else ""
    target = Path(os.environ.get("CARGO_TARGET_DIR", root / "target"))
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=11435)
    parser.add_argument("--upstream", default="http://127.0.0.1:11434")
    parser.add_argument("--runner", type=Path, default=target / "debug" / f"ollama-qualifier{suffix}")
    parser.add_argument("--exit-on-stdin", action="store_true", help=argparse.SUPPRESS)
    options = parser.parse_args()
    upstream = urlsplit(options.upstream)
    if (upstream.scheme != "http" or upstream.hostname not in ("127.0.0.1", "localhost")
            or not upstream.port or upstream.port == options.port or upstream.path not in ("", "/")
            or upstream.username or upstream.password or upstream.query or upstream.fragment):
        parser.error("--upstream must be a different local HTTP port")
    if not options.runner.is_file():
        parser.error("build the qualification runner with cargo build --locked --bin ollama-qualifier")

    native = NativeQualifier(options.runner.resolve(), options.upstream)
    requests = QualifiedRequests(native)
    try:
        with ThreadingHTTPServer(("127.0.0.1", options.port), make_handler(options.upstream, requests.inference)) as service:
            print(f"Qualified Ollama endpoint http://127.0.0.1:{service.server_port}", flush=True)
            if options.exit_on_stdin:
                def exit_on_stdin():
                    sys.stdin.buffer.read(1)
                    service.shutdown()

                threading.Thread(target=exit_on_stdin, daemon=True).start()
            service.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        native.close(permanent=True)


if __name__ == "__main__":
    main()
