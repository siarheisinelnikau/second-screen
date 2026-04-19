param([ValidateSet('Debug','Release')][string]$Configuration = 'Release', [string]$SdkVersion = '10.0.26100.0', [switch]$CheckOnly)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $repo
try {
    $kit = 'C:/Program Files (x86)/Windows Kits/10'
    $missing = @()
    foreach ($relative in @("Include/$SdkVersion/um/iddcx/1.4/IddCx.h", 'Include/wdf/umdf/2.25/wdf.h', "build/$SdkVersion/WindowsDriver.common.targets")) {
        if (-not (Test-Path -LiteralPath (Join-Path $kit $relative))) { $missing += (Join-Path $kit $relative) }
    }
    if ($missing.Count -gt 0) { throw ("WDK prerequisites are missing. No signing, installation or system changes attempted.`n" + ($missing -join "`n")) }
    $locator = 'C:/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe'
    $msbuild = (& $locator -latest -version '[17.0,18.0)' -products '*' -requires Microsoft.Component.MSBuild -find 'MSBuild/**/Bin/MSBuild.exe' | Select-Object -First 1)
    if (-not $msbuild) { throw 'MSBuild is unavailable. Nothing will be installed.' }
    if ($CheckOnly) { Write-Output 'WDK prerequisites found; no build or deployment was performed.'; return }
    & $msbuild 'driver/SecondScreenDisplay.vcxproj' /t:Build /m:2 "/p:Configuration=$Configuration" /p:Platform=x64 "/p:WindowsTargetPlatformVersion=$SdkVersion" /p:SignMode=Off /p:EnableInf2Cat=false
    if ($LASTEXITCODE -ne 0) { throw 'WDK build failed; the driver was not installed or signed' }
    Write-Output "Unsigned driver output: $repo/target/driver/$Configuration"
} finally { Pop-Location }
