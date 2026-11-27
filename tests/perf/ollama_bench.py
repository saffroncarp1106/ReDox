"""Score a single ReDox Ollama chat."""

import argparse
from contextlib import ExitStack, contextmanager
from copy import copy
from dataclasses import dataclass
import json
import os
from pathlib import Path
import queue
import random
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from urllib.error import HTTPError
from urllib.parse import urlsplit
from urllib.request import Request, urlopen

from ollama_bench_cases import grade, make_case


ROOT = Path(__file__).resolve().parents[2]
QUALIFIED_ENDPOINT = "http://127.0.0.1:11435"
LOCAL_OLLAMA = "http://127.0.0.1:11434"
QUALIFIED_MODELS = {"aisnley-ox-1-bbb:35b", "redox-qwen-rust:35b"}


class BenchmarkError(Exception):
    pass


class BenchmarkStopped(Exception):
    pass


class RunControl:
    def __init__(self, model):
        self._selection = model
        self._lock = threading.Lock()
        self.stop = threading.Event()

    def select_model(self, model):
        with self._lock:
            self._selection = model

    def selected_model(self):
        with self._lock:
            return self._selection

    def check_running(self):
        if self.stop.is_set():
            raise BenchmarkStopped


@dataclass
class Score:
    passed: int = 0
    failed: int = 0
    passed_ex_json: int | None = None
    inconclusive: int = 0
    generated: int = 0
    requests: int = 0
    rollovers: int = 0
    eval_seconds: float = 0.0
    task_seconds: float = 0.0

    def record(self, passed, usage=None, rollovers=0, *, elapsed=0.0, passed_ex_json=None):
        if passed is None:
            self.inconclusive += 1
        else:
            self.passed += int(passed)
            self.failed += int(not passed)
        if passed_ex_json is not None:
            self.passed_ex_json = (self.passed_ex_json or 0) + int(passed_ex_json)
        self.task_seconds += elapsed
        self.rollovers += rollovers
        if usage is not None:
            self.generated += usage["output_tokens"]
            self.eval_seconds += usage["eval_seconds"]
            self.requests += usage["requests"]

    def __str__(self):
        total = self.passed + self.failed
        speed = self.generated / self.eval_seconds if self.eval_seconds else 0
        skipped = f" | {self.inconclusive} inconclusive" if self.inconclusive else ""
        counts = f"{self.passed}/{total} passed | {self.failed} failed{skipped}"
        if self.passed_ex_json is not None:
            counts = f"True {self.passed}/{total} | Ex. JSON {self.passed_ex_json}/{total} passed"
        return (f"{counts} | {self.task_seconds:,.2f}s total task time | "
                f"{self.generated:,} generated tokens | {speed:.1f} tok/s | "
                f"{self.rollovers} rollovers | {self.requests} requests")


def read_events(pipe, events, finished):
    def publish(event):
        while not finished.is_set():
            try:
                events.put(event, timeout=0.20)
                return
            except queue.Full:
                continue

    try:
        with pipe:
            for line in pipe:
                if finished.is_set():
                    return
                publish(json.loads(line))
    except (OSError, ValueError) as error:
        publish({"event": "broken", "text": str(error)})
    finally:
        publish({"event": "broken", "text": "ReDox runner exited."})


def receive(events, deadline, control, show_wait=lambda: None):
    while (remaining := deadline - time.monotonic()) > 0:
        control.check_running()
        try:
            event = events.get(timeout=min(remaining, 1))
        except queue.Empty:
            show_wait()
            continue
        control.check_running()
        if event["event"] == "broken":
            raise BenchmarkError(event["text"])
        return event
    raise BenchmarkError("Watchdog expired before the task finished.")


def short(text, limit=70):
    return "".join(char if " " <= char <= "~" else " " for char in text[:limit])


def duration(seconds):
    hours, remainder = divmod(int(seconds), 3600)
    minutes, seconds = divmod(remainder, 60)
    return f"{hours:02d}:{minutes:02d}:{seconds:02d}"


