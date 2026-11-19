"""Check queued suite changes through the native runner and Cargo grader."""

from http.server import ThreadingHTTPServer
import json
from pathlib import Path
import re
import threading
from types import SimpleNamespace
import unittest

from ollama_bench import RunControl, run_queue
from test_ollama_model_switch import MockOllama
from test_ollama_rust_bench import SUBMISSIONS


class QueueModel(MockOllama):
    def do_POST(self):
        request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        if self.path == "/api/show":
            self.reply({"capabilities": ["tools"]})
            return
        if self.path == "/api/generate":
            self.server.unloads.append(request)
            self.reply({"done": True, "done_reason": "unload"})
            return

        self.server.requests.append(request)
        task = request["messages"][-1]["content"]
        if "solution.rs" in task:
            content = SUBMISSIONS[0]
        else:
            marker = re.search(r"jam-[0-9a-f]+", task).group()
            content = "```json\n" + json.dumps({
                "acknowledged": True, "marker": marker, "type": "remember_marker",
            }) + "\n```"
        self.reply({"message": {"content": content}, "done": True, "eval_count": 4})


class BenchmarkQueueTest(unittest.TestCase):
    def test_order_scores_and_history_in_both_modes(self):
        plan = [("first", "general", 1), ("second", "general", 1),
                ("first", "rust", 1), ("second", "rust", 1)]
        for keep_context in (True, False):
            with self.subTest(keep_context=keep_context):
                notes = []
                with ThreadingHTTPServer(("127.0.0.1", 0), QueueModel) as server:
                    server.requests = []
                    server.unloads = []
                    serve = threading.Thread(target=server.serve_forever, daemon=True)
                    serve.start()
                    options = SimpleNamespace(
                        model="first", suite="general", rounds=None, seed=42,
                        endpoint=f"http://127.0.0.1:{server.server_port}",
                        context=64000, output=None, watchdog=15, build_timeout=60,
                    )
                    try:
                        result = run_queue(options, plan, keep_context, RunControl("first"),
                                           lambda kind, value: notes.append(value) if kind == "line" else None)
                    finally:
                        server.shutdown()
                        serve.join()

                self.assertEqual(result, 0, notes)
                self.assertEqual([request["model"] for request in server.requests], [job[0] for job in plan])
                self.assertEqual([r["options"]["num_ctx"] for r in server.requests], [64000] * 4)
                self.assertEqual([r["options"]["num_predict"] for r in server.requests], [1024, 1024, 4096, 4096])
                self.assertEqual([bool(r.get("tools")) for r in server.requests], [True, True, False, False])
                for index, request in enumerate(server.requests):
                    assistants = [m for m in request["messages"] if m["role"] == "assistant"]
                    self.assertEqual(len(assistants), int(keep_context and index > 1))
                    if keep_context:
                        self.assertEqual(request.get("keep_alive"), -1)
                if not keep_context:
                    self.assertTrue(server.unloads)
                    self.assertTrue(all(request["keep_alive"] == 0 for request in server.unloads))
                paths = list(dict.fromkeys(Path(line.removeprefix("Results: ")) for line in notes
                                           if line.startswith("Results: ")))
                self.assertEqual(len(paths), 4)
                rows = [json.loads(path.read_text(encoding="utf-8")) for path in paths]
                self.assertEqual([(row["model"], row["suite"]) for row in rows],
                                 [(model, suite) for model, suite, _ in plan])
                self.assertEqual([row["batch"] for row in rows], [1, 2, 3, 4])
                self.assertEqual(len({row["chat"] for row in rows}), 2 if keep_context else 4)
                self.assertTrue(all(row["passed"] for row in rows))
