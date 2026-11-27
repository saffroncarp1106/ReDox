try {
    $listeners = @(Get-NetTCPConnection -State Listen | Where-Object LocalPort -eq 11434)
    $session = (Get-Process -Id $PID).SessionId
    $cli = @(foreach ($directory in @(
        (Join-Path $env:LOCALAPPDATA 'Programs/Ollama'),
        (Join-Path $env:ProgramFiles 'Ollama')
    )) {
        $candidate = Join-Path $directory 'ollama.exe'
        if (Test-Path -LiteralPath $candidate -PathType Leaf) { $candidate }
    }) | Select-Object -First 1

    $servers = @($listeners | Select-Object -ExpandProperty OwningProcess -Unique | ForEach-Object {
        $process = Get-Process -Id $_ -ErrorAction SilentlyContinue
        if (-not $process -or $process.SessionId -ne $session -or
            [IO.Path]::GetFileName($process.Path) -ine 'ollama.exe') {
            throw 'Port 11434 belongs to a process ReDox cannot restart.'
        }
        $process
    })
    if ($servers.Count -gt 0) {
        $cli = $servers[0].Path
        if (@($servers | Where-Object Path -ine $cli).Count -gt 0) {
            throw 'Multiple Ollama installations are using port 11434.'
        }
    }
    if (-not $cli) {
        $cli = (Get-Command ollama.exe -CommandType Application -ErrorAction SilentlyContinue |
            Select-Object -First 1).Source
    }
    if (-not $cli) { throw 'Ollama executable was not found.' }

    $desktop = Join-Path (Split-Path -Path $cli -Parent) 'ollama app.exe'
    $tray = @(Get-Process -Name 'ollama app' -ErrorAction SilentlyContinue | Where-Object {
        $_.SessionId -eq $session -and $_.Path -ieq $desktop
    })
    # Stop the manager first so it cannot relaunch its server during the restart.
    if ($tray.Count -gt 0) {
        Stop-Process -InputObject $tray -Force
        $tray | Wait-Process -Timeout 5 -ErrorAction SilentlyContinue
    }
    $taskkill = Join-Path $env:SystemRoot 'System32/taskkill.exe'
    foreach ($server in $servers) {
        $server.Refresh()
        if (-not $server.HasExited) {
            # Stop child model runners together with their Ollama server.
            & $taskkill /PID $server.Id /T /F | Out-Null
            $server | Wait-Process -Timeout 5 -ErrorAction SilentlyContinue
            $server.Refresh()
            if (-not $server.HasExited) { throw 'Ollama server did not stop.' }
        }
    }

    if (Test-Path -LiteralPath $desktop -PathType Leaf) {
        Start-Process -FilePath $desktop -WindowStyle Hidden
    } else {
        Start-Process -FilePath $cli -ArgumentList 'serve' -WindowStyle Hidden
    }
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}
