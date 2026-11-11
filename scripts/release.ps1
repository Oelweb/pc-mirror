<#
Neue Version veröffentlichen: version.txt erhöhen, dann dieses Skript starten.
Baut und testet, paketiert, signiert den Update-Feed und lädt alles auf den Server.

  -NoPublish     nur bauen und signieren, nichts hochladen
  -RuntimeFrom   Empfänger-Runtime aus einem fertigen Paket übernehmen (ohne MSYS2)
#>
param(
    [string]$MsysRoot = "$env:LOCALAPPDATA\PCMirrorBuild\msys64",
    [string]$RuntimeFrom,
    [switch]$NoPublish
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
Push-Location $projectRoot
try {
    $cargo = (Get-Command cargo -ErrorAction SilentlyContinue).Source
    if (!$cargo) { $cargo = "$env:USERPROFILE\.cargo\bin\cargo.exe" }
    $key = Join-Path $projectRoot '.tools\signing\update-signing.key'
    if (!(Test-Path -LiteralPath $key)) { throw "Privater Signaturschlüssel fehlt: $key (aus der Sicherung wiederherstellen)" }

    & $cargo test --workspace --release
    if ($LASTEXITCODE -ne 0) { throw 'Rust-Tests fehlgeschlagen' }

    if (!$RuntimeFrom -and (Test-Path -LiteralPath "$MsysRoot\usr\bin\bash.exe")) {
        $env:CHERE_INVOKING = '1'; $env:MSYSTEM = 'UCRT64'
        & "$MsysRoot\usr\bin\bash.exe" -lc 'bash scripts/build.sh'
        if ($LASTEXITCODE -ne 0) { throw 'Empfänger-Build oder -Tests fehlgeschlagen' }
    }

    $releases = "$projectRoot\dist\server-release"
    $package = "$projectRoot\build\package"
    $arguments = @{ MsysRoot = $MsysRoot; PackageDirectory = $package; ReleaseDirectory = $releases; EnableDeltas = $true }
    if ($RuntimeFrom) { $arguments.RuntimeFrom = $RuntimeFrom }
    & "$PSScriptRoot\package.ps1" @arguments
    if ($LASTEXITCODE -ne 0) { throw 'Paketierung fehlgeschlagen' }

    & "$projectRoot\tests\Check-Runtime.ps1" -PackageRoot $package
    python "$projectRoot\tests\backend_integration.py" --package $package
    if ($LASTEXITCODE -ne 0) { throw 'Empfänger-Smoke-Test fehlgeschlagen' }
    # Ende-zu-Ende ohne iPhone: nachgebauter Sender gegen den Rust-Empfänger
    & $cargo build --release -p pcmirror-receiver --examples
    if ($LASTEXITCODE -ne 0) { throw 'Testsender konnte nicht gebaut werden' }
    foreach ($case in @(@('--codec', '264'), @('--codec', '265'), @('--codec', '264', '--mode', '3'), @('--codec', '264', '--pin', '4711'))) {
        python "$projectRoot\tests\airplay_e2e.py" --package $package @case
        if ($LASTEXITCODE -ne 0) { throw "Ende-zu-Ende-Test fehlgeschlagen: $case" }
    }
    python "$projectRoot\tests\receiver_fuzz.py" --package $package
    if ($LASTEXITCODE -ne 0) { throw 'Robustheitstest fehlgeschlagen' }
    python "$projectRoot\tests\fallback_test.py" --package $package
    if ($LASTEXITCODE -ne 0) { throw 'Rückfalltest fehlgeschlagen' }
    python "$projectRoot\tests\verify_package.py" --package $package
    if ($LASTEXITCODE -ne 0) { throw 'Paketprüfung fehlgeschlagen' }

    # Feed signieren; die App akzeptiert nur Feeds mit gültiger Signatur.
    $feed = "$releases\releases.win.json"
    & $cargo run -q --release -p pcmirror-sign -- sign $key $feed
    if ($LASTEXITCODE -ne 0) { throw 'Feed konnte nicht signiert werden' }
    & $cargo run -q --release -p pcmirror-sign -- verify "$projectRoot\packaging\update-signing.pub" $feed
    if ($LASTEXITCODE -ne 0) { throw 'Feed-Signatur passt nicht zum eingebauten öffentlichen Schlüssel' }

    if (!$NoPublish) {
        python "$PSScriptRoot\publish-server.py" --release-directory $releases
        if ($LASTEXITCODE -ne 0) { throw 'Veröffentlichung fehlgeschlagen' }
    }
} finally { Pop-Location }
