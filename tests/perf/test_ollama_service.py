"""Exercise the qualified endpoint without loading weights."""

from contextlib import contextmanager
import http.client
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import socket
import sys
import threading
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from ollama_bench import benchmark_endpoint, has_qualification_service, run_benchmark

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "src"))
from ollama_http import make_handler
from ollama_service import MODEL, QualifiedRequests


@contextmanager
def serving(handler):
    with ThreadingHTTPServer(("127.0.0.1", 0), handler) as server:
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            yield server
        finally:
            server.shutdown()
            thread.join()


class QualifiedServiceTest(unittest.TestCase):
    def test_benchmark_starts_reuses_and_reaps_its_endpoint(self):
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            endpoint = f"http://127.0.0.1:{listener.getsockname()[1]}"
        with patch("ollama_bench.QUALIFIED_ENDPOINT", endpoint):
            self.assertFalse(has_qualification_service(endpoint))
            with benchmark_endpoint(None) as owned:
                self.assertEqual(owned, endpoint)
                self.assertTrue(has_qualification_service(endpoint))
                with benchmark_endpoint(None) as reused:
                    self.assertEqual(reused, endpoint)
                self.assertTrue(has_qualification_service(endpoint))
            self.assertFalse(has_qualification_service(endpoint))

    def test_benchmark_excludes_wait_for_qualification_from_task_time(self):
        calls = []

        def qualify(context, cpu_only):
            calls.append((context, cpu_only))
            if len(calls) == 1:
                time.sleep(1.2)
            return [{"role": "assistant", "content": "Qualified"}]

        class Upstream(BaseHTTPRequestHandler):
            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                if self.path == "/api/show":
                    result = {"capabilities": []}
                else:
                    prompt = body["messages"][-1]["content"]
                    answer = prompt[prompt.index("{"):]
                    result = {"message": {"content": f"```json\n{answer}\n```"}, "done": True}
                data = (json.dumps(result) + "\n").encode()
                self.send_response(200)
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

        notes = []
        with serving(Upstream) as upstream:
            handler = make_handler(f"http://127.0.0.1:{upstream.server_port}", QualifiedRequests(qualify).inference)
            with serving(handler) as relay:
                options = SimpleNamespace(
                    model=MODEL, suite="general", rounds=1, seed=42, context=64000, output=1024,
                    watchdog=1, endpoint=f"http://127.0.0.1:{relay.server_port}",
                )
                self.assertEqual(run_benchmark(options, notify=lambda kind, value: notes.append((kind, value))), 0, notes)
        path = next(value.removeprefix("Results: ") for kind, value in notes
                    if kind == "line" and value.startswith("Results: "))
        row = json.loads(Path(path).read_text(encoding="utf-8"))
        self.assertTrue(row["passed"])
        self.assertLess(row["elapsed"], 1)
        self.assertEqual(calls, [(64000, False), (64000, False)])

    def test_waits_before_forwarding_and_streams_the_original_response(self):
        entered = threading.Event()
        passed = threading.Event()
        finish_stream = threading.Event()
        requests = []
        history = [{"role": "assistant", "content": "Passing test transcript"}]

        def qualify(context, cpu_only):
            self.assertEqual((context, cpu_only), (64000, False))
            entered.set()
            self.assertTrue(passed.wait(5))
            return history

        class Upstream(BaseHTTPRequestHandler):
            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                requests.append((self.path, body))
                self.send_response(200)
                self.send_header("Content-Type", "application/x-ndjson")
                self.send_header("Transfer-Encoding", "chunked")
                self.end_headers()
                first = b'{"message":"first"}\n'
                self.wfile.write(f"{len(first):x}\r\n".encode() + first + b"\r\n")
                self.wfile.flush()
                finish_stream.wait(5)
                self.wfile.write(b'0\r\n\r\n')

        with serving(Upstream) as upstream:
            handler = make_handler(f"http://127.0.0.1:{upstream.server_port}", QualifiedRequests(qualify).inference)
            with serving(handler) as relay:
                with connection(relay) as client:
                    client.request("POST", "/api/chat", json.dumps({
                        "model": MODEL, "messages": [{"role": "user", "content": "Actual user prompt"}],
                    }))
                    self.assertTrue(entered.wait(5))
                    self.assertEqual(requests, [])
                    passed.set()
                    with client.getresponse() as response:
                        try:
                            self.assertEqual(response.status, 200)
                            self.assertEqual(response.readline(), b'{"message":"first"}\n')
                        finally:
                            finish_stream.set()
                        self.assertEqual(response.read(), b"")

        self.assertEqual(requests[0][0], "/api/chat")
        sent = requests[0][1]
        self.assertEqual(sent["messages"], [*history, {"role": "user", "content": "Actual user prompt"}])
        self.assertEqual(sent["options"]["num_ctx"], 64000)
        self.assertEqual(sent["keep_alive"], 900)

    def test_openai_and_preflight_use_the_gate_and_errors_fail_closed(self):
        warm = []

        def qualify(context, cpu_only):
            warm.append((context, cpu_only))
            if len(warm) == 3:
                raise RuntimeError("Cargo unavailable")
            return [{"role": "assistant", "content": "Qualified"}]

        seen = []

        class Upstream(BaseHTTPRequestHandler):
            def do_POST(self):
                seen.append((self.path, json.loads(self.rfile.read(int(self.headers["Content-Length"])))))
                self.send_response(200)
                self.send_header("Content-Length", "2")
                self.end_headers()
                self.wfile.write(b"{}")

        with serving(Upstream) as upstream:
            handler = make_handler(f"http://127.0.0.1:{upstream.server_port}", QualifiedRequests(qualify).inference)
            with serving(handler) as relay:
                for path, payload, expected_status in [
                    ("/_redox/qualify", {"model": MODEL}, 200),
                    ("/v1/chat/completions", {"model": MODEL, "messages": [{"role": "user", "content": "hello"}]}, 200),
                    ("/v1/responses", {"model": MODEL, "input": "hello"}, 502),
                    ("/api/generate", {"model": MODEL, "keep_alive": 0}, 200),
                    ("/api/chat", {"model": "qwen3.6:35b", "messages": []}, 200),
                ]:
                    with connection(relay) as client:
                        client.request("POST", path, json.dumps(payload))
                        with client.getresponse() as result:
                            self.assertEqual(result.status, expected_status)
                            result.read()
        self.assertEqual(len(warm), 3)
        self.assertEqual([path for path, _ in seen], ["/v1/chat/completions", "/api/generate", "/api/chat"])
        self.assertEqual(seen[0][1]["messages"][0]["content"], "Qualified")


@contextmanager
def connection(server):
    client = http.client.HTTPConnection("127.0.0.1", server.server_port, timeout=5)
    try:
        yield client
    finally:
        client.close()


if __name__ == "__main__":
    unittest.main()