def run_task(process, events, prompt, label, model, watchdog, control, notify, **settings):
    process.stdin.write(json.dumps({"prompt": prompt, "model": model, **settings}) + "\n")
    process.stdin.flush()
    start = time.monotonic()
    status = "waiting for the model"
    preview = ""
    width = 0
    last_update = 0.0

    def progress():
        nonlocal width, last_update
        now = time.monotonic()
        if now - last_update < 0.20 or (notify is None and not sys.stdout.isatty()):
            return
        line = f"{label}  {now - start:6.1f}s  {short(status)}"
        if preview:
            line += f" | {short(preview[-45:], 45)}"
        last_update = now
        if notify is not None:
            notify("progress", line)
            return
        columns = max(1, shutil.get_terminal_size(fallback=(120, 30)).columns - 1)
        line = line[:columns]
        width = min(columns, max(width, len(line)))
        print("\r" + line.ljust(width), end="", flush=True)

    try:
        progress()
        while True:
            event = receive(events, start + watchdog, control, progress)
            match event["event"]:
                case "status":
                    status = event["text"].split(": ", 1)[-1]
                case "delta" | "replace":
                    status = "generating"
                    if event["event"] == "replace":
                        preview = ""
                    preview = (preview + event["text"])[-80:]
                case "done":
                    return event
            progress()
    finally:
        if width:
            print("\r" + " " * width + "\r", end="", flush=True)


def fixture_case(workspace, rng):
    expected = {"release_channel": f"canary-{rng.getrandbits(64):016x}"}
    (workspace / "fixture.json").write_text(json.dumps(expected), encoding="utf-8")
    prompt = ('Call the read_file tool now with {"path":"fixture.json"}. The file is in the '
              "selected project root and changes between tasks. After the tool returns, copy "
              "the resulting JSON object as your final answer.")
    return "read fixture", prompt, expected


def installed_models(endpoint):
    base = endpoint.strip().rstrip("/")
    url = f"{base}/tags" if base.endswith("/api") else f"{base}/api/tags"
    with urlopen(url, timeout=5) as response:
        data = json.load(response)
    return sorted({entry["name"] for entry in data["models"]})


def service_url(endpoint, route):
    return endpoint.strip().rstrip("/").removesuffix("/api") + "/_redox/" + route


def has_qualification_service(endpoint):
    try:
        with urlopen(service_url(endpoint, "health"), timeout=2) as response:
            return json.load(response).get("service") == "redox-qualified-ollama"
    except (OSError, ValueError, AttributeError):
        return False


@contextmanager
def benchmark_endpoint(requested):
    endpoint = urlsplit(requested or LOCAL_OLLAMA)
    local_default = (endpoint.scheme == "http" and endpoint.hostname in ("127.0.0.1", "localhost")
                     and endpoint.port in (11434, 11435) and endpoint.path.rstrip("/") in ("", "/api"))
    if not local_default:
        yield requested
        return
    if has_qualification_service(QUALIFIED_ENDPOINT):
        yield QUALIFIED_ENDPOINT
        return

    command = [sys.executable, "-B", ROOT / "src" / "ollama_service.py",
               "--port", str(urlsplit(QUALIFIED_ENDPOINT).port), "--upstream", LOCAL_OLLAMA,
               "--exit-on-stdin"]
    child = subprocess.Popen(command, stdin=subprocess.PIPE,
                             creationflags=subprocess.CREATE_NEW_PROCESS_GROUP if os.name == "nt" else 0)
    try:
        deadline = time.monotonic() + 8
        while not has_qualification_service(QUALIFIED_ENDPOINT):
            if child.poll() is not None:
                raise BenchmarkError(f"Qualification service exited with code {child.returncode}.")
            if time.monotonic() >= deadline:
                raise BenchmarkError(f"Qualification service did not become ready at {QUALIFIED_ENDPOINT}.")
            time.sleep(0.10)
        yield QUALIFIED_ENDPOINT
    finally:
        child.stdin.close()
        try:
            child.wait(timeout=6)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()


def wait_for_qualification(options, model, control, notify):
    if model.casefold() not in QUALIFIED_MODELS:
        return
    if not has_qualification_service(options.endpoint):
        raise BenchmarkError("Aisnley-Ox requires the qualified Ollama service. Select its endpoint before starting.")
    if notify is not None:
        notify("progress", f"{model}  Waiting for 10 reasoning passes and 1 Rust build")
    finished = queue.Queue(maxsize=1)

    def qualify():
        body = json.dumps({"model": model, "options": {"num_ctx": options.context}}).encode()
        request = Request(service_url(options.endpoint, "qualify"), data=body,
                          headers={"Content-Type": "application/json"})
        try:
            with urlopen(request, timeout=None) as response:
                result = json.load(response)
            finished.put(None if result.get("qualified") else "Qualification did not pass.")
        except HTTPError as error:
            with error:
                try:
                    finished.put(json.load(error).get("error") or str(error))
                except (OSError, ValueError, AttributeError):
                    finished.put(str(error))
        except (OSError, ValueError, AttributeError) as error:
            finished.put(str(error))

    threading.Thread(target=qualify, daemon=True).start()
    while True:
        control.check_running()
        try:
            error = finished.get(timeout=0.25)
        except queue.Empty:
            continue
        control.check_running()
        if error:
            raise BenchmarkError(f"Server qualification failed: {error}")
        return


