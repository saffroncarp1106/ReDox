"""Exercise the Rust suite using mock model replies and real Cargo builds."""

from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import threading
from types import SimpleNamespace
import unittest

from ollama_bench import run_benchmark


SUBMISSIONS = (
    """
pub struct Job { pub id: String, pub priority: i32, pub prerequisites: Vec<String> }
pub enum ScheduleError {
    DuplicateId(String), MissingDependency { job: String, dependency: String }, Cycle(Vec<String>)
}
pub fn schedule(_: &[Job]) -> Result<Vec<String>, ScheduleError> { panic!("compile only") }
""",
    """
pub enum Action {
    Open { account: String }, Deposit { account: String, cents: u64 },
    Withdraw { account: String, cents: u64 }, Transfer { from: String, to: String, cents: u64 },
}
pub enum LedgerError {
    DuplicateAccount { index: usize, account: String }, UnknownAccount { index: usize, account: String },
    InsufficientFunds { index: usize, account: String }, Overflow { index: usize, account: String },
    ZeroAmount { index: usize }, SameAccount { index: usize },
}
pub fn apply_ledger(_: &[Action]) -> Result<std::collections::BTreeMap<String, u64>, LedgerError> {
    panic!("compile only")
}
""",
    """
pub enum Expr {
    Identifier(String), Not(Box<Expr>), And(Box<Expr>, Box<Expr>), Or(Box<Expr>, Box<Expr>),
}
pub struct ParseError { pub offset: usize, pub message: String }
pub fn parse_filter(_: &str) -> Result<Expr, ParseError> { panic!("compile only") }
""",
    "pub fn schedule() {}",
)


class MockRustModel(BaseHTTPRequestHandler):
    def do_POST(self):
        request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        if self.path == "/api/show":
            response = {"capabilities": ["tools"]}
        else:
            number = len(self.server.requests)
            self.server.requests.append(request)
            source = SUBMISSIONS[number]
            content = source if number != 1 else f"```rust\n{source}\n```"
            response = {"message": {"content": content}, "done": True, "eval_count": 20}
        data = (json.dumps(response) + "\n").encode()
        self.send_response(200)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *_):
        pass


class RustBuildSuiteTest(unittest.TestCase):
    def test_real_cargo_scores_all_three_interfaces_and_rejects_a_bad_submission(self):
        notes = []
        with ThreadingHTTPServer(("127.0.0.1", 0), MockRustModel) as server:
            server.requests = []
            threading.Thread(target=server.serve_forever, daemon=True).start()
            options = SimpleNamespace(
                model="mock-rust", endpoint=f"http://127.0.0.1:{server.server_port}",
                rounds=4, watchdog=15, context=64000, output=None, seed=42,
                suite="rust", build_timeout=60,
            )
            try:
                result = run_benchmark(options, notify=lambda kind, value: notes.append(value) if kind == "line" else None)
                self.assertEqual(result, 0, notes)
            finally:
                server.shutdown()

        self.assertEqual(len(server.requests), 4)
        for request in server.requests:
            self.assertFalse(request.get("tools"))
            self.assertEqual(request["options"]["num_predict"], 4096)
            self.assertIn("Return the complete Rust source", request["messages"][0]["content"])
        report = Path(next(line.removeprefix("Results: ") for line in notes if line.startswith("Results: ")))
        rows = [json.loads(line) for line in report.read_text(encoding="utf-8").splitlines()]
        self.assertEqual([row["task"] for row in rows], [
            "Dependency scheduler", "Transaction ledger", "Filter expression parser", "Dependency scheduler",
        ])
        self.assertEqual([row["build_status"] for row in rows], ["compiled"] * 3 + ["compile_error"])
        self.assertEqual([row["passed"] for row in rows], [True, True, True, False])
        self.assertTrue(all(row["build_seconds"] > 0 and row["scored"] for row in rows))
        logged_seconds = 0
        for row in rows:
            logged_seconds += row["elapsed"]
            self.assertAlmostEqual(row["total_task_seconds"], logged_seconds)
            self.assertAlmostEqual(row["model_total_task_seconds"], logged_seconds)
            self.assertGreaterEqual(row["elapsed"], row["build_seconds"])
        self.assertIn("error", rows[-1]["build_diagnostic"])


if __name__ == "__main__":
    unittest.main()
