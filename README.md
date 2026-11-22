# ReDoxCodex
A front-end rewrite for rust because 1.2g of ram for a basic ui window is ridiculous.

Redox: A reaction where both reduction and oxidation occurs. The process of Oxidation, is the lose of an electron, the project was aimed to rewrite Codex to no longer use Electron and instead Rust, ironically when Iron oxidizes, it Rusts. Technically a redox reaction needs a reduction, which we have by reducing ram consumption by 10x, so use the free memory for an Ollama model!

## Run
Version **0.2.2**. The Studio runtime library is bundled under `src/ui/studio_runtime` so ordinary builds work from this repository alone.

Windows resource budgets are available in **Settings > Resource limiter**. Enable the limiter, choose CPU and RAM percentages, enter your connection's upload capacity in Mbps, choose an upload percentage, and click **Apply resource budgets**. Applied settings persist across restarts. The limiter starts disabled with proposed budgets of 50% and a user-editable 20 Mbps upload reference.

Windows Job Objects enforce a shared CPU hard cap, committed-memory ceiling derived from installed physical RAM, and outgoing-network rate limit for ReDox and its subsequently launched descendants, including its Codex app-server connection and assistant shell commands. Limits apply throughout the app session while enabled. Reaching the memory ceiling can cause task or app allocation failures. CPU percentages follow Windows job scheduling intervals and can show brief sampling spikes. The page reports each enforcement operation separately, including OS failures and potentially retained previous limits. Clearing **Enable resource budgets** and applying removes these caps.

Coverage currently includes outgoing Wi-Fi and Ethernet traffic. GPU percentage, disk bandwidth, and incoming-network throttling remain unavailable. Microsoft's [disk-rate API documentation](https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/ns-jobapi2-jobobject_io_rate_control_information) marks it unsupported on Windows 10 1607 and newer. The [network job API](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_net_rate_control_information) covers outgoing traffic. External Ollama servers, pre-existing independent tunnels, other applications, and cloud-side computation operate outside ReDox's process tree. Upload percentages use the capacity entered in settings. Enforcement requires Windows, and incompatible parent job restrictions can prevent attachment or individual limits.

On Windows, open `Launch ReDox.cmd` to rebuild changed project files and then start the updated release executable. Cargo skips unchanged work. Use `Launch ReDox.cmd --debug` for a debug build. A failed build stops the launcher instead of starting an older executable. This uses local source files and does not download source updates. Rust and the Visual Studio C++ build tools are required.

On macOS, with Rust and the native build tools installed, run `./"Mac Build.sh"` to build and launch ReDox in debug mode. Use `./"Mac Build.sh" --release` for an optimized build.

On Linux, with Rust and the native platform build dependencies installed, run `cargo run --locked --bin redox`.

For faster edit/build cycles, use `Launch ReDox.cmd --debug` or `cargo build --locked --bin redox`. Development builds retain backtrace line information with reduced debug symbols. For native debugging with local variables, set `CARGO_PROFILE_DEV_DEBUG=2` before building; changing this setting rebuilds affected artifacts. See [build speed measurements](docs/BUILD_SPEED.md) for the reproducible benchmark and tradeoffs.

The Codex transport moves RPC results without deep copies, tracks streamed answer IDs without retaining duplicate text, releases large parsing buffers, and opts out of notifications the UI does not consume. These reduce local processing and allocations; they do not establish model-token or account-quota savings. See [pipeline measurements](docs/CODEX_PIPELINE_PERFORMANCE.md) for paired release benchmarks, confidence intervals, and limitations.

On Windows, ReDox shows a desktop notification with its icon and “ReDox has a response.” when a Codex or Ollama reply completes successfully. Windows notification settings, including Do not disturb, control whether it appears. Cancelled and failed replies do not trigger it.

After you discover **Windows 98**, choose it from the **UI** menu for the teal desktop, classic windows, and Start taskbar. **Chat** opens the regular conversation view. **Terminal** opens the same conversation in black and white over Chat. Type at the `>` prompt and press **Enter** to send or **Shift+Enter** for a new line. Terminal shows recorded Codex file changes in dashed add and remove sections with blank lines around the code and a line count for each block. Replies stream below those changes under `----- redox -----`. Close Terminal or select **Chat** to switch views with the current draft intact. **My Chats**, **Open Project**, and **Ollama Agents** open their corresponding ReDox tools.

