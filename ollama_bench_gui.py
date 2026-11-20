"""Window for the Ollama endurance benchmark."""

import queue
import threading
import tkinter as tk
from tkinter import ttk
from tkinter.scrolledtext import ScrolledText

from ollama_client import api_url


def launch(options, control, run_benchmark, installed_models, run_queue):
    root = tk.Tk()
    root.title("Ollama Stresser")
    root.geometry("930x690")
    root.minsize(690, 540)
    events = queue.SimpleQueue()
    endpoint = tk.StringVar(value=options.endpoint)
    model = tk.StringVar(value=options.model)
    suites = {"Reasoning and tools": "general", "Rust Build": "rust"}
    suite = tk.StringVar(value=next(label for label, key in suites.items()
                                    if key == getattr(options, "suite", "general")))
    suite_names = {key: label for label, key in suites.items()}
    tasks = tk.StringVar(value=str(options.rounds or 25))
    unlimited = tk.BooleanVar(value=options.rounds is None)
    context = tk.StringVar(value=str(options.context))
    keep_context = tk.BooleanVar(value=True)
    plan = []
    queue_active = False
    batch_status = ""
    active = None
    model_status = tk.StringVar(value="Ready")
    overall_score = tk.StringVar(value="Overall   Waiting for results")
    current_score = tk.StringVar()
    progress = tk.StringVar(value="Choose a model and benchmark, then start.")
    closing = False

    frame = ttk.Frame(root, padding=14)
    frame.pack(fill="both", expand=True)
    frame.columnconfigure(0, weight=1)
    frame.rowconfigure(6, weight=1)
    toolbar = ttk.Frame(frame)
    toolbar.grid(row=0, column=0, sticky="ew", pady=(0, 10))
    toolbar.columnconfigure(1, weight=1)
    ttk.Label(toolbar, text="Ollama endpoint").grid(row=0, column=0, padx=(0, 8), sticky="w")
    endpoint_entry = ttk.Entry(toolbar, textvariable=endpoint)
    endpoint_entry.grid(row=0, column=1, columnspan=4, sticky="ew")
    ttk.Label(toolbar, text="Ollama model").grid(row=1, column=0, padx=(0, 8), sticky="w")
    picker = ttk.Combobox(toolbar, textvariable=model, values=[options.model], state="readonly")
    picker.grid(row=1, column=1, sticky="ew", pady=(7, 0))
    ttk.Label(toolbar, text="Benchmark").grid(row=2, column=0, padx=(0, 8), pady=(7, 0), sticky="w")
    suite_picker = ttk.Combobox(toolbar, textvariable=suite, values=list(suites), state="readonly")
    suite_picker.grid(row=2, column=1, sticky="ew", pady=(7, 0))
    settings = ttk.Frame(toolbar)
    settings.grid(row=3, column=0, columnspan=5, sticky="w", pady=(7, 0))
    ttk.Label(settings, text="Context tokens").pack(side="left")
    context_entry = ttk.Spinbox(settings, from_=2048, to=1048576, increment=1024,
                                textvariable=context, width=9)
    context_entry.pack(side="left", padx=(6, 18))
    ttk.Label(settings, text="Tasks per run or batch").pack(side="left")
    tasks_entry = ttk.Spinbox(settings, from_=1, to=1000000, textvariable=tasks, width=6)
    tasks_entry.pack(side="left", padx=6)
    unlimited_button = ttk.Checkbutton(settings, text="Run until stopped (single run)", variable=unlimited)
    unlimited_button.pack(side="left", padx=(8, 0))

    queue_box = ttk.LabelFrame(frame, text="Benchmark queue", padding=7)
    queue_box.grid(row=1, column=0, sticky="ew", pady=(0, 10))
    queue_box.columnconfigure(0, weight=1)
    entries = ttk.Treeview(queue_box, columns=("model", "suite", "tasks"),
                           show="headings", selectmode="browse", height=4)
    for column, title, width in [("model", "Model", 390), ("suite", "Suite", 160), ("tasks", "Tasks", 65)]:
        entries.heading(column, text=title)
        entries.column(column, width=width, stretch=column != "tasks")
    entries.grid(row=0, column=0, rowspan=4, sticky="nsew")
    scroll = ttk.Scrollbar(queue_box, orient="vertical", command=entries.yview)
    scroll.grid(row=0, column=1, rowspan=4, sticky="ns")
    entries.configure(yscrollcommand=scroll.set)

    def positive_entry(variable, name):
        try:
            number = int(variable.get())
            if number > 0:
                return number
        except ValueError:
            pass
        progress.set(f"Enter a positive whole number for {name}.")
        return None

    def selected_endpoint():
        value = endpoint.get().strip()
        try:
            api_url(value, "tags")
        except ValueError as error:
            progress.set(str(error))
            return None
        return value

    def refresh_plan(index=None):
        entries.delete(*entries.get_children())
        for number, (name, kind, count) in enumerate(plan):
            entries.insert("", "end", iid=str(number), values=(name, suite_names[kind], count))
        if index is not None:
            entries.selection_set(str(index))
            entries.see(str(index))
        action.configure(text="Start queue" if plan else "Start benchmark")

    def add_batch():
        count = positive_entry(tasks, "tasks per batch")
        if count is not None:
            plan.append((model.get(), suites[suite.get()], count))
            refresh_plan(len(plan) - 1)

    def edit_batch(offset=None):
        selected = entries.selection()
        if not selected:
            return
        index = int(selected[0])
        if offset is None:
            plan.pop(index)
            refresh_plan(min(index, len(plan) - 1) if plan else None)
        elif 0 <= index + offset < len(plan):
            plan.insert(index + offset, plan.pop(index))
            refresh_plan(index + offset)

    edits = []
    for row, (label, callback) in enumerate([
        ("Add batch", add_batch), ("Move up", lambda: edit_batch(-1)),
        ("Move down", lambda: edit_batch(1)), ("Remove", edit_batch),
    ]):
        button = ttk.Button(queue_box, text=label, command=callback)
        button.grid(row=row, column=2, sticky="ew", padx=(7, 0))
        edits.append(button)
    keep_button = ttk.Checkbutton(queue_box, text="Keep each model’s chat and request warm weights between batches",
                                  variable=keep_context)
    keep_button.grid(row=4, column=0, columnspan=3, sticky="w", pady=(6, 0))
    run_controls = (endpoint_entry, context_entry, tasks_entry, unlimited_button, keep_button, *edits)

    def update_model_status():
        if queue_active:
            model_status.set(batch_status or f"Queue active   {active}")
            return
        next_model = control.selected_model()
        status = f"Active   {active}"
        if next_model != active:
            status += f"   |   Next task   {next_model}"
        model_status.set(status)

    def switch_model():
        control.select_model(model.get())
        update_model_status()

    def start_run():
        nonlocal active, queue_active, batch_status
        if active is not None:
            return
        selected = selected_endpoint()
        if selected is None:
            return
        tokens = positive_entry(context, "context tokens")
        if tokens is None:
            return
        batch_plan = tuple(plan)
        rounds = None
        if not batch_plan and not unlimited.get():
            rounds = positive_entry(tasks, "tasks per run")
            if rounds is None:
                return
        queue_active = bool(batch_plan)
        batch_status = ""
        active = batch_plan[0][0] if batch_plan else model.get()
        options.model = active
        options.suite = suites[suite.get()]
        options.context = tokens
        options.endpoint = selected
        options.rounds = rounds
        suite_picker.state(["disabled"])
        for widget in run_controls:
            widget.state(["disabled"])
        control.stop.clear()
        control.select_model(active)
        update_model_status()
        overall_score.set("Overall   Waiting for results")
        current_score.set("")
        progress.set("Starting benchmark")
        if queue_active:
            action.state(["disabled"])
            picker.state(["disabled"])
        else:
            action.configure(text="Switch after task", command=switch_model)
        stop.state(["!disabled"])
        threading.Thread(target=worker, args=(batch_plan, keep_context.get()), daemon=True).start()

    def refresh_models():
        selected = selected_endpoint()
        if selected is None:
            return
        refresh.state(["disabled"])

        def fetch():
            try:
                events.put(("models", (selected, installed_models(selected))))
            except (OSError, ValueError, KeyError, TypeError) as error:
                events.put(("models_error", (selected, str(error))))

        threading.Thread(target=fetch, daemon=True).start()

    def stop_run():
        control.stop.set()
        action.state(["disabled"])
        stop.state(["disabled"])
        progress.set("Stopping benchmark")

    action = ttk.Button(toolbar, text="Start benchmark", command=start_run)
    action.grid(row=1, column=2, padx=(7, 0), pady=(7, 0))
    refresh = ttk.Button(toolbar, text="Refresh", command=refresh_models)
    refresh.grid(row=1, column=3, padx=(7, 0), pady=(7, 0))
    stop = ttk.Button(toolbar, text="Stop", command=stop_run)
    stop.grid(row=1, column=4, padx=(7, 0), pady=(7, 0))
    stop.state(["disabled"])
    ttk.Label(frame, textvariable=model_status).grid(row=2, column=0, sticky="w")
    ttk.Label(frame, textvariable=overall_score).grid(row=3, column=0, sticky="w", pady=(8, 0))
    ttk.Label(frame, textvariable=current_score).grid(row=4, column=0, sticky="w")
    ttk.Label(frame, textvariable=progress).grid(row=5, column=0, sticky="w", pady=(8, 6))
    log = ScrolledText(frame, font="TkFixedFont", wrap="word", state="disabled", height=12)
    log.grid(row=6, column=0, sticky="nsew")

    def append_line(line):
        at_bottom = log.yview()[1] >= 0.99
        log.configure(state="normal")
        log.insert("end", line + "\n")
        if int(log.index("end-1c").split(".")[0]) > 600:
            log.delete("1.0", "101.0")
        if at_bottom:
            log.see("end")
        log.configure(state="disabled")

    def worker(batch_plan, warm):
        def notify(kind, value):
            events.put((kind, value))

        try:
            if batch_plan:
                code = run_queue(options, batch_plan, warm, control, notify)
            else:
                code = run_benchmark(options, control, notify)
        except Exception as error:
            events.put(("line", f"Benchmark stopped: {error}"))
            code = 1
        events.put(("finished", code))

    def close_window():
        nonlocal closing
        if active is None:
            root.destroy()
        else:
            closing = True
            stop_run()

    def poll():
        nonlocal active, queue_active, batch_status
        for _ in range(100):
            try:
                kind, value = events.get_nowait()
            except queue.Empty:
                break
            match kind:
                case "models":
                    refresh.state(["!disabled"])
                    server, models = value
                    if server != endpoint.get().strip():
                        continue
                    picker.configure(values=models)
                    if active is None:
                        if models and model.get() not in models:
                            model.set(models[0])
                        model_status.set(f"{len(models)} models found" if models else "No models found")
                case "models_error":
                    refresh.state(["!disabled"])
                    server, error = value
                    if server != endpoint.get().strip():
                        continue
                    if active is None:
                        model_status.set("Could not reach Ollama. Check the endpoint and click Refresh.")
                    append_line(f"Could not refresh models at {server}: {error}")
                case "line":
                    append_line(value)
                case "progress":
                    if not control.stop.is_set():
                        progress.set(value)
                case "batch":
                    number, total, active, name = value
                    batch_status = f"Batch {number}/{total}   {active}   {name}"
                    update_model_status()
                    overall_score.set("Batch   Waiting for results")
                case "task":
                    active, score = value
                    update_model_status()
                    if not queue_active:
                        current_score.set(f"Current model   {score}")
                case "score":
                    total, score = value
                    overall_score.set(f"{'Batch' if queue_active else 'Overall'}   {total}")
                    current_score.set(f"Current model   {score}" if not queue_active else "")
                case "final_score":
                    overall_score.set(f"{'Batch' if queue_active else 'Overall'}   {value}")
                case "finished":
                    if closing:
                        root.destroy()
                        return
                    model_status.set(f"Last run   {active}")
                    active = None
                    queue_active = False
                    action.configure(text="Start queue" if plan else "Start benchmark", command=start_run)
                    action.state(["!disabled"])
                    suite_picker.state(["!disabled"])
                    picker.state(["!disabled"])
                    for widget in run_controls:
                        widget.state(["!disabled"])
                    stop.state(["disabled"])
                    progress.set({0: "Benchmark complete", 130: "Stopped by user"}.get(value, "Benchmark stopped"))
        root.after(100, poll)

    root.protocol("WM_DELETE_WINDOW", close_window)
    refresh_models()
    root.after(100, poll)
    root.mainloop()
    return 0
