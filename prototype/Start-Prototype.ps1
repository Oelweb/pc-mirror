param(
    [string]$MsysRoot = "$env:LOCALAPPDATA\PCMirrorBuild\msys64",
    [string]$DeviceName = 'PC-Mirror'
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
$receiver = Join-Path $projectRoot 'build\backend\uxplay.exe'
if (!(Test-Path -LiteralPath $receiver)) { throw 'Backend fehlt. Zuerst scripts/build-backend.sh in MSYS2 UCRT64 ausführen.' }
$env:PATH = "$MsysRoot\ucrt64\bin;$env:PATH"
$env:GST_PLUGIN_SYSTEM_PATH_1_0 = "$MsysRoot\ucrt64\lib\gstreamer-1.0"
$env:GST_PLUGIN_SCANNER_1_0 = "$MsysRoot\ucrt64\libexec\gstreamer-1.0\gst-plugin-scanner.exe"
$logDir = Join-Path $env:LOCALAPPDATA 'PC-Mirror\logs'
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
Write-Host "iPhone: Kontrollzentrum -> Bildschirmsynchronisierung -> $DeviceName"
Write-Host "Log: $logDir\prototype.log | Beenden: Strg+C"
& $receiver -n $DeviceName -nh -p 48000 -s '1920x1080@60' -fps 60 -vs d3d11videosink -as wasapisink -d 1 2>&1 | Tee-Object -FilePath "$logDir\prototype.log"
if ($LASTEXITCODE -ne 0) { throw "UxPlay wurde mit Code $LASTEXITCODE beendet. Siehe Logdatei." }
