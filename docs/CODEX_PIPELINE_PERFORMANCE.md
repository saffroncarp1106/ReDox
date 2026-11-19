# Codex pipeline measurements

Measured on 2026-11-11, Windows x64, AMD Ryzen 9 5900HS (8 cores / 16 logical
processors), Rust 1.88.0. Baseline source: `0b299fd`; measurements compare its
algorithms with the working-tree revisions. The installed app-server reports
`codex-cli 0.167.0-alpha.3.2`.

## What changed

Changes in `src/codex.rs`:

- RPC results are moved out of the received JSON value instead of deeply cloned.
  A pointer-identity regression test verifies the large string allocation is
  transferred intact. Error and missing-result behavior is preserved.
- Streaming keeps a set of seen item IDs and an aggregate byte count, replacing
  the map of duplicate answer strings. It eliminates up to 8 MiB of duplicate
  answer text per response, plus spare string capacity. The UI still receives
  each delta. Final-only messages, completion deduplication, image delivery, and
  the existing 8 MiB text limit retain their behavior.
- A completed protocol record releases its scratch buffer when capacity exceeds
  64 KiB. Smaller records reuse their allocation; partial records survive a
  cancelled read. The maximum record remains 16 MiB. This trades buffer reuse on
  consecutive large records for lower retained memory. Returning memory to the
  allocator does not guarantee an immediate reduction in process RSS.
- Initialization opts out of command/file output deltas and reasoning deltas
  that ReDox does not display. Answer streaming, item/turn lifecycle, diffs,
  errors, token-usage events, and incoming requests remain enabled. Any future UI
  for those omitted streams must change the filter. The app-server supports
  filtering notification methods per connection; this does not suppress RPC
  requests or responses. See the official
  [app-server protocol](https://learn.chatgpt.com/docs/app-server).

The installed app-server accepted the revised initialization in a read-only
handshake test. No model turn or desktop interaction was started.

## Repeated release benchmarks

Each row uses three warmup pairs, followed by 30 paired batches. Execution order
is balanced and randomized. Timings use `Instant` and `black_box`; the benchmark
runs serially in an optimized build with ThinLTO. Each speedup is the geometric
mean of the paired original/revised ratios. Intervals are percentile bootstrap
95% intervals from 5,000 paired resamples of log ratios. They are not ratios of
the two independently reported medians.

| Isolated operation / fixture | Original median | Revised median | Speedup | 95% interval |
| --- | ---: | ---: | ---: | ---: |
| RPC result extraction and teardown, 4 MiB result | 1,154.625 us | 154.981 us | 7.53x | 6.72-8.43x |
| Stream bookkeeping, 4 MiB answer in 1 KiB deltas | 2,310.978 us | 141.603 us | 16.13x | 15.81-16.48x |
| Ollama context-size estimate, 64 KiB text | 96.598 us | 40.303 us | 2.52x | 2.39-2.66x |
| Ollama context-size estimate, 4 MiB image payload | 922.434 us | 0.127 us | 6,432.88x | 5,784.94-7,161.14x |

The RPC fixture prepares eight owned inputs outside each timed batch, then times
result extraction and destruction. The stream fixture performs 16 answers per
batch; it excludes JSON parsing, channel delivery, rendering, and network time.
Estimator batches perform 64 estimates and preserve exactly the original
byte-counting behavior. The large image ratio comes from avoiding a copy of an
excluded 4 MiB payload; it says nothing about image inference speed. That very
short revised operation is especially sensitive to timer and scheduling noise.

These are statistical evidence for these routines on this machine, not proof of
a whole-client speedup. Samples share a process and machine; the intervals do
not capture between-process, machine, workload, or model-service variation.
Fixtures were selected to exercise the affected allocations, not as a
representative workload mix. No before/after GUI latency, whole-process CPU/RAM,
network volume, or account-usage measurement is claimed. Notification filtering
and buffer release were verified functionally, not given timing percentages.

Reproduce with:

```powershell
cargo test --locked --release -j 1 --lib _benchmark -- --ignored --nocapture --test-threads=1
```

Implementations are in `tests/transport/codex_pipeline.rs`,
`tests/transport/ollama_state.rs`, and `tests/perf/measure.rs`. Raw samples and
statistics from this run are preserved under `temp/codex-pipeline/`:

- `release-codex_rpc_4m.json`
- `release-codex_stream_4m.json`
- `release-ollama_text_64k.json`
- `release-ollama_image_4m.json`

That directory is ignored by Git. Running the benchmark again replaces those
four result files. Archive a run before comparing independent processes.

The earlier single development-profile estimator timing in
[client optimization verification](CLIENT_OPTIMIZATION.md) was not statistical
evidence and is not directly comparable with this release benchmark.

## Local resources versus model usage

These changes avoid local copies, retained text, and unused pipe events. They do
not alter model selection, reasoning effort, instructions, tool results, or
context policy, so no model-token or account-quota reduction is claimed.
Suppressing delivery of a reasoning notification does not eliminate the model's
reasoning work.

The existing `ChatRequest::from_chat` already sends just the newest completed
user message and its images when a server thread exists. The transport resumes
with `excludeTurns: true`, avoiding a full-history response. The server still
uses the conversation context: sending fewer bytes is not equivalent to billing
fewer input tokens. New/imported branches without a server thread need their
prior context. Removing it would change task behavior.

Further account-usage work needs representative completed tasks with recorded
input/cached/output tokens, quality, retries, and elapsed time. Persistent
app-server reuse also needs measurement: it can avoid process startups but
retains idle memory and requires shared lifecycle and cancellation handling.

## Python and Rust

`git ls-files '*.py' '*.pyi' '*.pyx'` found zero tracked Python files. This checkout
has 112 tracked Rust files, including tests and the bundled Studio runtime; 78
are under `src/`. Its client and Codex transport are already Rust and launch the
native `codex.exe app-server --stdio` process directly. There is no Python
transport wrapper here to rewrite.

The official [open-source component map](https://learn.chatgpt.com/docs/open-source)
points to the native Codex CLI and Rust app-server source. Codex also has a
JavaScript/V8 code-mode host, as described in the
[release notes](https://learn.chatgpt.com/docs/changelog). Those facts do not imply
every Codex component uses one language. The hosted service implementation is
not available in this checkout, so its Python percentage is unknown.

A Rust rewrite of this local pipeline would duplicate its current language.
Profiling allocations, parsing, polling, process startup, and task retries is
more useful. A separate Python tool could merit a native implementation if
profiling identifies substantial CPU-bound work there; it would not accelerate
remote inference or inherently reduce model tokens.

## Verification

The regular suite passed 151 tests, with eight manual/optional tests ignored and
one known Windows resource-limiter test filtered. The two release benchmarks
and installed-app-server handshake passed separately. The debug build, scoped
Rust formatting, and `git diff --check` passed. All-target Clippy completed with
existing warnings only. Library tests passed again after consolidating the
shared benchmark helper into one test-only module.

```powershell
cargo test --locked -j 1 -- --skip resources::windows::tests::descendants_inherit_membership
cargo test --locked -j 1 --lib installed_codex_accepts_filtered_notifications -- --ignored --nocapture
cargo build --locked -j 1
```

The handshake uses `REDOX_PIPELINE_CODEX` when supplied, otherwise normal
executable discovery. The excluded, unchanged job-object test previously failed
with Windows error 50 (unsupported child-job resource configuration). Existing
repository-wide formatting and strict-Clippy failures are documented in
[client optimization verification](CLIENT_OPTIMIZATION.md).
