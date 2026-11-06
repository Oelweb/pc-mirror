param([string]$MsysRoot = "$env:LOCALAPPDATA\PCMirrorBuild\msys64", [switch]$NoPublish)
$ErrorActionPreference='Stop'
$projectRoot=Split-Path $PSScriptRoot -Parent
Push-Location $projectRoot
try {
    $env:CHERE_INVOKING='1';$env:MSYSTEM='UCRT64'
    & "$MsysRoot\usr\bin\bash.exe" -lc 'bash scripts/build.sh && ctest --test-dir build/app --output-on-failure'
    if($LASTEXITCODE -ne 0){throw 'Build or tests failed'}
    $releases="$projectRoot\dist\server-release"
    & "$PSScriptRoot\package.ps1" -MsysRoot $MsysRoot -PackageDirectory "$projectRoot\build\package" -ReleaseDirectory $releases -EnableDeltas
    if($LASTEXITCODE -ne 0){throw 'Packaging failed'}
    python "$projectRoot\tests\backend_integration.py" --package "$projectRoot\build\package"
    if($LASTEXITCODE -ne 0){throw 'Backend smoke test failed'}
    if(!$NoPublish){
        python "$PSScriptRoot\publish-server.py" --release-directory $releases
        if($LASTEXITCODE -ne 0){throw 'Publishing failed'}
    }
} finally {Pop-Location}
