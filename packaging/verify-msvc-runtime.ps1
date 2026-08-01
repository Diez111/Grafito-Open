param(
    [Parameter(Mandatory = $true)]
    [string]$ExePath
)

$ErrorActionPreference = "Stop"

$rustFlags = $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS
if ($rustFlags -notmatch '(?:^|\s)-C\s+target-feature=\+crt-static(?:\s|$)') {
    throw "MSVC release build is missing -C target-feature=+crt-static"
}

$targetCfg = & rustc --print cfg --target x86_64-pc-windows-msvc -C target-feature=+crt-static
if ($LASTEXITCODE -ne 0 -or $targetCfg -notcontains 'target_feature="crt-static"') {
    throw "rustc did not enable the crt-static target feature"
}

$exe = (Resolve-Path $ExePath).Path
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path $vswhere)) {
    throw "vswhere.exe was not found on the Windows runner"
}

$installationPath = (& $vswhere -latest -products * `
    -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
    -property installationPath | Select-Object -First 1)
if (-not $installationPath) {
    throw "Visual Studio C++ tools were not found"
}

$toolsRoot = Join-Path $installationPath "VC\Tools\MSVC"
$dumpbin = Get-ChildItem -Path $toolsRoot -Filter dumpbin.exe -File -Recurse |
    Where-Object { $_.FullName -match '\\bin\\Hostx64\\x64\\dumpbin\.exe$' } |
    Sort-Object FullName -Descending |
    Select-Object -First 1
if ($null -eq $dumpbin) {
    throw "dumpbin.exe for Hostx64/x64 was not found"
}

$dependencies = & $dumpbin.FullName /nologo /dependents $exe | Out-String
if ($LASTEXITCODE -ne 0) {
    throw "dumpbin failed while inspecting $exe"
}

$dynamicCrtPattern = '(?im)^\s*(?:api-ms-win-crt-[^\s]+|concrt[^\s]*|msvcp[^\s]*|vcruntime[^\s]*|ucrtbase)\.dll\s*$'
$dynamicCrtImports = [regex]::Matches($dependencies, $dynamicCrtPattern) |
    ForEach-Object { $_.Value.Trim() } |
    Sort-Object -Unique
if ($dynamicCrtImports) {
    throw "MSVC executable imports the dynamic CRT: $($dynamicCrtImports -join ', ')"
}

Write-Host "MSVC CRT verification passed: crt-static enabled and no dynamic CRT imports found."
