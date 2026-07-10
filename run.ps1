#requires -version 5
# Build (release) and launch the AlphaGeometry Studio web app.
#   .\run.ps1            # serve on port 8787
#   .\run.ps1 9000       # serve on port 9000
$ErrorActionPreference = 'Stop'
Set-Location -Path $PSScriptRoot

$port = if ($args.Count -ge 1) { [int]$args[0] } else { 8787 }

Write-Host "Building AlphaGeometry Studio (release)..." -ForegroundColor Cyan
cargo build --release -p ag-studio
if ($LASTEXITCODE -ne 0) { Write-Host "Build failed." -ForegroundColor Red; exit 1 }

$exe = Join-Path $PSScriptRoot 'target\release\agstudio.exe'
$url = "http://127.0.0.1:$port"
Write-Host "Opening $url" -ForegroundColor Green
Start-Process $url
& $exe serve --port $port