def build_target():
    return Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")).resolve()


class RunnerSession:
    def __init__(self, options, workspace, control):
        self.workspace = workspace
        self.events = queue.Queue(maxsize=512)
        self.finished = threading.Event()
        executable = "ollama-bench-runner.exe" if os.name == "nt" else "ollama-bench-runner"
        runner = build_target() / "debug" / "examples" / executable
        suite = getattr(options, "suite", "general")
        output = options.output or (4096 if suite == "rust" else 1024)
        self.process = subprocess.Popen(
            [runner, options.endpoint, options.model, str(options.context), str(output), workspace, suite],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, encoding="utf-8",
            bufsize=1, creationflags=subprocess.CREATE_NEW_PROCESS_GROUP if os.name == "nt" else 0,
        )
        self.reader = threading.Thread(target=read_events, args=(self.process.stdout, self.events, self.finished),
                                       daemon=True)
        self.reader.start()
        try:
            self.ready = receive(self.events, time.monotonic() + options.watchdog, control)
            if self.ready["event"] != "ready":
                raise BenchmarkError("ReDox runner sent an unexpected startup response.")
        except BaseException:
            self.close()
            raise

    def close(self):
        self.finished.set()
        if self.process.poll() is None:
            self.process.terminate()
        self.process.wait()
        self.process.stdin.close()
        self.reader.join()


