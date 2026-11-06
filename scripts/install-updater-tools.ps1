param([switch]$SkipCli)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$cache = Join-Path $root '.tools'
New-Item -ItemType Directory -Force -Path $cache | Out-Null
function Get-VerifiedArchive([string]$Url, [string]$Path, [string]$Sha256) {
    if (!(Test-Path -LiteralPath $Path)) { Invoke-WebRequest -Uri $Url -OutFile $Path }
    if ((Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -ne $Sha256) { throw "Prüfsumme stimmt nicht: $Path" }
}
if (!(Test-Path -LiteralPath "$root\third_party\velopack-sdk\include\Velopack.hpp")) {
    $archive = Join-Path $cache 'velopack-sdk.zip'
    Get-VerifiedArchive 'https://github.com/velopack/velopack/releases/download/1.2.161/velopack_libc_1.2.161.zip' $archive '31de8f2cf05a47bd5328a9f98b0c41aec1582156f6075f1058b26941f7f17c20'
    Expand-Archive -LiteralPath $archive -DestinationPath "$root\third_party\velopack-sdk" -Force
}
if (!$SkipCli) {
    $archive = Join-Path $cache 'vpk.1.2.161.zip'
    Get-VerifiedArchive 'https://github.com/velopack/velopack/releases/download/1.2.161/vpk.1.2.161.nupkg' $archive '2b56ce117f803fc70c103cb423bd040e395e40370f6ff6e10818e9ff9c26a323'
    if (!(Test-Path -LiteralPath "$cache\vpk\tools\net8.0\any\vpk.dll")) { Expand-Archive -LiteralPath $archive -DestinationPath "$cache\vpk" }
    if (!(Get-Command dotnet -ErrorAction SilentlyContinue) -and !(Test-Path -LiteralPath "$cache\dotnet\dotnet.exe")) {
        Invoke-WebRequest 'https://dot.net/v1/dotnet-install.ps1' -OutFile "$cache\dotnet-install.ps1"
        & "$cache\dotnet-install.ps1" -Channel 8.0 -Runtime dotnet -InstallDir "$cache\dotnet" -NoPath
    }
}
Write-Output 'Velopack SDK/CLI 1.2.161 bereit.'
