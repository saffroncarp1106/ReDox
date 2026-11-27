# SaffyOS Python runtime

Locked SaffyOS runs Python 3.12 scripts in WebAssembly through Wasmtime. The guest receives a temporary workspace containing the script and its listed inputs. ReDox disables network access and environment inheritance, and the guest has no host shell.

## Install on 64-bit Windows

1. Download the [Wasmtime 51.0.0 Windows x64 ZIP](https://github.com/bytecodealliance/wasmtime/releases/download/v51.0.0/wasmtime-v51.0.0-x86_64-windows.zip). Use `Get-FileHash` to verify SHA-256 `D2F5853A1CDED9B1256B45CD57686B6855E69049EF07FD4AD78FEB78DFA3AADA`. Extract `wasmtime.exe`.
2. Download the standalone [Python 3.12.0 WASI module](https://github.com/vmware-labs/webassembly-language-runtimes/releases/download/python/3.12.0%2B20231211-040d5a6/python-3.12.0.wasm). Verify SHA-256 `E5DC5A398B07B54EA8FDB503BF68FB583D533F10EC3F930963E02B9505F7A763`. Rename the file to `python.wasm`.
3. Place both files in `%LOCALAPPDATA%\ReDox\saffyos-runtime`, outside the folder selected for locked SaffyOS.

The operation uses the guest's Python standard library. Native Python extensions, host executables, and package downloads are unavailable. For a script that opens a relative path, provide that input path relative to the workspace root. Include imported local Python modules in the inputs too. Total staged input is limited to 32 MiB.

For example, to verify Ainsley's adventure demo, use `run_python` with `{"path":"game_engine.py","inputs":["game_state.json"]}`. The demo can write a save within the sandbox. Its original workspace save stays intact.
