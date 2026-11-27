$ErrorActionPreference = "Stop"
$root = "C:\Users\llkin\Documents\ReDoxWorkingDirectory\Redox terminal update v0.2.3\ReDoxCodex-main\ReDoxCodex-main"
Write-Output "=== fn defs in src ==="
Get-ChildItem -Recurse -Filter *.rs (Join-Path $root "src") | Select-String -Pattern '\bfn (composer|prompt|prompt_area|clear_chat_buttons|chat_transcript|focus_prompt|focus_win98_prompt)\b' | ForEach-Object { $_.Path + ':' + $_.LineNumber }
Write-Output "=== icon svg names ==="
Get-ChildItem (Join-Path $root "src\ui\assets") -Filter *.svg | ForEach-Object { $_.Name } | Sort-Object
Write-Output "=== change-related tokens in tests ==="
Get-ChildItem -Recurse -Filter *.rs (Join-Path $root "tests") | Select-String -Pattern 'terminal|Terminal|code change|ChangeData|Change\b' | ForEach-Object { $_.Path + ':' + $_.LineNumber }
