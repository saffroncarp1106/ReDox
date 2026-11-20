"""Direct, cancellable access to Ollama's native API."""

from collections import deque
from contextlib import contextmanager, suppress
import http.client
import json
import queue
import socket
import threading
import time
from urllib.parse import urlsplit, urlunsplit
from urllib.request import Request, urlopen


LOCAL_OLLAMA = "http://127.0.0.1:11434"
READ_FIXTURE = {
    "type": "function",
    "function": {
        "name": "read_file",
        "description": "Read the named benchmark fixture.",
        "parameters": {
            "type": "object",
            "properties": {"path": {"type": "string"}},
            "required": ["path"],
        },
    },
}
INSTRUCTIONS = {
    "general": ("Solve each benchmark task independently unless it asks you to recall a conversational value. "
                "Use read_file when the task asks for a fixture. Return the requested JSON as your final answer."),
    "rust": ("Return the complete Rust source for the requested solution.rs module. Use the standard library and "
             "the exact public signatures in the task. Return plain Rust or one Rust code fence."),
}


class BenchmarkError(Exception):
    pass


def api_url(endpoint, route):
    target = urlsplit(endpoint.strip())
    if (target.scheme not in ("http", "https") or not target.hostname
            or target.username is not None or target.query or target.fragment):
        raise ValueError("Enter an Ollama URL such as http://127.0.0.1:11434.")
    # Accessing port validates it before starting a worker.
    target.port
    path = target.path.rstrip("/").removesuffix("/api")
    return urlunsplit((target.scheme, target.netloc, f"{path}/api/{route}", "", ""))


def installed_models(endpoint):
    with urlopen(api_url(endpoint, "tags"), timeout=5) as response:
        data = json.load(response)
    return sorted({entry["name"] for entry in data["models"]})


def unload_model(endpoint, model):
    body = json.dumps({"model": model, "keep_alive": 0, "stream": False}).encode()
    request = Request(api_url(endpoint, "generate"), data=body, headers={"Content-Type": "application/json"})
    with urlopen(request, timeout=5) as response:
        response.read()


@contextmanager
def chat_events(endpoint, payload, deadline, control, progress):
    target = urlsplit(api_url(endpoint, "chat"))
    connector = http.client.HTTPSConnection if target.scheme == "https" else http.client.HTTPConnection
    events = queue.Queue(maxsize=128)
    cancelled = threading.Event()
    lock = threading.Lock()
    active_socket = None

    def publish(kind, value):
        while not cancelled.is_set():
            try:
                events.put((kind, value), timeout=0.1)
                return
            except queue.Full:
                continue

    def fetch():
        nonlocal active_socket
        connection = connector(target.hostname, target.port, timeout=min(5, max(0.1, deadline - time.monotonic())))
        try:
            connection.connect()
            with lock:
                if cancelled.is_set():
                    return
                active_socket = connection.sock
                active_socket.settimeout(max(0.1, deadline - time.monotonic()))
            connection.request("POST", target.path, json.dumps(payload).encode(),
                               {"Content-Type": "application/json", "Connection": "close"})
            with connection.getresponse() as response:
                if response.status != 200:
                    body = response.read(4096).decode("utf-8", errors="replace")
                    try:
                        body = json.loads(body)["error"]
                    except (ValueError, TypeError, KeyError):
                        pass
                    raise BenchmarkError(f"Ollama HTTP {response.status}: {body or response.reason}")
                for line in response:
                    if cancelled.is_set():
                        return
                    if line.strip():
                        publish("data", json.loads(line))
        except (BenchmarkError, OSError, ValueError, http.client.HTTPException) as error:
            publish("error", str(error))
        finally:
            connection.close()
            publish("end", None)

    def receive():
        while (remaining := deadline - time.monotonic()) > 0:
            control.check_running()
            try:
                kind, value = events.get(timeout=min(0.2, remaining))
            except queue.Empty:
                progress("waiting", "")
                continue
            control.check_running()
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                break
            if kind == "error":
                raise BenchmarkError(value)
            if kind == "end":
                break
            yield value
        raise BenchmarkError("Ollama stream ended before the task finished."
                             if remaining > 0 else "Watchdog expired before the task finished.")

    worker = threading.Thread(target=fetch, daemon=True)
    worker.start()
    try:
        yield receive()
    finally:
        with lock:
            cancelled.set()
            if active_socket is not None:
                with suppress(OSError):
                    active_socket.shutdown(socket.SHUT_RDWR)
        worker.join(timeout=0.2)


