param([string]$MsysRoot)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$testRoot = Join-Path $root ('.tools\update-integration-' + [guid]::NewGuid().ToString('N').Substring(0,8))
$inputDir = Join-Path $testRoot 'input'
$releases = Join-Path $testRoot 'releases'
$installed = Join-Path $testRoot 'installed'
if (Test-Path -LiteralPath $installed) { throw 'Update-Test wurde bereits installiert; eindeutiges neues Testverzeichnis verwenden.' }
New-Item -ItemType Directory -Force -Path $inputDir,$releases | Out-Null
Copy-Item -LiteralPath "$root\build\app\bin\velopack-test-driver.exe","$root\build\app\bin\velopack_libc.dll" -Destination $inputDir
if (!$MsysRoot) { $MsysRoot = if (Test-Path 'C:\msys64\ucrt64\bin') { 'C:\msys64' } else { "$env:LOCALAPPDATA\PCMirrorBuild\msys64" } }
$runtimeBin = Join-Path $MsysRoot 'ucrt64\bin'
Copy-Item -LiteralPath "$runtimeBin\libstdc++-6.dll","$runtimeBin\libgcc_s_seh-1.dll","$runtimeBin\libwinpthread-1.dll" -Destination $inputDir
$dotnet = (Get-Command dotnet -ErrorAction SilentlyContinue).Source
if (!$dotnet) { $dotnet = "$root\.tools\dotnet\dotnet.exe" }
$cli = "$root\.tools\vpk\tools\net8.0\any\vpk.dll"
foreach ($version in @('0.2.0','0.2.1')) {
    Set-Content -LiteralPath "$inputDir\fixture-version.txt" -Value $version
    & $dotnet $cli pack --packId PCMirror.UpdateIntegration --packTitle 'PC-Mirror Update Test' --packVersion $version --packDir $inputDir --mainExe velopack-test-driver.exe --outputDir $releases --noPortable --delta None --shortcuts None --yes --skip-updates *> "$testRoot\pack-$version.log"
    if ($LASTEXITCODE -ne 0) { Get-Content "$testRoot\pack-$version.log" -Tail 20; throw 'Velopack-Testpaket fehlgeschlagen.' }
    if ($version -eq '0.2.0') {
        $setup = (Get-ChildItem -LiteralPath $releases -Filter '*Setup.exe' | Select-Object -First 1).FullName
        $process = Start-Process -FilePath $setup -ArgumentList @('--silent','--installto',('"'+$installed+'"')) -PassThru -WindowStyle Hidden -Wait
        if ($process.ExitCode -ne 0) { throw 'Testinstallation fehlgeschlagen.' }
    }
}
$feed = Get-Content -Raw "$releases\releases.win.json" | ConvertFrom-Json
$target = $feed.Assets | Where-Object Version -eq '0.2.1' | Select-Object -First 1
if (!$target.SHA256) { throw 'SHA-256 im echten Velopack-Feed fehlt.' }
$package = Join-Path $releases $target.FileName
$backup = $package + '.original'
Copy-Item -LiteralPath $package -Destination $backup
$bytes = [IO.File]::ReadAllBytes($package); $bytes[100] = $bytes[100] -bxor 0x01; [IO.File]::WriteAllBytes($package,$bytes)
$report = Join-Path $testRoot 'checksum-result.txt'
$driver = Join-Path $installed 'current\velopack-test-driver.exe'
$process = Start-Process -FilePath $driver -ArgumentList @('--reject-corrupt',('"'+$releases+'"'),('"'+$report+'"')) -PassThru -WindowStyle Hidden -Wait
if ($process.ExitCode -ne 0 -or !(Test-Path $report)) { throw 'Prüfung des beschädigten Pakets fehlgeschlagen.' }
$checksumResult = Get-Content -Raw $report
if ($checksumResult -notmatch 'checksum|SHA256') { throw "Unerwarteter Fehler statt Prüfsummenfehler: $checksumResult" }
Write-Output 'PASS real Velopack rejects a same-size package with corrupted SHA-256.'
Copy-Item -LiteralPath $backup -Destination $package
$report = Join-Path $testRoot 'applied-version.txt'
$process = Start-Process -FilePath $driver -ArgumentList @('--apply',('"'+$releases+'"'),('"'+$report+'"')) -PassThru -WindowStyle Hidden -Wait
if ($process.ExitCode -ne 0) { throw 'Update-Vorbereitung fehlgeschlagen.' }
$deadline = (Get-Date).AddSeconds(30)
while (!(Test-Path $report) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 250 }
if (!(Test-Path $report) -or (Get-Content -Raw $report).Trim([char]0).Trim() -ne '0.2.1') { throw 'Update und Neustart wurden nicht bestätigt.' }
if ((Get-Content -Raw "$installed\current\fixture-version.txt").Trim() -ne '0.2.1') { throw 'Installierte Dateien wurden nicht ersetzt.' }
Write-Output 'PASS real Velopack installs 0.2.1 and restarts the updated process.'
$verifiedInstall = (Resolve-Path -LiteralPath $installed).Path
if (!$verifiedInstall.StartsWith((Resolve-Path -LiteralPath $testRoot).Path + '\')) { throw 'Testinstallation liegt außerhalb des Testverzeichnisses.' }
$cleanup = Start-Process -FilePath "$verifiedInstall\Update.exe" -ArgumentList @('uninstall','--silent') -PassThru -WindowStyle Hidden -Wait
if ($cleanup.ExitCode -ne 0) { throw 'Entfernen der isolierten Testinstallation fehlgeschlagen.' }
