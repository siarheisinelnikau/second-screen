param([string]$OutputDirectory = '.test-output/ui-preview')
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $repo
try {
    & cargo build --release --locked --offline --example ui_preview --features virtual-display --target x86_64-pc-windows-msvc -j 2 --config 'target.x86_64-pc-windows-msvc.rustflags=["-C","target-feature=+crt-static"]'
    if ($LASTEXITCODE -ne 0) { throw 'Offscreen renderer compilation failed' }
    $output = [IO.Path]::GetFullPath((Join-Path $repo $OutputDirectory))
    if (-not $output.StartsWith(([IO.Path]::GetFullPath($repo) + [IO.Path]::DirectorySeparatorChar), [StringComparison]::OrdinalIgnoreCase)) { throw 'Preview output must be inside this repository' }
    New-Item -ItemType Directory -Force -Path $output | Out-Null
    $executable = Join-Path $repo 'target/x86_64-pc-windows-msvc/release/examples/ui_preview.exe'
    $process = Start-Process -FilePath $executable -ArgumentList ('"' + $output + '"') -WorkingDirectory $repo -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $output 'render.log') -RedirectStandardError (Join-Path $output 'render-error.log')
    if (-not $process.WaitForExit(30000)) { $process.Kill(); $process.WaitForExit(); throw 'Only the disposable offscreen renderer was terminated at its 30-second deadline' }
    if ($process.ExitCode -ne 0) { throw "Offscreen renderer failed: see $output/render-error.log" }
    Get-Content -LiteralPath (Join-Path $output 'render.log')
} finally { Pop-Location }
