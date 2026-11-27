"""Run after building the ollama-bench-runner Cargo example."""

from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import random
import threading
from types import SimpleNamespace
import unittest

from ollama_bench import RunControl, installed_models, run_benchmark
from ollama_bench_cases import make_case


class MockOllama(BaseHTTPRequestHandler):
    def reply(self, body):
        data = (json.dumps(body) + "\n").encode()
        self.send_response(200)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        self.reply({"models": [{"name": name} for name in ("second", "first")]})

    def do_POST(self):
        request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        model = request["model"]
        if self.path == "/api/show":
            self.reply({"capabilities": ["tools"]})
            return

        self.server.requests.append(request)
        results = [msg for msg in request["messages"] if msg["role"] == "tool"]
        if model == "first":
            # A selection comes in while the first model is answering.
            self.server.control.select_model("second")
            answer = self.server.first_answer
        elif not results:
            self.reply({"message": {"tool_calls": [{"function": {
                "name": "read_file", "arguments": {"path": "fixture.json"}
            }}]}, "done": True})
            return
        else:
            answer = json.loads(results[-1]["content"])

        fence = "```"
        content = json.dumps(answer)
        if model == "second":
            content = f"{fence}json\n{content}\n{fence}"
        self.reply({"message": {"content": content}, "done": True, "eval_count": 4})

    def log_message(self, *_):
        pass


class ModelSwitchTest(unittest.TestCase):
    def test_switch_keeps_history_and_scores_the_right_model(self):
        seed = 42
        rng = random.Random(seed)
        marker = f"jam-{rng.getrandbits(64):016x}"
        control = RunControl("first")
        events = []
        with ThreadingHTTPServer(("127.0.0.1", 0), MockOllama) as server:
            server.control = control
            server.first_answer = make_case(0, rng, marker)[2]
            server.requests = []
            threading.Thread(target=server.serve_forever, daemon=True).start()
            options = SimpleNamespace(
                model="first", endpoint=f"http://127.0.0.1:{server.server_port}",
                rounds=2, watchdog=8, context=64000, output=1024, seed=seed,
            )
            try:
                self.assertEqual(installed_models(options.endpoint + "/api/"), ["first", "second"])
                code = run_benchmark(options, control, lambda *event: events.append(event))
                self.assertEqual(code, 0, [value for kind, value in events if kind == "line"])
            finally:
                server.shutdown()

        self.assertEqual([request["model"] for request in server.requests], ["first", "second", "second"])
        history = server.requests[1]["messages"]
        self.assertTrue(any(msg["role"] == "assistant" and marker in msg["content"] for msg in history))
        self.assertEqual([value[0] for kind, value in events if kind == "task"], ["first", "second"])
        reports = [value.removeprefix("Results: ") for kind, value in events
                   if kind == "line" and value.startswith("Results: ")]
        self.assertEqual(len(set(reports)), 1)
        rows = [json.loads(line) for line in Path(reports[0]).read_text(encoding="utf-8").splitlines()]
        self.assertEqual(
            [(r["model"], r["passed"], r["passed_ex_json"]) for r in rows],
            [("first", False, True), ("second", True, True)],
        )
        self.assertAlmostEqual(rows[-1]["total_task_seconds"], sum(r["elapsed"] for r in rows))
        self.assertEqual([row["model_total_task_seconds"] for row in rows], [row["elapsed"] for row in rows])


if __name__ == "__main__":
    unittest.main()
