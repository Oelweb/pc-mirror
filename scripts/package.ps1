<#
Baut PC-Mirror.exe (Rust), stellt die Empfänger-Runtime zusammen und erzeugt
optional Installer und Update-Pakete mit Velopack.

Runtime-Quelle:
  -RuntimeFrom <Ordner>  übernimmt uxplay.exe + GStreamer aus einem fertigen, geprüften
                         Paket (Standard: build\package, wenn kein MSYS2 vorhanden ist).
  sonst                  sammelt sie aus MSYS2 (build\backend\bin\uxplay.exe via scripts/build.sh).
#>
param(
    [string]$MsysRoot = "$env:LOCALAPPDATA\PCMirrorBuild\msys64",
    [string]$RuntimeFrom,
    [string]$PackageDirectory,
    [string]$ReleaseDirectory,
    [switch]$EnableDeltas,
    [switch]$SkipInstaller
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
$version = (Get-Content -Raw -LiteralPath "$projectRoot\version.txt").Trim()
if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'version.txt muss eine stabile SemVer-Version enthalten, z. B. 0.3.0.' }
$output = if ($PackageDirectory) { [IO.Path]::GetFullPath($PackageDirectory) } else { Join-Path $projectRoot "dist\PC-Mirror-$version" }
$cargo = (Get-Command cargo -ErrorAction SilentlyContinue).Source
if (!$cargo) { $cargo = "$env:USERPROFILE\.cargo\bin\cargo.exe" }

& $cargo build --release -p pcmirror --manifest-path "$projectRoot\Cargo.toml"
if ($LASTEXITCODE -ne 0) { throw 'Rust-Build fehlgeschlagen.' }

# Saubere Zielstruktur; eine übernommene Runtime darf nicht im Ziel selbst liegen.
if ($RuntimeFrom) { $RuntimeFrom = [IO.Path]::GetFullPath($RuntimeFrom) }
if (!$RuntimeFrom -and !(Test-Path -LiteralPath "$MsysRoot\ucrt64\bin")) {
    $RuntimeFrom = Join-Path $projectRoot 'build\package-runtime'
    if (!(Test-Path -LiteralPath "$RuntimeFrom\uxplay.exe")) { throw "Weder MSYS2 ($MsysRoot) noch eine Runtime-Quelle gefunden. -RuntimeFrom angeben." }
}
if ($RuntimeFrom -and $RuntimeFrom.TrimEnd('\') -ieq $output.TrimEnd('\')) { throw 'RuntimeFrom und Paketordner müssen verschieden sein.' }
if (Test-Path -LiteralPath $output) { Remove-Item -LiteralPath $output -Recurse -Force }
New-Item -ItemType Directory -Force -Path "$output\docs", "$output\licenses" | Out-Null

if ($RuntimeFrom) {
    python "$PSScriptRoot\reuse_runtime.py" --source $RuntimeFrom --destination $output
} else {
    python "$PSScriptRoot\collect_runtime.py" --prefix "$MsysRoot\ucrt64" --receiver "$projectRoot\build\backend\bin\uxplay.exe" --destination $output
}
if ($LASTEXITCODE -ne 0) { throw 'Runtime konnte nicht vollständig gesammelt werden.' }

Copy-Item -LiteralPath "$projectRoot\target\release\PC-Mirror.exe" -Destination $output
Copy-Item -LiteralPath "$projectRoot\README.md", "$projectRoot\LICENSE" -Destination $output
Copy-Item -Path "$projectRoot\docs\*.md" -Destination "$output\docs"
Copy-Item -LiteralPath "$projectRoot\third_party\uxplay\LICENSE" -Destination "$output\licenses\UxPlay-GPL-3.0.txt"
Copy-Item -LiteralPath "$projectRoot\third_party\uxplay\lib\llhttp\LICENSE-MIT" -Destination "$output\licenses\llhttp-MIT.txt"
python "$PSScriptRoot\rust_licenses.py" --output "$output\licenses\Rust-Crates.txt"
if ($LASTEXITCODE -ne 0) { throw 'Lizenzhinweise der Rust-Abhängigkeiten konnten nicht erstellt werden.' }

python "$PSScriptRoot\source_archive.py"
if ($LASTEXITCODE -ne 0) { throw 'Quellcodearchiv konnte nicht erstellt werden.' }

if (!$SkipInstaller) {
    & "$PSScriptRoot\install-updater-tools.ps1"
    $dotnet = (Get-Command dotnet -ErrorAction SilentlyContinue).Source
    if (!$dotnet) { $dotnet = "$projectRoot\.tools\dotnet\dotnet.exe" }
    $releases = if ($ReleaseDirectory) { [IO.Path]::GetFullPath($ReleaseDirectory) } else { Join-Path $projectRoot 'dist\releases' }
    $deltaMode = if ($EnableDeltas) { 'BestSpeed' } else { 'None' }
    New-Item -ItemType Directory -Force -Path $releases | Out-Null
    & $dotnet "$projectRoot\.tools\vpk\tools\net8.0\any\vpk.dll" pack --packId PCMirror --packTitle PC-Mirror --packVersion $version --packDir $output --mainExe PC-Mirror.exe --runtime win-x64 --channel win --delta $deltaMode --noPortable --icon "$projectRoot\assets\app.ico" --outputDir $releases --yes --skip-updates
    if ($LASTEXITCODE -ne 0) { throw 'Velopack-Installer konnte nicht erstellt werden.' }
    Copy-Item -LiteralPath "$projectRoot\dist\PC-Mirror-$version-source.zip" -Destination $releases
    @("App/Backend source: https://pcmirror.oel.best/PC-Mirror-$version-source.zip",
      'Unchanged runtime sources: https://pcmirror.oel.best/PC-Mirror-0.2.1-runtime-sources.zip',
      'License and build instructions are included in the source archive.') | Set-Content -LiteralPath "$releases\SOURCES.txt" -Encoding utf8
}
Write-Output "App: $output\PC-Mirror.exe"
