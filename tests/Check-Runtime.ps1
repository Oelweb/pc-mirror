param([string]$PackageRoot)
$ErrorActionPreference = 'Stop'
if (!$PackageRoot) { $version = (Get-Content -Raw (Join-Path (Split-Path $PSScriptRoot -Parent) 'version.txt')).Trim(); $PackageRoot = Join-Path (Split-Path $PSScriptRoot -Parent) "dist\PC-Mirror-$version" }
$PackageRoot = (Resolve-Path -LiteralPath $PackageRoot).Path
$nativeBin = if (Test-Path -LiteralPath "$PackageRoot\uxplay.exe") { $PackageRoot } else { "$PackageRoot\bin" }
$previousEnvironment = @{}
foreach ($name in @('PATH','GST_PLUGIN_SYSTEM_PATH_1_0','GST_PLUGIN_PATH_1_0','GST_PLUGIN_SCANNER_1_0','GST_REGISTRY_1_0')) {
    $previousEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
}
# Deliberately exclude the compiler/MSYS2 paths from the loader search path.
$env:PATH = "$nativeBin;$env:SystemRoot\System32;$env:SystemRoot"
$env:GST_PLUGIN_SYSTEM_PATH_1_0 = "$PackageRoot\lib\gstreamer-1.0"
$env:GST_PLUGIN_PATH_1_0 = ''
$env:GST_PLUGIN_SCANNER_1_0 = "$PackageRoot\libexec\gstreamer-1.0\gst-plugin-scanner.exe"
$env:GST_REGISTRY_1_0 = Join-Path $env:TEMP ('pcmirror-test-' + [guid]::NewGuid().ToString('N') + '.bin')
try {
    foreach ($element in @('appsrc','decodebin','h264parse','avdec_h264','avdec_aac','avdec_alac','videoconvert','videoscale','audioconvert','audioresample','volume','level','d3d11videosink','wasapisink','videotestsrc')) {
        $output = & "$nativeBin\gst-inspect-1.0.exe" $element 2>&1
        if ($LASTEXITCODE -ne 0) { throw "Plugin $element fehlt: $output" }
        Write-Output "PASS $element"
    }
    & "$nativeBin\uxplay.exe" -v
    if ($LASTEXITCODE -ne 0) { throw 'UxPlay konnte mit den gebündelten DLLs nicht starten.' }
} finally {
    Remove-Item -LiteralPath $env:GST_REGISTRY_1_0 -ErrorAction SilentlyContinue
    foreach ($name in $previousEnvironment.Keys) { [Environment]::SetEnvironmentVariable($name, $previousEnvironment[$name], 'Process') }
}
