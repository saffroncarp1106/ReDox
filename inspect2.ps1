$ErrorActionPreference = "Stop"
$root = "C:\Users\llkin\Documents\ReDoxWorkingDirectory\Redox terminal update v0.2.3\ReDoxCodex-main\ReDoxCodex-main"
Write-Output "=== files in src/ui ==="
Get-ChildItem (Join-Path $root "src\ui") -Filter *.rs | ForEach-Object { $_.Name } | Sort-Object
Write-Output "=== files in src/ui/assets ==="
$assets = Join-Path $root "src\ui\assets"
if (Test-Path $assets) { Get-ChildItem $assets | ForEach-Object { $_.FullName } | Sort-Object } else { "NO assets dir" }
Write-Output "=== icons.rs location ==="
$icons = Get-ChildItem -Recurse -Filter icons.rs (Join-Path $root "src")
foreach ($f in $icons) { $f.FullName }
