# Ollama Stresser

Run **Launch Ollama Stresser.cmd** to open the desktop app. You can copy this folder to another location and launch it there. The desktop app uses Python 3.10 or newer with Tkinter. The Rust Build suite also needs Cargo and a Rust toolchain.

The window opens idle and defaults to Ollama at http://127.0.0.1:11434. To use a different server, enter its URL and click **Refresh**. Select an installed model and a suite, then click **Start benchmark**. Clear **Run until stopped** to use a fixed task count.

- **Reasoning and tools** scores reasoning, code tracing, memory, and read-only fixture tasks. The read tool opens the fixture generated for the current task. **True** requires fenced JSON, while **Ex. JSON** also accepts valid bare JSON.
- **Rust Build** asks for three Rust interfaces in rotation and checks each submission with an offline Cargo build. The app records diagnostics and build time. Build timeouts are recorded as inconclusive. The app leaves compiled output in the run folder.

Use **Switch after task** to change models during a run while retaining chat history. For an ordered comparison, use **Add batch** to queue a model, suite, and task count. The queue can retain each model's chat and request that Ollama keep its weights warm between batches. Clear that option to request a cold start for every batch.

Use **Stop** to stop a run or queue. The per-task generation watchdog defaults to 900 seconds, and the Cargo build deadline defaults to 180 seconds. The app tracks token rate and context rollovers. It drops older exchanges as the estimated context fills and retains the current memory task's marker.

On Windows, reports are written to the OllamaStresser\results folder under %LOCALAPPDATA%. The app logs each report's full path. Reports include the selected model, score, elapsed time, token usage, and tool and build results where applicable.

## Terminal

Run these PowerShell commands from the stresser folder. Press Ctrl+C to stop. Pass **--gui** alongside flags to open the desktop app with those settings.

~~~powershell
& '.\Launch Ollama Stresser.cmd' --model MODEL --rounds 7
& '.\Launch Ollama Stresser.cmd' --model MODEL --suite rust --rounds 3 --output 16000
& '.\Launch Ollama Stresser.cmd' --help
~~~

The app uses Ollama's native streaming API. If you choose an endpoint with a proxy or a qualification step, any delays it introduces during a request count toward the task time. [Ollama chat API](https://docs.ollama.com/api/chat)
