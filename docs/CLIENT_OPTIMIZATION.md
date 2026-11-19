# Client optimization verification

Reviewed on 2026-11-11 on Windows with the pinned Rust 1.88.0 toolchain.

## Changes

- `src/ollama_state.rs`: context-size estimates count serialized JSON bytes
  through a non-allocating writer. Top-level image payloads are excluded by
  borrowing the message rather than cloning it. The existing 4096-byte allowance
  per image, JSON escaping, nested fields, and conservative context budgets are
  preserved.
- `src/ollama_tools.rs`: command output retains its 64 KiB prefix per stream and
  drains excess bytes to a sink. Truncation is explicit. A verbose child can finish
  writing without a closed pipe or an unbounded output buffer.
- Windows shell command text is passed using cmd's quoting rules. Previously,
  a PowerShell command containing quotes could print the script text and return
  success without executing the script. Tool descriptions now identify the
  platform shell. Existing approval checks and the 30-second timeout remain.
- `src/ui/markdown.rs`: ordinary Markdown borrows the source text when no math
  delimiter conversion is needed, avoiding a full message allocation and copy on
  each render. The conversion path and rendering behavior are unchanged.
- `src/run.rs`: the 8 ms Escape polling branch only runs when desktop control is
  enabled and supported. Ordinary assistant runs no longer wake this branch
  roughly 125 times per second. Desktop-enabled runs retain the existing Escape
  cancellation behavior. This is a source-level reduction in scheduled polling,
  not a measured percentage reduction in whole-app CPU use.

## Delegation review

The selected Ollama worker was given the optimization task in the existing access
mode. Its broad investigation repeatedly reread files without concluding. A later
`run.rs` patch replaced owned streamed text with an invalid `'static` reference and
introduced a tuple pattern for a non-tuple value. Review rejected those changes.
The exact file was preserved under
`temp/optimization-review/ollama-run-rejected-20261111-095440.rs` before restoring
the working ownership and event-handling code. The reviewer implemented the
focused polling guard and the other fixes documented above. No successful worker
completion or valid worker-supplied optimization batch is claimed.

## Measurements

The later [Codex pipeline measurements](CODEX_PIPELINE_PERFORMANCE.md) contain
30-pair release benchmarks, confidence intervals, raw sample locations, and
their limitations. Reproduce the current isolated estimator comparison with:

```powershell
cargo test --locked --release -j 1 --lib prompt_estimation_benchmark -- --ignored --nocapture --test-threads=1
```

The initial benchmark version produced one development-profile observation,
200 estimates per fixture:

| Fixture | Original | Revised |
| --- | ---: | ---: |
| 64 KiB text | 389.38 ms | 409.66 ms |
| 4 MiB image payload | 166.10 ms | 0.74 ms |

The image case avoids copying 4 MiB per estimate. That initial text-only timing
did not show a speed improvement; the change avoids temporary clones and
serialized strings. These measurements include debug-build overhead and are not
release-build, whole-application CPU, RAM, or frame-rate measurements.

## Regression coverage

- Estimator equality against the original algorithm for escaped text, Unicode,
  tool calls, nested data, empty/malformed image fields, and non-object values.
- Image payload length does not change the existing per-image context allowance.
- Bounded streams drain fully without breaking the writer; exact-limit, empty,
  small, and invalid-UTF-8 output preserves the existing behavior.
- A real command writes 192 KiB to each pipe and creates a completion marker.
  Both streams are truncated, and the command succeeds.
- Windows quoted script paths, arguments containing spaces and `&`, quoted file
  reads, and nonzero exit statuses are preserved.
- Existing Markdown/math rendering and Ollama retry, rollover, cancellation,
  access-mode, and approval-flow tests pass.

## Environment limitations

Initial verification of the changes above: 146 tests passed, six optional/manual tests
ignored, and the known Windows job test filtered as described below. The regular
debug build and `cargo check --locked --all-targets --features design-export -j 1`
both passed. Formatting checks on changed Rust files and `git diff --check`
passed. Non-strict all-target Clippy completed with existing warnings only.

The initial unrestricted-parallel test build failed with compiler/linker
out-of-memory errors. `-j 1` builds successfully without changing project or app
resource limits.

The unchanged test `resources::windows::tests::descendants_inherit_membership`
fails with Windows error 50 ("The request is not supported") when configuring a
child job in this environment. The remaining tests can be run with:

```powershell
cargo test --locked -j 1 -- --skip resources::windows::tests::descendants_inherit_membership
```

Repository-wide formatting and strict Clippy checks already fail on unrelated
code. Existing Clippy findings include `Presence::new` without `Default`, nested
UI conditions, a redundant attachment-name format, format-argument style, Wii
feedback branching, and a login-state boolean. These are not silently suppressed
in source. Native desktop interaction and end-to-end GUI performance were not
tested in this pass.

Remaining work includes profiling long transcripts, bounding checkpoint rewrite
cost, supporting long-running tool command sessions, and process-tree
cancellation. The command timeout and descendant ownership are not changed by
the pipe-draining fix.
