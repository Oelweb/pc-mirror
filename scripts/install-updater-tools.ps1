# Lädt die Velopack-Pack-CLI 1.2.161 (nur Build-Werkzeug) mit geprüfter SHA-256.
# Die fertige App braucht kein .NET; Updates nutzt sie über die Rust-Crate `velopack`.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$cache = Join-Path $root '.tools'
New-Item -ItemType Directory -Force -Path $cache | Out-Null
$archive = Join-Path $cache 'vpk.1.2.161.zip'
if (!(Test-Path -LiteralPath $archive)) {
    Invoke-WebRequest -Uri 'https://github.com/velopack/velopack/releases/download/1.2.161/vpk.1.2.161.nupkg' -OutFile $archive
}
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne '2b56ce117f803fc70c103cb423bd040e395e40370f6ff6e10818e9ff9c26a323') {
    throw "Prüfsumme stimmt nicht: $archive"
}
if (!(Test-Path -LiteralPath "$cache\vpk\tools\net8.0\any\vpk.dll")) { Expand-Archive -LiteralPath $archive -DestinationPath "$cache\vpk" }
if (!(Get-Command dotnet -ErrorAction SilentlyContinue) -and !(Test-Path -LiteralPath "$cache\dotnet\dotnet.exe")) {
    Invoke-WebRequest 'https://dot.net/v1/dotnet-install.ps1' -OutFile "$cache\dotnet-install.ps1"
    & "$cache\dotnet-install.ps1" -Channel 8.0 -Runtime dotnet -InstallDir "$cache\dotnet" -NoPath
}
Write-Output 'Velopack-CLI 1.2.161 bereit.'