Terminal uses VileR’s **PxPlus IBM BIOS**, an 8×8 pixel font from the Ultimate Oldschool PC Font Pack. At 100% Terminal zoom the font uses its native size. Use **−** and **+** in the Terminal title bar to scale the prompt and transcript text from 75% to 200%. ReDox remembers Terminal zoom separately from the app zoom. Terminal fills the available Chat area. Use its **Fullscreen** button or **F11** to fill the screen with Terminal, and use the restore button or **F11** to return. [Font license](src/ui/assets/fonts/ibm-bios/LICENSE.TXT)

Type `/` at the Terminal prompt to see matching commands. Press **Tab** to fill in the first suggestion or click one, then press **Enter** to run it. Suggestions also cover loaded model IDs, supported effort levels, settings pages, and help topics. Use `/help` for the full list or `/help model effort` for a command's syntax. Use `/stop` to stop the response in the current chat. Use `/settings` to change the Terminal caret width or enter a title bar color such as `#800080`. Leave the color blank to hide the bar, or enter `default` to use the theme's title bar color. Commands such as `/settings models` open an app settings page.

New project chats start with **Full access**, which allows filesystem and command access without approval prompts.
Use `/access` to see the current chat's mode, or `/access full`, `/access workspace`, or `/access read-only` to change
it. Chats without a project start in read-only mode.

The Windows 98 top bar uses a compact blue gradient caption, pixel-drawn window controls, and a gray menu row. **Windows 98 Dark** appears beside it in the **UI** menu after unlocking Windows 98. It uses black backgrounds and title bars, neutral gray controls, white text, and high-contrast borders while keeping the same layout and Terminal.

Terminal's **CRT filter** is enabled by default and can be adjusted in `/settings`. The live preview shows changes to scanline strength and spacing, RGB scanline brightness and bloom, RGB halo strength and separation, and screen curvature. Brightness adds a colored scanline core, and bloom adds a soft glow around it. Each can be adjusted independently down to 0. Curvature gently bows the transcript and scanlines while keeping the screen nearly rectangular.

Windows 98 uses classic scrollbars with arrow buttons, beveled gray thumbs, and a dithered track. Codex history and Projects have gray headers above separate white, bordered scroll areas with matching left and right edges. Archived projects sits below the Projects list; expanded archive rows are compact and the list scrolls within a 120-point height limit. Arrow buttons support holding to repeat; mouse wheel, thumb dragging, and track clicks remain available.

Startup restores the saved window size. Saved sizes below the 900 × 600 minimum recover to 1100 × 900 before the window opens.

Chat rows have a small **×** button to archive locally. Open **Archived chats** and use **↶** to restore them. These choices persist across restarts and history refreshes without changing the server's archive state. Chats generating a response must finish or stop before archiving.

**New chat** reuses an empty conversation for the same project and provider, and is disabled while that blank chat is selected. Creating or returning to a blank chat focuses the message box in every layout. Drafts and attachments are preserved when starting another chat.

## Account

Open **Settings > Account > Log out** to sign out of the Codex account shared with the CLI. Saved chats and drafts are kept. Logout waits until responses, sign-in, and account refreshes finish; failures leave the displayed account intact. Use **Sign in with ChatGPT** to reconnect.

## Desktop control (Windows)

Enable **Settings → Workspace & access → Allow Codex to see and control this desktop**. Codex then receives the native `desktop_control` tool for screenshots, window discovery/focus, clicking, dragging, scrolling, Unicode typing, and key chords. Existing chats pick up the tools on their next response while keeping the visible transcript. No desktop skill, Electron runtime, extra desktop server, or API key is required.

