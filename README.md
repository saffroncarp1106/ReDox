# ReDox

**A native Rust desktop client for Codex and Ollama.**
Because 1.2 GB of RAM for a chat window is ridiculous.

ReDox started as a rewrite of the Codex desktop front end without Electron. It is a single native app (Rust + [egui](https://github.com/emilk/egui)) that talks to your Codex account and to any Ollama server you point it at, plus a handful of extras: five swappable looks, a built-in Ollama coding agent, optional desktop control, and a couple of hidden surprises.

> **Version 0.2.3.** Windows is the main target. macOS and Linux build and run (chat, projects, Ollama), minus the features marked *Windows only* below.

**Why the name?** In a redox reaction, one thing gets reduced while another gets oxidized. Here, the Electron app got *reduced* (the aim was roughly a tenfold drop in memory use) and the rewrite is in Rust, which is what iron turns into when it oxidizes. Spend the RAM you saved on a bigger Ollama model.

---

## Getting started

### What you need

- **Rust.** The repo pins 1.88.0 in `rust-toolchain.toml`, so `rustup` picks it up for you.
  - Windows: the Visual Studio C++ build tools.
  - macOS: Xcode's command line tools.
  - Linux: the usual native window and OpenGL development packages (X11 or Wayland).
- **For Codex:** the Codex CLI, signed in. ReDox launches `codex app-server --stdio` behind the scenes, so it shares an account with the CLI. It looks for the CLI automatically; if it can't find it, point ReDox at the executable in Settings.
- **For Ollama:** a running Ollama server (default `http://127.0.0.1:11434`). ReDox never installs or downloads models for you.

### Run it

| Platform | Command |
| --- | --- |
| Windows | Double-click `Launch ReDox.cmd` (or `Launch ReDox.cmd --debug` for a faster debug build) |
| macOS | `./"Mac Build.sh"` (debug) or `./"Mac Build.sh" --release` |
| Linux | `cargo run --locked --bin redox` |

On Windows the launcher rebuilds anything that changed and then starts the fresh executable. Cargo skips unchanged work, so repeat launches are quick. If the build fails, the launcher stops instead of quietly starting an old copy. It builds from your local files and never downloads source updates. `Build Release.bat` and `Build Debug.bat` build without launching.

Prefer not to build? Windows releases ship as `ReDox-Windows-x64.exe` on the [Releases page](https://github.com/saffroncarp1106/ReDoxCodex/releases).

### First launch

1. Open **Settings > Account** and choose **Sign in with ChatGPT**.
2. Pick a project folder.
3. Start chatting.

To sign out, use **Settings > Account > Log out**. Your saved chats and drafts stay put. Logout waits for any running responses to finish, and if it fails, your account stays shown as signed in.

---

## What's inside

### Chatting

- **Projects and history.** Codex history shows up under its project folders (use **Load more chats** for older ones). Right-click a history chat and choose **Add to original project** to group it with its working folder. Replying to an imported chat starts a separate ReDox chat, so the original stays untouched.
- **Tidying up.** Archive chats with the small **×** on a row and restore them from **Archived chats**. Right-click a project and choose **Archive project**, then restore it from **Archived projects**. These are local choices; they don't change anything on the server or on disk.
- **New chat** reuses an empty chat for the same project and provider, and drafts and attachments survive when you start another one.
- **Edit sent prompts.** Click the pencil next to a prompt to rewrite it. **Send** creates a new branch, and the arrows beside the prompt let you flip between versions. Project files and finished tool actions stay on disk.
- **Queued follow-ups.** Send a message while a reply is running and it waits its turn.
- **Long conversations stay smooth.** Messages are laid out around the viewport, and scrolling up during a stream keeps your place until you hit **Jump to latest**.
- **Rich messages.**
  - Markdown with tables and code blocks that keep their exact source.
  - Math: `$...$` or `\(...\)` inline, `$$...$$` or `\[...\]` for display. Right-click an equation to copy its LaTeX.
  - PNG, JPEG, WebP, and animated GIF images from Markdown tags, local paths, embedded data, or URLs.
  - Video links (MP4, WebM, MOV, MKV, AVI, M4V) get a **Play video** button that opens your default player. ReDox has no built-in video decoder.
- **Attachments.** Drag files in or paste images from the clipboard.
- **Usage at a glance.** The bottom-left of the sidebar shows your account quota percentages, or Ollama token counts and context use. **Usage details** refreshes them.

### Looks

Switch between interfaces from the **UI** menu. All of them work with both Codex and Ollama.

| UI | What it is |
| --- | --- |
| **Codex** | The default. |
| **Claude** | A Claude-flavored take on the same app. |
| **Gemini** | A Gemini-flavored take. |
| **Wii** | Silver and white with cyan accents, channel shortcuts, glossy buttons, player cursors, and sound effects (sounds are *Windows only*). Settings > Appearance > Wii controls sounds and motion. |
| **Windows 98** (and **Windows 98 Dark**) | Hidden until you find it. See below. |

Want your own look? UI packs are small JSON files (colors, heading text, an optional PNG background). Drag one into the app or use **UI > Custom…** to import it, no rebuild required. See [UI packs](docs/UI_PACKS.md) and [Customizing the interface](docs/UI_CUSTOMIZATION.md). The startup cursor animation is configurable under **Settings > Cursors**.

<details>
<summary><b>Spoiler: unlocking Windows 98</b></summary>

Open **Settings > About** and click the ReDox logo five times. Windows 98 and Windows 98 Dark then appear in the **UI** menu, with a teal desktop, classic bevelled windows, a Start taskbar, classic scrollbars, and a pixel-drawn title bar.

It also includes a **Terminal** view of your conversation:

- Black and white, with a `>` prompt. **Enter** sends, **Shift+Enter** adds a new line.
- Codex file changes appear as dashed add and remove blocks with line counts, and replies stream beneath `----- redox -----`.
- Type `/` to see commands and press **Tab** to complete one. `/help` lists them all, `/stop` halts the current response, and `/settings` opens things like caret width, title bar color, and the **CRT filter** (scanlines, RGB glow, bloom, and curvature, each adjustable down to zero).
- **−** and **+** zoom the text from 75% to 200%, and **F11** (or the **Fullscreen** button) fills the screen.
- Switching between Chat and Terminal keeps your draft.

The Terminal font is credited below.

</details>

### Ollama agents

Open **Agents** on the right, connect to your server, and pick an installed model (Qwen works well). Then:

- **Use for this chat** hands the current conversation to that model.
- **New agent chat** starts a separate local chat.
- **Switch to Codex** goes back to your account provider.

The server address, preferred model, instructions, per-chat provider, and token totals all persist across restarts. Local Ollama calls don't touch your Codex quota (a cloud-backed Ollama model might have its own usage). Image attachments need a model that advertises vision support.

**Project tools.** Turn on **Allow project tools** and tool-capable models can list and read files, create folders, write files, and run commands. The project's access mode decides how far that goes:

| Mode | What the model can do |
| --- | --- |
| Read-only | Inspect files only. |
| Workspace | Work inside the project, with an approval prompt before changes or commands. |
| Full access | Reach outside the project and skip approvals. |

Commands run with your own OS permissions, time out after 30 seconds by default (a model can ask for up to 86,400 seconds for a slow build, and the approval prompt shows the timeout), and keep the first 64 KiB of output. Files that get overwritten are backed up first. Models without tool support still work as plain chat models.

**Let Codex delegate.** Enable **Let Codex delegate to Ollama** and Codex gets a `delegate_to_ollama` tool for offloading investigation, implementation, and first-pass verification to your local model. Codex hands over one deliverable with acceptance criteria, then reviews the real diff and double-checks the result before answering. You can ask for this in the main chat, or type a task in Agents and click **Ask Codex to delegate**. Progress and output show up at the top of the Agents panel and survive a restart. A turn can delegate at most eight times, and Codex's own orchestration still uses Codex quota. You can resume a delegated task later by giving Codex its `task_id` plus review notes.

**Long runs without babysitting.**

- **Context window.** Drag **Context window (tokens)** or type an exact number. Bigger windows use more memory.
- **Handoffs.** Before the context fills up, ReDox asks the model to write a Markdown handoff (what's done, what's left, what still needs checking), then starts a fresh window from the original task plus that handoff and recent tool evidence.
- **Retries.** Failed requests are retried up to three times (connection drops, timeouts, temporary server errors, incomplete replies). **Stop** cancels any pending retry.
- **Loop guard.** Repeating the same call and result over and over triggers a recovery nudge, and after that the worker stops as `stalled`.
- **Checkpoints.** Each run keeps a live `steps.md` checkpoint. After a restart, reopen the chat and send **Continue**.

Having GPU or CUDA trouble? Open **Model & tool options** in Agents, tick **CPU only (GPU troubleshooting)**, and retry. It's slower but it works.

#### The qualified endpoint and the Rust profile

This is the most experimental part of the project, so here is the plain-English version.

- `docs/ollama/Qwen36Rust.Modelfile` defines a Rust-focused profile of `qwen3.6:35b` called **Aisnley-Ox-1-bbb:35b**. Create it with `ollama create Aisnley-Ox-1-bbb:35b -f docs/ollama/Qwen36Rust.Modelfile`. It uses Qwen's recommended coding sampling settings and a 64,000-token context. For long implementations, raise **Model & tool options > Response limit** to around 16,000.
- The *qualified endpoint* sits in front of Ollama on port `11435`. The first time anything asks that profile for an answer, the endpoint holds the request until the model passes a self-check (10 strict reasoning-and-tools tasks, plus writing a small Rust expression parser that compiles with your local Cargo). If it fails, the model is unloaded and it tries again. It re-checks whenever it sees the model unload.
- Start it on Windows with `Launch ReDox.cmd --qualified-ollama` (needs Python 3 and Cargo). Then point ReDox, the Ollama CLI (`OLLAMA_HOST=http://127.0.0.1:11435`), or any OpenAI-style client (`http://127.0.0.1:11435/v1`) at it. Clients that talk to port `11434` directly get plain Ollama behavior.

### Windows-only extras

**ReDox Remote control** (desktop control). Under **Settings > Workspace & access**, tick *Allow the selected agent to see and control this desktop*. The agent gets a `redox_remote_control` tool for screenshots, window discovery and focus, clicking, dragging, scrolling, typing, and key combos. It works with Codex, and with Ollama models that support both tools and vision (delegated Ollama workers don't get it).

- It's **off by default** and the choice is remembered.
- Screenshots of all monitors go to your model as images, captured on demand. Nothing is recorded or saved to disk.
- By default, input asks for approval once per response. **Stop** or holding **Escape** cancels actions immediately.
- Each response is capped at 100 desktop calls.
- Windows can refuse input to elevated apps, the lock screen, or a disconnected session.
- This is separate from project file permissions.

**Resource limiter.** **Settings > Resource limiter** lets you cap what ReDox (and everything it launches, including Codex and shell commands) can use: a CPU percentage, a RAM percentage, and an upload percentage based on the connection speed you enter in Mbps. Tick **Enable resource budgets**, choose your numbers, and click **Apply resource budgets**. It starts disabled with proposed values of 50% / 50% / 50% of 20 Mbps.

Worth knowing before you use it:

- Limits come from Windows Job Objects. Hitting the memory ceiling can make allocations fail, and CPU readings can spike briefly.
- Coverage is outgoing Wi-Fi and Ethernet traffic. GPU, disk bandwidth, and incoming network throttling aren't available.
- External Ollama servers, other apps, and cloud-side computation aren't in ReDox's process tree, so they aren't limited.
- Each step reports its own result, including OS errors, and clearing the checkbox and applying removes the caps.

**Notifications.** A desktop notification says "ReDox has a response." when a Codex or Ollama reply finishes successfully (not on cancel or error). Windows' notification settings, including Do not disturb, decide whether you see it.

### Everything else

**Discord Rich Presence.** In **Settings > About**, click **Enable Presence** and set the two lines of text. It's off by default. ReDox talks to your local Discord desktop app, with no bot token or login, and shares only your custom text and elapsed time (never chat text or project paths). Blank fields fall back to "ReDox / Vibing".

**Updates.** ReDox checks GitHub Releases at startup and offers **Yes** or **No**. **Never show again** is saved, and you can still check manually under **Settings > Updates**.

- On Windows, it downloads `ReDox-Windows-x64.exe`, waits for the current response to finish, swaps the executable in place, and restarts. It keeps `redox-previous.exe` as a backup and restores it if the swap fails. The install folder must be writable. Chats and settings live elsewhere and are kept.
- Other platforms show **View release and download**.
- Using a fork or a private repo? Set `REDOX_UPDATE_REPOSITORY` (default `saffroncarp1106/ReDoxCodex`) and, for private releases, `REDOX_UPDATE_TOKEN` with a read-only GitHub token. Tokens are never bundled or saved.
- Release tags look like `v1.2.3` or `1.2.3`, and a test tag needs the compiled Windows asset attached.

**Window behavior.** ReDox restores your last window size. The minimum is 900 × 600, and a saved size below that falls back to 1100 × 900. If a project folder has been moved or renamed, sending a message opens Workspace settings so you can reconnect it without losing your draft.

**Where ReDox keeps things.** Under your local app-data folder (`%LOCALAPPDATA%` on Windows, `~/Library/Application Support` on macOS, `~/.local/share` on Linux):

| Folder | Contents |
| --- | --- |
| `ReDox/media` | Generated images saved from replies |
| `ReDox/pasted-images` | Images pasted from the clipboard |
| `ReDox/agent-state/<id>` | Ollama run checkpoints (`steps.md`, `handoffs/`) |
| `ReDox/agent-backups` | Copies of files an Ollama agent replaced |
| `ReDox/ollama-lifetime-tokens` | Your lifetime Ollama token total |

Delete a finished run's `handoffs/` folder to clear its snapshot history, or the whole run folder to clear its checkpoint too.

---

## For tinkerers

### Project layout

| Path | What lives there |
| --- | --- |
| `src/` | The app and its logic: Codex and Ollama transport, delegation, history, updates, resource limits, desktop control |
| `src/ui/` | Everything you see: layouts, themes, icons, fonts, UI preferences |
| `src/ui/layouts/` | Built-in UI configs (JSON) |
| `src/ui/studio_runtime/` | The bundled Studio runtime library, so normal builds work from this repo alone |
| `tests/` | Automated tests |
| `docs/` | Notes, measurements, and release notes |

The rule of thumb from `AGENTS.md`: presentation code stays in `src/ui/`, model transport and app actions stay outside it. Cargo output goes in `target/`, and `temp/` is a gitignored archive for old scratch files.

### Extra programs

| Binary | Purpose | Run it |
| --- | --- | --- |
| `redox` | The app | `cargo run --locked --bin redox` |
| `presence-test` | Standalone Discord presence check (press Enter to clear and exit) | `cargo run --bin presence-test` |
| `ollama-qualifier` | Helper used by the qualified endpoint | Built by `Launch ReDox.cmd --qualified-ollama` |
| `export-designs` | Exports ReDox's real renderers into Studio skins | `cargo run --locked --features design-export --bin export-designs` |
| `redox-studio-native-preview` | Renders a theme and scene offscreen for Studio | Built by the launchers |

`export-designs` renders at 1100 × 900 and writes into a sibling `../ReCreation/skins/redox/` folder, so it expects that companion repo next to this one. Set `REDOX_EXPORT_REVIEW=1` to also write review PNGs under `target/design-review`.

### Building and checking

```sh
cargo fmt --check
cargo test --locked
cargo clippy --all-targets -- -D warnings
cargo build --locked
```

Development builds keep line numbers for backtraces but trim other debug info. For full local-variable debugging, set `CARGO_PROFILE_DEV_DEBUG=2` before building.

### More reading

- [Build speed measurements](docs/BUILD_SPEED.md)
- [Codex pipeline performance](docs/CODEX_PIPELINE_PERFORMANCE.md): local processing and allocation savings. These don't claim model-token or quota savings.
- [Client optimization verification](docs/CLIENT_OPTIMIZATION.md)
- [Build and task review](docs/BUILD_AND_TASK_REVIEW.md)
- [Discord presence design notes](docs/DISCORD_PRESENCE.md)
- [Sources](docs/SOURCES.md) and the [release notes](docs/)

### Roadmap

- Move custom UI work into its own repo, with a proper guide for making your own looks.

---

## Credits and licenses

ReDox stands on a lot of other people's work. Thank you to everyone below.

### Fonts

| Font | Used for | Made by | License |
| --- | --- | --- | --- |
| **PxPlus IBM BIOS** (v2.2, 2020) | The Windows 98 Terminal | **VileR**, from [The Ultimate Oldschool PC Font Pack](https://int10h.org/oldschool-pc-fonts/), © 2016–2020 VileR | [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/). Bundled in `src/ui/assets/fonts/ibm-bios/` with its original [license](src/ui/assets/fonts/ibm-bios/LICENSE.TXT) and readme. |
| **STIX Two Math** | Math, arrows, and box-drawing fallback in text and code | The [STI Pub companies](https://github.com/stipub/stixfonts) (AMS, AIP, APS, ACS, IEEE, Elsevier); STIX Two by Tiro Typeworks | [SIL Open Font License 1.1](https://openfontlicense.org/). Bundled through the [`latex-rust`](https://github.com/jscarr64/LaTeX-Rust) crate. |
| **Hack, Ubuntu-Light, Noto Emoji, emoji-icon-font** | Default text, monospace, and emoji glyphs | Bundled by egui in [`epaint_default_fonts`](https://docs.rs/epaint_default_fonts) | Each font keeps its own license (SIL OFL, Ubuntu Font Licence, MIT); the license texts ship in that crate. |
| **Segoe UI, Tahoma, Georgia** | Interface text on Windows | Microsoft | **Not bundled.** If present, ReDox loads them from your own Windows Fonts folder at runtime. Nothing is redistributed. |

The PxPlus font is licensed under share-alike terms. If you redistribute a modified version of that font file, it must stay under CC BY-SA 4.0 with credit to VileR.

### Art, animation, and sound

- **Startup cursor animation:** a native Rust adaptation of [Swarm Rebuild](https://github.com/DhvanilPatel/swarm-cursors) by **Dhvanil Patel**, which is itself based on the [original Swarm animation](https://califa.github.io/swarm-site/) by **Joel Califa**.
- **Wii interface:** hand, button, arrow, and footer artwork come from the [Wii Menu community file on Figma](https://www.figma.com/design/TGmVIfpPewFNYOoNIylzfw/Wii-Menu--Community---Copy-). The red, green, and gold player cursors are recolored variants of the original hand. Sound effects (Hover, Click, Bloomp, Succes) are bundled in `src/ui/assets/wii/`. Details are in [`src/ui/assets/wii/README.md`](src/ui/assets/wii/README.md). Everything is bundled, and ReDox makes no Figma requests at runtime.
- **Windows 98 interface:** a recreation inspired by the classic desktop look.

### Libraries

Built with [egui and eframe](https://github.com/emilk/egui) (Emil Ernerfeldt and contributors), [egui_commonmark](https://crates.io/crates/egui_commonmark), [pulldown-cmark](https://github.com/pulldown-cmark/pulldown-cmark), [latex-rust](https://github.com/jscarr64/LaTeX-Rust), [reqwest](https://github.com/seanmonstar/reqwest), [tokio](https://tokio.rs/), [image](https://github.com/image-rs/image), [rfd](https://github.com/PolyMeilex/rfd), [arboard](https://github.com/1Password/arboard), [serde](https://serde.rs/), [windows-sys](https://github.com/microsoft/windows-rs), [winrt-toast-reborn](https://crates.io/crates/winrt-toast-reborn), [winresource](https://crates.io/crates/winresource), and the community [discord-rich-presence](https://github.com/vionya/discord-rich-presence) crate by vionya. Each crate keeps its own license. The full list with exact versions is in `Cargo.toml` and `Cargo.lock`.

### Services, docs, and models

- [Codex app-server protocol](https://learn.chatgpt.com/docs/app-server) for talking to the Codex CLI.
- [Ollama's API](https://docs.ollama.com/api/chat): [model list](https://docs.ollama.com/api/tags), [streaming chat](https://docs.ollama.com/api/chat), and [tool calling](https://docs.ollama.com/capabilities/tool-calling).
- [Qwen](https://huggingface.co/Qwen/Qwen3.6-35B-A3B) from the Qwen team at Alibaba Cloud. The Rust profile builds on `qwen3.6:35b` and follows Qwen's recommended coding settings. ReDox does not distribute any model weights.
- Microsoft's [job object](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_net_rate_control_information) APIs for the resource limiter, and [Discord's Rich Presence](https://docs.discord.com/developers/rich-presence/best-practices) docs.

### Trademarks

ReDox is an independent project. It is not affiliated with or endorsed by OpenAI, Anthropic, Google, Microsoft, Nintendo, or Discord. Codex, ChatGPT, Claude, Gemini, Windows, Wii, and Discord are trademarks of their respective owners. The Codex, Claude, Gemini, Windows 98, and Wii interfaces are homages that exist for fun and personal use.

### License

ReDox's own source code doesn't have a license file yet, so by default all rights remain with the author. The bundled third-party pieces above keep their own licenses.
