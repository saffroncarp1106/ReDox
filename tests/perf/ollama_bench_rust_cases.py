"""Rust challenges and benchmark-owned interface drivers."""

from pathlib import Path
from textwrap import dedent


PARSER = Path(__file__).resolve().parents[2] / "src" / "ollama_qualification"


def case(name, specification, harness):
    prompt = (
        "Write the complete Rust 2021 source for the module solution.rs. Use the standard "
        "library and implement the behavior below. Preserve the public interface exactly. "
        "Return plain Rust or a single Rust code fence. The benchmark supplies main.rs.\n\n"
        + dedent(specification).strip()
    )
    return name, prompt, dedent(harness).lstrip()


CASES = (
    case(
        "Dependency scheduler",
        """
        Implement a deterministic scheduler for jobs with prerequisites.

        pub struct Job {
            pub id: String,
            pub priority: i32,
            pub prerequisites: Vec<String>,
        }
        pub enum ScheduleError {
            DuplicateId(String),
            MissingDependency { job: String, dependency: String },
            Cycle(Vec<String>),
        }
        pub fn schedule(jobs: &[Job]) -> Result<Vec<String>, ScheduleError>

        First report the first duplicate ID in input order. Then report the first
        missing prerequisite by job order and prerequisite order. Treat repeated
        prerequisites for the same job as one edge. Schedule a job only after all
        its prerequisites. When several jobs are ready, choose the highest priority,
        then the lexicographically smallest ID. Use a topological sort that tracks
        ready jobs efficiently. If scheduling gets stuck, report every unscheduled
        ID in lexicographic order. Empty input produces an empty schedule. A job
        that requires itself is blocked by a cycle.
        """,
        """
        mod solution;
        use solution::{schedule, Job, ScheduleError};

        fn main() {
            let jobs = [
                Job { id: String::from("build"), priority: 10_i32, prerequisites: vec![String::from("fetch")] },
                Job { id: String::from("fetch"), priority: 0_i32, prerequisites: Vec::new() },
            ];
            let plan: fn(&[Job]) -> Result<Vec<String>, ScheduleError> = schedule;
            match plan(&jobs) {
                Ok(order) => println!("{}", order.join(",")),
                Err(ScheduleError::DuplicateId(id)) => { let _: String = id; }
                Err(ScheduleError::MissingDependency { job, dependency }) => {
                    let _: (String, String) = (job, dependency);
                }
                Err(ScheduleError::Cycle(blocked)) => { let _: Vec<String> = blocked; }
            }
        }
        """,
    ),
    case(
        "Transaction ledger",
        """
        Implement a transaction ledger that validates and applies actions in order.

        pub enum Action {
            Open { account: String },
            Deposit { account: String, cents: u64 },
            Withdraw { account: String, cents: u64 },
            Transfer { from: String, to: String, cents: u64 },
        }
        pub enum LedgerError {
            DuplicateAccount { index: usize, account: String },
            UnknownAccount { index: usize, account: String },
            InsufficientFunds { index: usize, account: String },
            Overflow { index: usize, account: String },
            ZeroAmount { index: usize },
            SameAccount { index: usize },
        }
        pub fn apply_ledger(actions: &[Action]) -> Result<std::collections::BTreeMap<String, u64>, LedgerError>

        An open action creates a zero balance and rejects a duplicate name. A deposit,
        withdrawal, or transfer rejects zero cents first. A transfer then rejects
        identical account names. Check account existence next, checking from before to
        for transfers. Reject overdrafts before checking whether the destination
        balance addition overflows. Use checked arithmetic and validate both sides of
        a transfer before updating either one. Stop at the first error and use its
        zero-based action index. On success return the final balances, including zeros.
        Treat account names as case sensitive.
        """,
        """
        mod solution;
        use solution::{apply_ledger, Action, LedgerError};
        use std::collections::BTreeMap;

        fn main() {
            let entries = [
                Action::Open { account: String::from("a") },
                Action::Open { account: String::from("b") },
                Action::Deposit { account: String::from("a"), cents: 25_u64 },
                Action::Transfer { from: String::from("a"), to: String::from("b"), cents: 12_u64 },
                Action::Withdraw { account: String::from("b"), cents: 5_u64 },
            ];
            let apply: fn(&[Action]) -> Result<BTreeMap<String, u64>, LedgerError> = apply_ledger;
            match apply(&entries) {
                Ok(balances) => println!("{balances:?}"),
                Err(LedgerError::DuplicateAccount { index, account })
                | Err(LedgerError::UnknownAccount { index, account })
                | Err(LedgerError::InsufficientFunds { index, account })
                | Err(LedgerError::Overflow { index, account }) => {
                    let _: (usize, String) = (index, account);
                }
                Err(LedgerError::ZeroAmount { index }) | Err(LedgerError::SameAccount { index }) => {
                    let _: usize = index;
                }
            }
        }
        """,
    ),
    (
        "Filter expression parser",
        (PARSER / "parser_prompt.txt").read_text(encoding="utf-8").rstrip(),
        (PARSER / "parser_harness.rs").read_text(encoding="utf-8"),
    ),
)


def make_rust_case(round_number):
    return CASES[round_number % len(CASES)]