def benchmark(options, workspace, control, notify, session=None, batch=None):
    suite = getattr(options, "suite", "general")
    output = options.output or (4096 if suite == "rust" else 1024)
    rust_build = None
    if suite == "rust":
        from ollama_bench_rust import RustBuild
        from ollama_bench_rust_cases import make_rust_case
    rng = random.Random(options.seed)
    marker = f"jam-{rng.getrandbits(64):016x}"
    score = Score()
    model_scores = {}
    started = time.monotonic()
    report_path = None
    own_session = session is None

    def say(line, error=False):
        if notify is not None:
            notify("line", line)
        else:
            print(line, file=sys.stderr if error else sys.stdout, flush=True)

    try:
        control.check_running()
        if own_session:
            session = RunnerSession(options, workspace, control)
        ready = session.ready
        report_name = "benchmark.jsonl" if batch is None else f"benchmark-batch-{batch:03d}.jsonl"
        report_path = Path(ready["folder"]) / report_name
        report_path.parent.mkdir(parents=True, exist_ok=True)
        if suite == "rust":
            rust_build = RustBuild(workspace / report_path.stem, build_target() / "ollama-bench-rust" / ready["chat"], ROOT)
            say(f"Rust Build | Cargo build deadline {options.build_timeout}s")
        say(f"Model {options.model} | requested context {options.context:,} | watchdog {options.watchdog}s/task")
        say(f"Single ReDox chat {ready['chat']}")
        say(f"Results: {report_path}")
        with report_path.open("x", encoding="utf-8", buffering=1) as report:
            task_settings = {}
            batch_fields = {}
            if batch is not None:
                task_settings = {"suite": suite, "output": output}
                if not own_session:
                    task_settings["keep_alive"] = -1
                batch_fields = {"batch": batch, "chat": ready["chat"], "keep_context": not own_session}
            round_number = 0
            previous_model = options.model
            while options.rounds is None or round_number < options.rounds:
                control.check_running()
                tool_task = rust_build is None and round_number % 7 == 1
                if rust_build is not None:
                    name, prompt, harness = make_rust_case(round_number)
                    expected = None
                elif tool_task:
                    name, prompt, expected = fixture_case(workspace, rng)
                else:
                    name, prompt, expected = make_case(round_number, rng, marker)
                round_number += 1
                label = f"#{round_number} {name}"
                model = control.selected_model()
                if model != previous_model:
                    say(f"Switched to {model} for task #{round_number}")
                    previous_model = model
                if model not in model_scores:
                    model_scores[model] = Score()
                model_score = model_scores[model]
                if notify is not None:
                    notify("task", (model, str(model_score)))
                wait_for_qualification(options, model, control, notify)
                build_fields = {}
                if rust_build is not None:
                    build_fields = {"build_status": "not_started", "build_seconds": None,
                                    "build_diagnostic": "", "scored": True}
                turn_started = time.monotonic()
                try:
                    result = run_task(session.process, session.events, prompt, label, model,
                                      options.watchdog, control, notify, **task_settings)
                    if result.get("model") != model:
                        raise BenchmarkError("ReDox returned a result for a different model.")
                except (BenchmarkError, OSError) as error:
                    failed_json_score = False if suite == "general" else None
                    elapsed = round(time.monotonic() - turn_started, 3)
                    score.record(False, elapsed=elapsed, passed_ex_json=failed_json_score)
                    model_score.record(False, elapsed=elapsed, passed_ex_json=failed_json_score)
                    if notify is not None:
                        notify("score", (str(score), str(model_score)))
                    if rust_build is not None:
                        build_fields["model_elapsed"] = elapsed
                    report.write(json.dumps({"round": round_number, "task": name, "model": model, "suite": suite,
                                             "passed": False, "passed_ex_json": failed_json_score,
                                             "elapsed": elapsed, "total_task_seconds": round(score.task_seconds, 3),
                                             "model_total_task_seconds": round(model_score.task_seconds, 3),
                                             "error": str(error), **build_fields, **batch_fields}) + "\n")
                    raise
                error = result["error"]
                usage = result["stats"]
                reply = result["text"]
                diagnostic = ""
                passed_ex_json = None
                if rust_build is not None:
                    build_fields["model_elapsed"] = round(time.monotonic() - turn_started, 3)
                    passed = False
                    if not error:
                        if notify is not None:
                            notify("progress", f"{label}  Building with Cargo")
                        else:
                            say(f"{label}  Building with Cargo")
                        build = rust_build.check(reply, harness, options.build_timeout, control.stop)
                        control.check_running()
                        passed = (build.status == "compiled") if build.scored else None
                        diagnostic = build.diagnostic
                        build_time = round(build.seconds, 3) if build.seconds is not None else None
                        build_fields.update(build_status=build.status, build_seconds=build_time,
                                            build_diagnostic=diagnostic, scored=build.scored)
                else:
                    eligible = not error and (not tool_task or result["file_reads"] > 0)
                    passed = eligible and grade(reply, expected)
                    passed_ex_json = passed or (eligible and grade(reply, expected, allow_bare_json=True))
                elapsed = round(time.monotonic() - turn_started, 3)
                score.record(passed, usage, result["rollovers"], elapsed=elapsed, passed_ex_json=passed_ex_json)
                model_score.record(passed, usage, result["rollovers"], elapsed=elapsed, passed_ex_json=passed_ex_json)
                if notify is not None:
                    notify("score", (str(score), str(model_score)))
                report.write(json.dumps({"round": round_number, "task": name, "model": model, "suite": suite,
                                         "passed": passed is True, "passed_ex_json": passed_ex_json,
                                         "elapsed": elapsed, "total_task_seconds": round(score.task_seconds, 3),
                                         "model_total_task_seconds": round(model_score.task_seconds, 3), "usage": usage,
                                         "rollovers": result["rollovers"], "file_reads": result["file_reads"],
                                         "error": error, "expected": None if passed or error else expected,
                                         "failed_reply": None if passed else reply[:320], **build_fields,
                                         **batch_fields}) + "\n")
                runtime = duration(time.monotonic() - started)
                outcome = {True: "PASS", False: "FAIL", None: "INCONCLUSIVE"}[passed]
                if passed_ex_json and not passed:
                    outcome = "FAIL (PASS ex. JSON)"
                cargo_time = build_fields.get("build_seconds")
                timing = f"{elapsed:.1f}s total, {cargo_time:.2f}s Cargo" if cargo_time is not None else f"{elapsed:.1f}s"
                say(f"{outcome} {label} ({timing}) | {model} | {score} | {runtime} run")
                if error or not passed:
                    cargo_error = next((line for line in diagnostic.splitlines() if "error" in line.lower()),
                                       diagnostic)
                    say(f"     {short(error or cargo_error or reply, 160) or 'empty response'}")
                if build_fields.get("build_status") == "tool_error":
                    raise BenchmarkError(diagnostic)
                if error:
                    control.stop.wait(2)
        return 0
    except (KeyboardInterrupt, BenchmarkStopped):
        say("Stopped by user.")
        return 130
    except (BenchmarkError, OSError) as error:
        say(f"Benchmark stopped: {error}", error=True)
        return 1
    finally:
        if own_session and session is not None:
            session.close()
        say(f"Final after {duration(time.monotonic() - started)} | {score}")
        if len(model_scores) > 1:
            for model, total in model_scores.items():
                say(f"{model} | {total}")
        if report_path is not None:
            say(f"Results: {report_path}")
        if notify is not None:
            notify("final_score", str(score))