def complete(endpoint, payload, deadline, control, progress):
    content, thinking, calls = [], [], []
    with chat_events(endpoint, payload, deadline, control, progress) as events:
        for event in events:
            if not isinstance(event, dict):
                raise BenchmarkError("Ollama returned an invalid chat response.")
            if event.get("error"):
                raise BenchmarkError(str(event["error"]))
            message = event.get("message") or {}
            delta = message.get("content") or ""
            thought = message.get("thinking") or ""
            content.append(delta)
            thinking.append(thought)
            calls.extend(message.get("tool_calls") or [])
            if delta or thought:
                progress("generating" if delta else "thinking", delta or thought)
            if event.get("done"):
                reply = {"role": "assistant", "content": "".join(content)}
                if calls:
                    reply["tool_calls"] = calls
                    if any(thinking):
                        reply["thinking"] = "".join(thinking)
                return reply, event


def token_estimate(messages):
    return sum(8 + len(json.dumps(message, ensure_ascii=False)) // 3 for message in messages)


class ChatSession:
    def __init__(self, endpoint, context):
        self.endpoint = endpoint
        self.context = context
        self.history = deque()
        self.remembered = []
        self.marker = None
        self.history_tokens = 0

    def run(self, prompt, model, suite, output, keep_alive, deadline, control, progress, fixture=None, remember=False):
        system = {"role": "system", "content": INSTRUCTIONS[suite]}
        turn = [{"role": "user", "content": prompt}]
        usage = {"output_tokens": 0, "eval_seconds": 0.0, "requests": 0}
        rollovers = file_reads = 0
        for request in range(9):
            budget = max(self.context // 2, self.context - output)
            fixed_tokens = token_estimate([system, *self.remembered, *turn])
            if fixture is not None:
                fixed_tokens += len(json.dumps(READ_FIXTURE)) // 3
            dropped = False
            while self.history and fixed_tokens + self.history_tokens > budget:
                oldest, tokens = self.history.popleft()
                self.history_tokens -= tokens
                if oldest is self.marker:
                    self.remembered = oldest
                    fixed_tokens += tokens
                dropped = True
            rollovers += int(dropped)
            messages = [system, *self.remembered, *(msg for exchange, _ in self.history for msg in exchange), *turn]
            payload = {"model": model, "messages": messages, "stream": True,
                       "options": {"num_ctx": self.context, "num_predict": output}}
            if keep_alive is not None:
                payload["keep_alive"] = keep_alive
            if fixture is not None:
                payload["tools"] = [READ_FIXTURE]
            try:
                reply, metrics = complete(self.endpoint, payload, deadline, control, progress)
            except BenchmarkError as error:
                if fixture is not None and "does not support tools" in str(error).lower():
                    return {"text": "", "error": str(error), "stats": usage, "rollovers": rollovers,
                            "file_reads": file_reads}
                raise
            usage["requests"] += 1
            usage["output_tokens"] += metrics.get("eval_count") or 0
            usage["eval_seconds"] += (metrics.get("eval_duration") or 0) / 1_000_000_000
            turn.append(reply)
            calls = reply.get("tool_calls", [])
            if not calls:
                if remember:
                    self.marker = turn
                    self.remembered = []
                tokens = token_estimate(turn)
                self.history.append((turn, tokens))
                self.history_tokens += tokens
                return {"text": reply["content"], "error": None, "stats": usage,
                        "rollovers": rollovers, "file_reads": file_reads}
            if fixture is None or request == 8:
                raise BenchmarkError("Unexpected tool call or benchmark tool-call limit reached.")
            progress("reading fixture", "")
            for call in calls:
                function = call.get("function") or {}
                arguments = function.get("arguments") or {}
                name = function.get("name") or "unknown"
                if name == "read_file" and isinstance(arguments, dict) and arguments.get("path") == fixture.name:
                    result = fixture.read_text(encoding="utf-8")
                    file_reads += 1
                else:
                    result = "Only the fixture named in this task is available."
                turn.append({"role": "tool", "tool_name": name, "content": result})
