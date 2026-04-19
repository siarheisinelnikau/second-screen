param([string]$SdkVersion = '10.0.26100.0')
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $repo
$savedPath = $env:PATH
try {
    $locator = 'C:/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe'
    $vs = (& $locator -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath | Select-Object -First 1)
    if (-not $vs) { throw 'An existing MSVC x64 toolchain is required. Nothing will be installed.' }
    $version = (Get-Content -LiteralPath (Join-Path $vs 'VC/Auxiliary/Build/Microsoft.VCToolsVersion.default.txt')).Trim()
    $vc = Join-Path $vs "VC/Tools/MSVC/$version"
    $sdk = 'C:/Program Files (x86)/Windows Kits/10'
    $compiler = Join-Path $vc 'bin/Hostx64/x64/cl.exe'
    $env:PATH = "$(Split-Path -Parent $compiler);$savedPath"
    New-Item -ItemType Directory -Force -Path '.test-output/driver-core' | Out-Null
    $includes = @("/I$vc/include", "/I$sdk/Include/$SdkVersion/ucrt", "/I$sdk/Include/$SdkVersion/shared", "/I$sdk/Include/$SdkVersion/um", "/I$sdk/Include/$SdkVersion/winrt")
    & $compiler /nologo /std:c++17 /EHsc /W4 /WX /MT /O2 /DNOMINMAX @includes /c driver/GpuReadback.cpp '/Fo.test-output/driver-core/GpuReadback.obj'
    if ($LASTEXITCODE -ne 0) { throw 'SDK-only GPU readback source compilation failed' }
    & $compiler /nologo /std:c++17 /EHsc /W4 /WX /MT /O2 /DNOMINMAX @includes driver/tests/CoreTests.cpp '/Fo.test-output/driver-core/CoreTests.obj' '/Fe.test-output/driver-core/CoreTests.exe' /link "/LIBPATH:$vc/lib/x64" "/LIBPATH:$sdk/Lib/$SdkVersion/ucrt/x64" "/LIBPATH:$sdk/Lib/$SdkVersion/um/x64"
    if ($LASTEXITCODE -ne 0) { throw 'Driver core tests compilation failed' }
    # This console fixture only copies generated pixels. It does not initialize D3D or open a driver.
    & './.test-output/driver-core/CoreTests.exe' '.test-output/driver-frame.bin'
    if ($LASTEXITCODE -ne 0) { throw 'Driver core tests failed' }
} finally { $env:PATH = $savedPath; Pop-Location }