def run_benchmark(options, control=None, notify=None):
    with tempfile.TemporaryDirectory(prefix="redox-ollama-bench-") as folder:
        return benchmark(options, Path(folder), control or RunControl(options.model), notify)


def unload_model(endpoint, model):
    base = endpoint.strip().rstrip("/")
    api = base if base.endswith("/api") else f"{base}/api"
    body = json.dumps({"model": model, "keep_alive": 0, "stream": False}).encode()
    request = Request(f"{api}/generate", data=body, headers={"Content-Type": "application/json"})
    with urlopen(request, timeout=5) as response:
        response.read()


def run_queue(options, plan, keep_context, control, notify=None):
    """Run fixed batches, optionally retaining one native chat per model."""
    def say(line):
        if notify is None:
            print(line, flush=True)
        else:
            notify("line", line)

    def release(model):
        try:
            unload_model(options.endpoint, model)
        except OSError as error:
            say(f"Could not release {model}: {error}")

    sessions = {}
    try:
        with ExitStack() as resources:
            for batch, (model, suite, rounds) in enumerate(plan, 1):
                control.check_running()
                if not model.strip() or suite not in ("general", "rust") or rounds < 1:
                    raise BenchmarkError(f"Invalid model, suite, or task count in batch {batch}.")
                job = copy(options)
                job.model, job.suite, job.rounds = model, suite, rounds
                control.select_model(model)
                suite_name = "Rust Build" if suite == "rust" else "Reasoning and tools"
                say(f"Batch {batch}/{len(plan)} | {model} | {suite_name} | {rounds} tasks")
                if notify is not None:
                    notify("batch", (batch, len(plan), model, suite_name))

                if keep_context:
                    if model not in sessions:
                        workspace = Path(resources.enter_context(tempfile.TemporaryDirectory(prefix="redox-ollama-bench-")))
                        session = RunnerSession(job, workspace, control)
                        sessions[model] = session
                        resources.callback(release, model)
                        resources.callback(session.close)
                    session = sessions[model]
                    code = benchmark(job, session.workspace, control, notify, session, batch)
                else:
                    unload_model(options.endpoint, model)
                    try:
                        with tempfile.TemporaryDirectory(prefix="redox-ollama-bench-") as folder:
                            code = benchmark(job, Path(folder), control, notify, batch=batch)
                    finally:
                        release(model)
                if code:
                    return code
        return 0
    except (KeyboardInterrupt, BenchmarkStopped):
        say("Stopped by user.")
        return 130
    except (BenchmarkError, OSError) as error:
        say(f"Queue stopped: {error}")
        return 1


def positive(value):
    number = int(value)
    if number < 1:
        raise argparse.ArgumentTypeError("enter a positive integer")
    return number


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", default="tiel-coder:35b-a3b-mtp-ud-iq4-xs")
    parser.add_argument("--endpoint", default=None)
    parser.add_argument("--context", type=positive, default=64000)
    parser.add_argument("--suite", choices=("general", "rust"), default="general", help="benchmark task suite")
    parser.add_argument("--output", type=positive, help="generation token allowance (1024 general, 4096 Rust)")
    parser.add_argument("--build-timeout", type=positive, default=180, help="maximum seconds for a Rust Cargo build")
    parser.add_argument("--watchdog", type=positive, default=900, help="maximum seconds per task")
    parser.add_argument("--rounds", type=positive, help="stop after this many tasks (default: unlimited)")
    parser.add_argument("--seed", type=int, default=1257, help="reproduce the same tasks and answer keys")
    parser.add_argument("--gui", action="store_true", help="open the benchmark window and model picker")
    options = parser.parse_args()
    options.model = options.model.strip()
    if not options.model:
        parser.error("--model must name an Ollama model")
    try:
        with benchmark_endpoint(options.endpoint) as endpoint:
            options.endpoint = endpoint
            if options.gui:
                sys.path.insert(0, str(ROOT / "src" / "ui"))
                from ollama_bench_gui import launch
                return launch(options, RunControl(options.model), run_benchmark, installed_models, run_queue)
            print("Press Ctrl+C to stop the benchmark.", flush=True)
            return run_benchmark(options)
    except KeyboardInterrupt:
        return 130
    except (BenchmarkError, OSError) as error:
        print(f"Benchmark stopped: {error}", file=sys.stderr, flush=True)
        return 1


if __name__ == "__main__":
    sys.exit(main())
