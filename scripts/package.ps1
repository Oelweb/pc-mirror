param(
    [string]$MsysRoot = "$env:LOCALAPPDATA\PCMirrorBuild\msys64",
    [string]$PackageDirectory,
    [string]$ReleaseDirectory,
    [switch]$EnableDeltas,
    [switch]$SkipInstaller
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
$version = (Get-Content -Raw -LiteralPath "$projectRoot\version.txt").Trim()
if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'version.txt muss eine stabile SemVer-Version enthalten, z. B. 0.2.0.' }
$output = if ($PackageDirectory) { [IO.Path]::GetFullPath($PackageDirectory) } else { Join-Path $projectRoot "dist\PC-Mirror-$version" }
New-Item -ItemType Directory -Force -Path "$output\scripts","$output\docs","$output\licenses" | Out-Null
python "$PSScriptRoot\collect_runtime.py" --prefix "$MsysRoot\ucrt64" --receiver "$projectRoot\build\app\bin\uxplay.exe" --app "$projectRoot\build\app\bin\PC-Mirror.exe" --destination $output --flat
if ($LASTEXITCODE -ne 0) { throw 'Runtime konnte nicht vollständig gesammelt werden.' }
Copy-Item -LiteralPath "$PSScriptRoot\Configure-Firewall.ps1" -Destination "$output\scripts"
Copy-Item -LiteralPath "$projectRoot\README.md","$projectRoot\LICENSE" -Destination $output
Copy-Item -Path "$projectRoot\docs\*.md" -Destination "$output\docs"
Copy-Item -LiteralPath "$projectRoot\third_party\uxplay\LICENSE" -Destination "$output\licenses\UxPlay-GPL-3.0.txt"
Copy-Item -LiteralPath "$projectRoot\third_party\uxplay\lib\llhttp\LICENSE-MIT" -Destination "$output\licenses\llhttp-MIT.txt"
Copy-Item -LiteralPath "$projectRoot\third_party\velopack\LICENSE" -Destination "$output\licenses\Velopack-MIT.txt"
Copy-Item -LiteralPath "$projectRoot\third_party\velopack\vendor\THIRD_PARTY_NOTICES.md" -Destination "$output\licenses\Velopack-Third-Party-Notices.md"
python "$PSScriptRoot\source_archive.py"
if ($LASTEXITCODE -ne 0) { throw 'Quellcodearchiv konnte nicht erstellt werden.' }
if (!$SkipInstaller) {
    & "$PSScriptRoot\install-updater-tools.ps1"
    $dotnet = (Get-Command dotnet -ErrorAction SilentlyContinue).Source
    if (!$dotnet) { $dotnet = "$projectRoot\.tools\dotnet\dotnet.exe" }
    $releases = if($ReleaseDirectory){[IO.Path]::GetFullPath($ReleaseDirectory)}else{Join-Path $projectRoot 'dist\releases'}
    $deltaMode = if($EnableDeltas){'BestSpeed'}else{'None'}
    New-Item -ItemType Directory -Force -Path $releases | Out-Null
    & $dotnet "$projectRoot\.tools\vpk\tools\net8.0\any\vpk.dll" pack --packId PCMirror --packTitle PC-Mirror --packVersion $version --packDir $output --mainExe PC-Mirror.exe --runtime win-x64 --channel win --delta $deltaMode --noPortable --icon "$projectRoot\assets\app.ico" --outputDir $releases --yes --skip-updates
    if ($LASTEXITCODE -ne 0) { throw 'Velopack-Installer konnte nicht erstellt werden.' }
    Copy-Item -LiteralPath "$projectRoot\dist\PC-Mirror-$version-source.zip" -Destination $releases
    if($EnableDeltas){
        @("App/Backend source: https://pcmirror.oel.best/PC-Mirror-$version-source.zip",'Unchanged runtime sources: https://pcmirror.oel.best/PC-Mirror-0.2.1-runtime-sources.zip','License and build instructions are included in the source archive.') | Set-Content -LiteralPath "$releases\SOURCES.txt" -Encoding utf8
    }
}
Write-Output "App: $output\PC-Mirror.exe"
