param([string]$MsysRoot = "$env:LOCALAPPDATA\PCMirrorBuild\msys64")
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
$output = Join-Path $projectRoot 'dist\PC-Mirror-Prototype'
New-Item -ItemType Directory -Force -Path $output | Out-Null
python "$PSScriptRoot\collect_runtime.py" --prefix "$MsysRoot\ucrt64" --receiver "$projectRoot\build\backend\uxplay.exe" --destination $output
if ($LASTEXITCODE -ne 0) { throw 'Runtime konnte nicht vollständig gesammelt werden.' }
Copy-Item -LiteralPath "$projectRoot\prototype\Start.ps1","$projectRoot\prototype\Start.cmd","$projectRoot\prototype\receiver.conf","$projectRoot\docs\IPHONE-TEST.md" -Destination $output
Copy-Item -LiteralPath "$projectRoot\scripts\Configure-Firewall.ps1" -Destination $output
New-Item -ItemType Directory -Force -Path "$output\licenses" | Out-Null
Copy-Item -LiteralPath "$projectRoot\third_party\uxplay\LICENSE" -Destination "$output\licenses\UxPlay-GPL-3.0.txt"
Copy-Item -LiteralPath "$projectRoot\third_party\uxplay\lib\llhttp\LICENSE-MIT" -Destination "$output\licenses\llhttp-MIT.txt"
Write-Output "Prototyp: $output\Start.cmd"
