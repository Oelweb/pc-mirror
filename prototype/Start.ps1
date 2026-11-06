$ErrorActionPreference = 'Stop'
$env:PATH = "$PSScriptRoot\bin;$env:SystemRoot\System32;$env:SystemRoot"
$env:GST_PLUGIN_SYSTEM_PATH_1_0 = "$PSScriptRoot\lib\gstreamer-1.0"
$env:GST_PLUGIN_PATH_1_0 = ''
$env:GST_PLUGIN_SCANNER_1_0 = "$PSScriptRoot\libexec\gstreamer-1.0\gst-plugin-scanner.exe"
$logDir = Join-Path $env:LOCALAPPDATA 'PC-Mirror\logs'
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$env:GST_REGISTRY_1_0 = Join-Path $logDir 'prototype-gst-registry.bin'
$config = Join-Path $PSScriptRoot 'receiver.conf'
Write-Host 'PC-Mirror Prototyp – am iPhone: Kontrollzentrum > Bildschirmsynchronisierung > PC-Mirror'
Write-Host "Log: $logDir\prototype.log | Beenden: Strg+C"
& "$PSScriptRoot\bin\uxplay.exe" -rc $config -n 'PC-Mirror' -nh -p 48000 -s '1920x1080@60' -fps 60 -vs d3d11videosink -as wasapisink -d 1 2>&1 | Tee-Object -FilePath "$logDir\prototype.log"
if ($LASTEXITCODE -ne 0) { throw "Empfänger beendet, Fehlercode $LASTEXITCODE. Details in $logDir\prototype.log" }
