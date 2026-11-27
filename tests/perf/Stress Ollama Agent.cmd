@echo off
setlocal
pushd "%~dp0..\.." || exit /b 1
title ReDox Ollama Agent Stress Test
set "bench_gui="
if "%~1"=="" set "bench_gui=--gui"

cargo build --locked --example ollama-bench-runner --bin ollama-qualifier
if errorlevel 1 goto done

where py >nul 2>nul
if errorlevel 1 (
    python -B "%~dp0ollama_bench.py" %bench_gui% %*
) else (
    py -3 -B "%~dp0ollama_bench.py" %bench_gui% %*
)

:done
set "bench_exit=%errorlevel%"
popd
if not "%bench_exit%"=="0" if "%~1"=="" pause
exit /b %bench_exit%