Desktop access is off by default and saved across restarts. Screenshots of all monitors are sent to your Codex model as images; they are captured on demand, without recording video or saving captures to disk. Input asks for approval once per response by default; this can be changed beside the toggle. After approval, Codex refreshes its view before acting. **Stop** cancels further actions; holding **Escape** blocks desktop actions. Desktop access is separate from project file permissions and is available to Codex, not Ollama workers. Windows can refuse focus or input to elevated apps, the lock screen, or a disconnected session. Screenshots include their physical pixel dimensions and desktop origin for multiple monitors. Each response is capped at 100 desktop calls.

If a saved project folder was moved or renamed, sending a request opens Workspace settings instead of attempting a command in that invalid folder (Windows error 267). Choose the new project location to reconnect that chat and preserve the draft. You can also select **Use selected project for this chat** when switching an existing conversation to the currently selected project.

macOS and Linux retain chat/project support; this desktop backend is Windows only. Tool integration follows the [Codex app-server dynamic tool protocol](https://learn.chatgpt.com/docs/app-server).

## Future Goals

Update to another Repo for custom UI's, with a detailed menu on how to make that
To Do:

## Ollama agents

Open **Agents** on the right, connect to your Ollama server (default `http://127.0.0.1:11434`), and choose an installed model such as Qwen. **Use for this chat** hands the conversation to that model. **New agent chat** starts a separate local chat, and **Switch to Codex** returns to the account provider. New chats inherit the active provider and local model. The server, preferred model, agent instructions, per-chat provider, and token usage persist across restarts.

**Usage details / refresh** shows the lifetime Ollama token total reported to this ReDox installation since tracking began. It includes prompt and generated tokens from local chats, delegated runs, and progress summaries.

To keep Codex in charge, enable **Let Codex delegate to Ollama**. Codex receives a native `delegate_to_ollama` tool and defaults to offloading project investigation, implementation, and initial verification using the current project and access mode. Codex supplies one deliverable, scope, acceptance criteria, and verification checks, then reviews the actual diff and independently verifies the result. Ask for delegation in the main chat, or type a task in Agents and click **Ask Codex to delegate**. Progress appears at the top of Agents. Expand a task to see its output. The worker result returns to Codex for review and the final response. Task logs survive restart. Stop cancels the parent run and its worker. File changes still use the existing approval flow. Existing Codex conversations without the tool, or with an older delegation schema, start a fresh server thread once while retaining the visible transcript. Each Codex turn is limited to eight delegations. Ollama worker tokens are counted in the panel. Codex orchestration and review still use Codex quota.

Delegated jobs return a compact JSON handoff targeted at 3,200 UTF-8 bytes, containing the final worker report rather than the beginning of its commentary. The handoff includes a task ID, status, changed paths, tool-call count, and command exit results. Oversized evidence is omitted with counts and a link to the full `report.json` beside the task's `steps.md`. The worker is asked to keep its final report below 800 tokens; the byte limit is not an exact token limit. Full action records remain in the checkpoint and progress is visible in Agents. Git inventories compare dirty and untracked file contents before and after the invocation, so existing untouched changes are not attributed to the worker. Concurrent edits can still appear in that inventory; file changes outside the repository are represented only when observed through file tools. Inventory failures are explicit and require manual inspection.

Pass the returned `task_id` to `delegate_to_ollama` with specific review corrections in `task` to resume its checkpoint, including after restart. Resume requires the same project, selected model, and access mode. An active worker continues across context windows with no overall time or tool-call ceiling. Three repetitions of an identical call and result, or of a cycle of up to four calls, trigger a recovery instruction. Further repetition before sustained progress stops the worker with `stalled`. Three consecutive identical truncated responses stop it with `output_limit`. These checks carry over into the next context window. Agents labels unfinished handoffs **Needs follow-up**. A `review_required` report appears as **Ready for review** so Codex can inspect the diff and verify the results. Failed jobs retain their error and evidence.

Ollama responses stream directly into the normal conversation in either layout. The composer identifies the local model, and the Agents panel shows input/output tokens, request counts, and generation speed. Local Ollama calls do not consume Codex account quota; a remote or cloud-backed Ollama model may have its own provider usage. No models are installed or downloaded by the application.

With **Allow project tools** enabled, models advertising tool support can list/read project files, create directories, write files, and run commands. Choose a project folder and the existing access mode. Read-only mode exposes only file inspection. Workspace mode confines file tools to the project and asks for approval before changes or commands. Full access permits external file paths and skips approvals. Shell commands run with your OS account permissions and bounded output. They default to 30 seconds. For a slow build or test, the agent can set `timeout_seconds` up to 86,400 seconds. The approval prompt shows the command's timeout. Replaced files are preserved in the platform's local application-data folder under `ReDox/agent-backups`. Models without tool support still work as chat models. Image attachments require a model advertising vision support.

Worker commands use `cmd.exe /D /S /C` on Windows and `/bin/sh -c` elsewhere. Invoke `powershell.exe` explicitly for PowerShell syntax. Quoted paths and command arguments are preserved. Output capture retains the first 64 KiB of each stream, marks truncation, and continues draining so verbose commands can finish normally within the timeout. Context-size estimation excludes image payloads without copying them. See [client optimization verification](docs/CLIENT_OPTIMIZATION.md) for scope and measurements.

Direct Ollama chats also continue without an overall time or tool-call ceiling. Direct project chats use the same repeated-tool recovery check. ReDox retries a failed Ollama request up to three times after connection drops, timeouts, temporary server errors, or incomplete responses. Each retry uses the results already recorded for that step. The transcript keeps partial text until a retry replaces it, and **Stop** cancels pending retries. Persistent failures surface the error. A response cut short by the output token limit continues automatically when it produces new text. ReDox discards tool calls from incomplete responses.

In **Agents**, drag **Context window (tokens)** below the agent model or enter an exact token count. The slider uses the model list’s reported maximum, with a 262,144-token range when the list omits it. Chats and delegated tasks apply the running model’s context limit. Larger windows use more memory.

ReDox asks Ollama to write a Markdown handoff before the growing transcript fills the selected context window. The handoff lists remaining tasks, completed work, and pending verification. ReDox saves it with the window's recorded actions, then starts a fresh window with the original task, the handoff, and excerpts of recent tool evidence. If summarization fails after retries or runs out of output tokens, ReDox uses the recorded evidence to continue. The configured output allowance is capped at one quarter of the context. Individual tasks, instructions, and images must fit the selected window.

Each local chat or delegated task has its own folder at `%LOCALAPPDATA%\ReDox\agent-state\<id>` on Windows, or under `ReDox/agent-state` in the platform's local application-data folder. `steps.md` is the live checkpoint, saved before tool actions and after results. `handoffs/` holds the Markdown snapshots. Delete an inactive run's `handoffs/` to clear its snapshot history, or its whole folder to clear the saved checkpoint too. After a restart, reopen the same Ollama chat and send **Continue** to pick up the saved task, or resume a delegated task with its `task_id`. The agent is instructed to inspect actions that have no recorded result before repeating them. Existing project access and approvals apply throughout, so a run may wait for approval. Summarization requests count toward local Ollama usage.

Mock Ollama transport tests cover multiple rollovers in one delegated run, summary fallback, and checkpoint recovery. To use the updated runner in an already open copy of ReDox, close it and relaunch with `Launch ReDox.cmd`, which builds from the current source. Model decisions and verification still depend on the selected model and task.

The integration uses Ollama's native [model list](https://docs.ollama.com/api/tags), [streaming chat API](https://docs.ollama.com/api/chat), and [tool calling](https://docs.ollama.com/capabilities/tool-calling). Ollama transport, progress checkpoints, and project actions are in `src/ollama.rs`, `src/ollama_state.rs`, and `src/ollama_tools.rs`; the panel is in `src/ui/agents.rs`.

If Ollama reports a CUDA/GPU initialization failure, open **Model & tool options** in Agents, enable **CPU only (GPU troubleshooting)**, and retry. This sets `num_gpu: 0` for the request. CPU generation can be slower.

### Ollama endurance benchmark

Open `tests\perf\Stress Ollama Agent.cmd`, pick an installed model, and click **Start benchmark**. The window opens idle. The command builds its verification runner and uses Python 3.10 or newer with Tkinter. The default is `tiel-coder:35b-a3b-mtp-ud-iq4-xs` at `http://127.0.0.1:11434` with a requested 64,000-token context. The benchmark scores reasoning, code-tracing, memory, and read-only fixture tasks through ReDox's Ollama transport in one chat.

During a run, pick another model and choose **Switch after task** to apply it at the next round. The chat keeps its history and overall score. The window shows the active model, any queued switch, and the current model's score. `benchmark.jsonl` records the model used for each task alongside its result, in the same folder as `steps.md`.

The benchmark keeps going until **Stop** in the window, **Ctrl+C** in terminal mode, or a task exceeds the watchdog deadline, which defaults to 900 seconds. After a run ends in the window, pick a model and click **Start benchmark** again to begin a fresh chat and score. Wrong answers are scored and the run continues. The progress view shows generated tokens, token rate, and context rollovers.

The reasoning and tools suite reports **True** scores with the original JSON fence requirement and **Ex. JSON** scores that also accept valid bare JSON. Both validate exact answer values and the required file read. The log stores them as `passed` and `passed_ex_json`.

The score display shows total task time in seconds. Each logged task adds its `elapsed` time, including tool calls and Cargo builds. In `benchmark.jsonl`, the final row's `total_task_seconds` gives the run total. For a run with model switches, use the final `model_total_task_seconds` row for each model to get its separate total.

This PowerShell command runs seven tasks in the terminal.

```powershell
& '.\tests\perf\Stress Ollama Agent.cmd' --rounds 7
```

Use `--watchdog 180` for a shorter per-task deadline, `--context 4096` to reach rollovers sooner, or `--model` and `--endpoint` for another Ollama configuration. Add `--gui` to open the window when passing options.

To run an ordered comparison, choose a model, suite, and **Tasks per batch**, then click **Add batch** for each entry. For example, add 25 Reasoning and tools tasks for each model, then 5 Rust Build tasks for each model. Choose one **Context tokens** value and click **Start queue**. Each batch reports its own score and results file.

With **Keep each model’s chat and request warm weights between batches** checked, the queue resumes that model’s chat on later entries and asks Ollama to keep it loaded. Ollama may still unload a model to make room for another. Clear the checkbox for a fresh chat and cold start in each batch. [Ollama model residency](https://docs.ollama.com/faq#how-does-ollama-handle-concurrent-requests)

#### Rust Build benchmark

Choose **Rust Build** in the benchmark window before starting, or pass `--suite rust` in the terminal. The suite rotates through a dependency scheduler, a transaction ledger, and a filter-expression parser. The model submits Rust source using the standard library. ReDox supplies a small Cargo crate and a driver that references the required public interface, then runs `cargo build --offline`.

A successful Cargo build passes. The benchmark records compiler diagnostics and build time separately from generation time. It checks compilation and public interfaces. The generated executable stays in the Cargo target directory for inspection. A build that exceeds `--build-timeout` is recorded as inconclusive and excluded from the scored total. The default build deadline is 180 seconds, and the default generation allowance for this suite is 4,096 tokens.

```powershell
& '.\tests\perf\Stress Ollama Agent.cmd' --suite rust --rounds 3
```

#### Qwen 3.6 Rust profile

Create `Aisnley-Ox-1-bbb:35b` on the Ollama server where `qwen3.6:35b` is installed. The profile reuses the stock model weights and adds a Rust system prompt and [Qwen's coding sampling preset](https://huggingface.co/Qwen/Qwen3.6-35B-A3B). ReDox enables thinking for this profile and requests a direct answer if reasoning reaches the response limit before any answer is produced. Select the model in Agents and set **Model & tool options > Response limit** to 16,000 for longer implementations at a 64,000-token context. Thinking counts toward the response limit. ReDox reserves up to one quarter of the context for generation.

The qualified Ollama service runs Aisnley-Ox-1-bbb's startup sequence for any client connected to it. It holds the first inference or model preload request until one attempt scores 10/10 on strict Reasoning and Tools tasks and compiles the Rust filter-expression parser with local Cargo. On a failed task it unloads the model and starts again. It preserves the passing transcript for inference, checks residency on subsequent requests, and qualifies again after observing an unload or a change in loading settings. Native Ollama requests renew a 15-minute keep-alive. The OpenAI-compatible endpoints use the profile's 64,000-token context.

Start the qualified endpoint from the project folder while Ollama is running on its normal port. ReDox's default connection and the stress benchmark discover it locally. The benchmark starts its own local endpoint when needed and closes it on exit. Other clients should use `http://127.0.0.1:11435` as their Ollama server or `http://127.0.0.1:11435/v1` as their OpenAI base URL. Clients pointed directly at Ollama's upstream port `11434` use that upstream's behavior. The benchmark waits for qualification before timing each scored task. The service console shows each qualification stage. Press Ctrl+C in the standalone service console to stop it.

```powershell
& '.\Launch ReDox.cmd' --qualified-ollama
```

To use the Ollama CLI from a second PowerShell window:

```powershell
$env:OLLAMA_HOST = 'http://127.0.0.1:11435'
ollama run Aisnley-Ox-1-bbb:35b
```

```powershell
ollama create Aisnley-Ox-1-bbb:35b -f docs/ollama/Qwen36Rust.Modelfile
& '.\tests\perf\Stress Ollama Agent.cmd' --model Aisnley-Ox-1-bbb:35b --suite rust --rounds 3 --context 64000 --output 16000 --watchdog 1500
```

Add `--gui` to the benchmark command to open the model picker and start the run from the window.

## Cursor animation

Animation inspiration: [Swarm Rebuild by Dhvanil Patel](https://github.com/DhvanilPatel/swarm-cursors), which credits the [original Swarm animation by Joel Califa](https://califa.github.io/swarm-site/). This application uses a native Rust adaptation.

## Message formatting

Both layouts display PNG, JPEG, WebP, and animated GIF images from Markdown image tags or image-file links, including local paths, workspace-relative paths, embedded image data, and HTTP(S) URLs. Uploaded images are previewed in the conversation. Generated-image events are retained in live responses and imported history. Inline generated data persists under `%LOCALAPPDATA%\ForgeDesk\media` on Windows, `~/Library/Application Support/ForgeDesk/media` on macOS, and `$XDG_DATA_HOME/ForgeDesk/media` (default `~/.local/share/ForgeDesk/media`) on Linux. If Codex supplies no accessible image data or file, the message explains that instead of hiding the output.

Video links (MP4, WebM, MOV, MKV, AVI, M4V) show **Play video**, opening the file or URL in the default player/browser. Video playback is external; the app does not embed a video decoder. Viewing media does not request another model generation.

Both Codex and Gemini layouts now use the shared Markdown/math renderer in `src/ui/markdown.rs`. Equations, fractions, powers, subscripts, logarithms, headings, links, lists, and tables are formatted. Inline math accepts `$...$` or `\(...\)`; display math accepts `$$...$$` or `\[...\]`. Code blocks retain literal source and indentation, including HTML and LaTeX code examples. Right-click an equation to copy its LaTeX. Unsupported equations remain readable as source with an explanation on hover.

Text and code use the bundled STIX Two Math font as a fallback for Unicode arrows (`→`, `⇒`), math symbols (`∈`, `ℝ`), and box drawing (`┌`, `─`, `│`).

Long conversations lay out the messages around the viewport. While a reply is streaming, scrolling away keeps your place. **Jump to latest** resumes following the reply.

Use the pencil beside a sent prompt to edit it in place. **Cancel** keeps the conversation as it was. **Send** generates a new reply from that point. Use the arrows beside the edited prompt to switch versions and continue either conversation path. Existing attachments stay with the edit unless you remove them. Project files and completed tool actions stay on disk.

Codex edits use a saved turn boundary when one is available. Older messages without a saved turn ID use the transcript loaded in ReDox, subject to the 128 KiB text limit.

## Verification

```powershell
cargo fmt --check
cargo test --locked
cargo clippy --all-targets -- -D warnings
cargo build --locked
```


ReDox checks for updates at startup and offers **Yes** (download and restart) or
**No** when a newer release is available. **Never show again** is saved and can
be changed in **Settings ? Updates**, where manual checks and **Update and restart**
remain available. Private releases require `REDOX_UPDATE_TOKEN`.

On Windows, updates download the release asset named `ReDox-Windows-x64.exe`
and replace the running executable in its current folder without rebuilding.
The generated helper is `<executable folder>/temp/update-<id>/update.ps1`.
It waits for ReDox to exit, preserves `redox-previous.exe` there, replaces the
executable, and restarts it. Replacement failures restore the backup; helper
errors are recorded in `update-error.txt` in the same directory. The installation
folder must be writable. Chats and settings remain in application storage.
Updates wait until the current response finishes before closing the application.
Other platforms can use **View release and download**.

The launcher rebuilds changed project files before starting the executable.
Use `Build Release.bat` to build without launching. Release tags must use `vMAJOR.MINOR.PATCH` or `MAJOR.MINOR.PATCH`.
A test tag must include the compiled Windows asset to support installation.

The default update repository is `saffroncarp1106/ReDoxCodex`.
`REDOX_UPDATE_REPOSITORY` optionally overrides it. For private releases, set
`REDOX_UPDATE_TOKEN` in the environment to a GitHub token with read access to
that repository. Each user needs their own access; tokens are not bundled or
saved in application settings. Git credential-manager access alone does not
authenticate the updater's GitHub API request.

Replying to a conversation imported from Codex history creates a separate ReDox chat with the loaded conversation context. The original history remains separate.

Loaded Codex history chats also appear under their project folders. History rows include the project folder name; use Load more chats to discover older conversations.

Right-click a **Codex history** chat and choose **Add to original project** to group it under its saved working folder in Projects. Chats with the same folder appear together.

Project folders can be hidden from the sidebar by right-clicking a project and choosing **Archive project**. Expand **Archived projects** and choose **Restore** to show it again. Archiving preserves the folder on disk and its chats.

The bottom-left sidebar shows account quota percentages when loaded, or Ollama token counts and context usage. Use Usage details / refresh to load or refresh account limits.

Sending a message while a response is running queues a separate follow-up turn. It starts automatically after the current response finishes.

Standalone Discord presence test: with Discord desktop running, use `cargo run --bin presence-test`. Press Enter to clear activity and exit. This test is separate from the ReDox app.

## Discord Rich Presence

In Settings > About, select Enable Presence and customize Details (first line) and State (second line). Changes save automatically, including whether presence is enabled, and publish after a short typing pause. Presence is disabled by default. Blank fields fall back to ReDox / Vibing; each field supports up to 128 characters. The displayed app name is controlled by the registered Discord application.

Discord desktop must be running and signed in. ReDox connects locally using its public Application ID without a bot token or account login. The worker retries unavailable connections every 15 seconds. Disable Presence clears the activity; shutdown attempts to clear it without blocking app exit indefinitely. Only your custom text and elapsed session time are sent, not chat text or project paths. The About page shows connection status. The standalone presence-test program is separate; exit it before testing the integrated presence with the same Application ID.

## Export the existing UIs into Studio

From this directory, run `cargo run --locked --features design-export --bin export-designs`. This uses Redox's actual renderers in an offscreen session at 1100 × 900 to export welcome, conversation and eight Settings scenes for each of the five themes into `../ReCreation/skins/redox/scenes/`, plus the five main originals. It skips model transport and stored account/history. Set `REDOX_EXPORT_REVIEW=1` to also generate PNGs under `target/design-review`. Rebuild Studio after refreshing embedded main templates.

The generated files preserve source geometry and font artwork. Studio can edit the resulting layers, while live application behavior and responsive layout remain in ReDox's renderers.

The debug and release launchers also build `redox-studio-native-preview.exe` beside ReDox. Studio can call it with `<theme> <scene> <width> <height> <absolute-output.redoxskin>` to capture the current renderer at a requested viewport. It runs offscreen with dummy account and chat data and does not start model transport.

## Wii interface (v0.2.0)

Choose **Wii** from the UI menu for the imported silver-and-white layout, cyan accents, channel shortcuts, glossy controls, player cursors and embedded feedback sounds. Settings > Appearance > Wii controls sounds and motion. The Wii presentation is integrated from the sibling ReDoxWiiUI source; existing application actions and model transports remain shared.
