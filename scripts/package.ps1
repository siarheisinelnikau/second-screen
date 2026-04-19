param([switch]$Offline, [switch]$VirtualDisplay)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $repo
try {
    $metadataArgs = @('metadata', '--no-deps', '--format-version', '1', '--locked')
    if ($Offline) { $metadataArgs += '--offline' }
    $metadataText = & cargo @metadataArgs
    if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed' }
    $metadata = $metadataText | ConvertFrom-Json
    $version = ($metadata.packages | Where-Object name -eq 'second-screen').version
    $buildArgs = @('build', '--release', '--locked', '--target', 'x86_64-pc-windows-msvc', '-j', '2', '--config', 'target.x86_64-pc-windows-msvc.rustflags=["-C","target-feature=+crt-static"]')
    if ($Offline) { $buildArgs += '--offline' }
    if ($VirtualDisplay) { $buildArgs += @('--features', 'virtual-display') }
    & cargo @buildArgs
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    $dist = Join-Path $repo 'dist'
    New-Item -ItemType Directory -Force -Path $dist | Out-Null
    $name = "SecondScreen-v$version.exe"
    $source = Join-Path $repo 'target/x86_64-pc-windows-msvc/release/second-screen.exe'
    $asset = Join-Path $dist $name
    Copy-Item -LiteralPath $source -Destination $asset
    $hash = (Get-FileHash -LiteralPath $asset -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText((Join-Path $dist 'SHA256SUMS'), "$hash  $name`n", [Text.UTF8Encoding]::new($false))
    Copy-Item -LiteralPath (Join-Path $repo 'LICENSE') -Destination $dist
    if (Test-Path -LiteralPath (Join-Path $repo 'THIRD_PARTY_NOTICES.md')) { Copy-Item -LiteralPath (Join-Path $repo 'THIRD_PARTY_NOTICES.md') -Destination $dist }
    Write-Output "Packaged $asset"
    Write-Output "SHA256 $hash"
} finally { Pop-Location }
