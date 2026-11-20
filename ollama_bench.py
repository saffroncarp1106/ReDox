"""Run the standalone Ollama Stresser."""

import argparse
from contextlib import ExitStack
from copy import copy
from dataclasses import dataclass
import json
import os
from pathlib import Path
import random
import shutil
import sys
import tempfile
import threading
import time
from uuid import uuid4

from ollama_bench_cases import JSON_FORMAT, grade, make_case
from ollama_client import BenchmarkError, ChatSession, LOCAL_OLLAMA, api_url, installed_models, unload_model


def default_results_directory():
    base = os.environ.get("LOCALAPPDATA") or os.environ.get("XDG_DATA_HOME")
    return (Path(base) if base else Path.home() / ".local" / "share") / "OllamaStresser" / "results"


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


def short(text, limit=70):
    return "".join(char if " " <= char <= "~" else " " for char in text[:limit])


def duration(seconds):
    hours, remainder = divmod(int(seconds), 3600)
    minutes, seconds = divmod(remainder, 60)
    return f"{hours:02d}:{minutes:02d}:{seconds:02d}"


def run_task(session, prompt, label, model, suite, output, keep_alive, watchdog, control, notify,
             fixture, remember):
    start = time.monotonic()
    status = "waiting for the model"
    preview = ""
    width = 0
    last_update = 0.0

    def progress(new_status, delta):
        nonlocal width, last_update, status, preview
        if new_status != "waiting" or not preview:
            status = new_status
        if delta:
            preview = (preview + delta)[-80:]
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
        progress("waiting for the model", "")
        return session.run(prompt, model, suite, output, keep_alive, start + watchdog, control,
                           progress, fixture=fixture, remember=remember)
    finally:
        if width:
            print("\r" + " " * width + "\r", end="", flush=True)


def fixture_case(workspace, rng, round_number, batch):
    expected = {"release_channel": f"canary-{rng.getrandbits(64):016x}"}
    fixture = workspace / f"fixture-{batch or 0}-{round_number + 1}.json"
    fixture.write_text(json.dumps(expected), encoding="utf-8")
    arguments = json.dumps({"path": fixture.name})
    prompt = (f"Call the read_file tool now with {arguments}. "
              "Use the value returned by this call as your answer.")
    return "read fixture", prompt, expected, fixture


class BenchmarkSession(ChatSession):
    def __init__(self, options, workspace):
        super().__init__(options.endpoint, options.context)
        self.workspace = workspace
        self.id = uuid4().hex
        root = getattr(options, "results_dir", None) or default_results_directory()
        self.folder = (Path(root) / self.id).resolve()
        self.folder.mkdir(parents=True)


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
            session = BenchmarkSession(options, workspace)
        report_name = "benchmark.jsonl" if batch is None else f"benchmark-batch-{batch:03d}.jsonl"
        report_path = session.folder / report_name
        if suite == "rust":
            rust_build = RustBuild(workspace / report_path.stem, session.folder / "cargo")
            say(f"Rust Build | Cargo build deadline {options.build_timeout}s")
        say(f"Model {options.model} | requested context {options.context:,} | watchdog {options.watchdog}s/task")
        say(f"Chat {session.id}")
        say(f"Results: {report_path}")
        with report_path.open("x", encoding="utf-8", buffering=1) as report:
            keep_alive = None if own_session else -1
            batch_fields = {}
            if batch is not None:
                batch_fields = {"batch": batch, "chat": session.id, "keep_context": not own_session}
            round_number = 0
            previous_model = options.model
            while options.rounds is None or round_number < options.rounds:
                control.check_running()
                tool_task = rust_build is None and round_number % 7 == 1
                fixture = None
                if rust_build is not None:
                    name, prompt, harness = make_rust_case(round_number)
                    expected = None
                elif tool_task:
                    name, prompt, expected, fixture = fixture_case(workspace, rng, round_number, batch)
                else:
                    name, prompt, expected = make_case(round_number, rng, marker)
                if rust_build is None:
                    prompt = f"{prompt}\n\n{JSON_FORMAT}"
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
                build_fields = {}
                if rust_build is not None:
                    build_fields = {"build_status": "not_started", "build_seconds": None,
                                    "build_diagnostic": "", "scored": True}
                turn_started = time.monotonic()
                try:
                    result = run_task(session, prompt, label, model, suite, output, keep_alive, options.watchdog,
                                      control, notify, fixture, name == "remember_marker")
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
    except (BenchmarkError, OSError, ValueError) as error:
        say(f"Benchmark stopped: {error}", error=True)
        return 1
    finally:
        say(f"Final after {duration(time.monotonic() - started)} | {score}")
        if len(model_scores) > 1:
            for model, total in model_scores.items():
                say(f"{model} | {total}")
        if report_path is not None:
            say(f"Results: {report_path}")
        if notify is not None:
            notify("final_score", str(score))


def run_benchmark(options, control=None, notify=None):
    with tempfile.TemporaryDirectory(prefix="ollama-stresser-") as folder:
        return benchmark(options, Path(folder), control or RunControl(options.model), notify)


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
                        workspace = Path(resources.enter_context(tempfile.TemporaryDirectory(prefix="ollama-stresser-")))
                        session = BenchmarkSession(job, workspace)
                        sessions[model] = session
                        resources.callback(release, model)
                    session = sessions[model]
                    code = benchmark(job, session.workspace, control, notify, session, batch)
                else:
                    unload_model(options.endpoint, model)
                    try:
                        with tempfile.TemporaryDirectory(prefix="ollama-stresser-") as folder:
                            code = benchmark(job, Path(folder), control, notify, batch=batch)
                    finally:
                        release(model)
                if code:
                    return code
        return 0
    except (KeyboardInterrupt, BenchmarkStopped):
        say("Stopped by user.")
        return 130
    except (BenchmarkError, OSError, ValueError) as error:
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
    parser.add_argument("--endpoint", default=LOCAL_OLLAMA)
    parser.add_argument("--results-dir", type=Path, help="directory for benchmark reports")
    parser.add_argument("--context", type=positive, default=64000)
    parser.add_argument("--suite", choices=("general", "rust"), default="general", help="benchmark task suite")
    parser.add_argument("--output", type=positive, help="generation token allowance (1024 general, 4096 Rust)")
    parser.add_argument("--build-timeout", type=positive, default=180, help="maximum seconds for a Rust Cargo build")
    parser.add_argument("--watchdog", type=positive, default=900, help="maximum seconds per task")
    parser.add_argument("--rounds", type=positive, help="stop after this many tasks (default: unlimited)")
    parser.add_argument("--seed", type=int, default=1257, help="reproduce the same tasks and answer keys")
    parser.add_argument("--gui", action="store_true", help="open the benchmark window and model picker")
    options = parser.parse_args()
    if len(sys.argv) == 1:
        options.gui = True
    options.model = options.model.strip()
    if not options.model:
        parser.error("--model must name an Ollama model")
    try:
        if options.gui:
            from ollama_bench_gui import launch
            return launch(options, RunControl(options.model), run_benchmark, installed_models, run_queue)
        api_url(options.endpoint, "chat")
        print("Press Ctrl+C to stop the benchmark.", flush=True)
        return run_benchmark(options)
    except KeyboardInterrupt:
        return 130
    except (BenchmarkError, OSError, ValueError) as error:
        print(f"Benchmark stopped: {error}", file=sys.stderr, flush=True)
        return 1


if __name__ == "__main__":
    sys.exit(main())
