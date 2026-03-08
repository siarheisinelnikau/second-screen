param([switch]$Offline)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $repo
try {
    $metadataArgs = @('metadata', '--locked', '--format-version', '1', '--filter-platform', 'x86_64-pc-windows-msvc')
    if ($Offline) { $metadataArgs += '--offline' }
    $json = & cargo @metadataArgs
    if ($LASTEXITCODE -ne 0) { throw 'Cannot resolve locked dependency notices' }
    $metadata = $json | ConvertFrom-Json
    $nodes = @{}
    foreach ($node in $metadata.resolve.nodes) { $nodes[$node.id] = $node }
    $active = @{}
    $pending = [Collections.Generic.Stack[string]]::new()
    $pending.Push($metadata.resolve.root)
    while ($pending.Count -gt 0) {
        $id = $pending.Pop()
        if ($active.ContainsKey($id)) { continue }
        $active[$id] = $true
        foreach ($dependency in $nodes[$id].dependencies) { $pending.Push($dependency) }
    }
    $notice = [Text.StringBuilder]::new()
    [void]$notice.AppendLine('# Third-party notices')
    [void]$notice.AppendLine()
    [void]$notice.AppendLine('Generated from Cargo.lock for the Windows x64 dependency graph, including build and test dependencies. License choices remain available under the respective upstream terms. This inventory does not imply that every listed package is linked into the final executable.')
    $missing = @()
    foreach ($package in ($metadata.packages | Where-Object { $_.name -ne 'second-screen' -and $active.ContainsKey($_.id) } | Sort-Object name, version)) {
        $folder = Split-Path -Parent $package.manifest_path
        [void]$notice.AppendLine()
        [void]$notice.AppendLine("## $($package.name) $($package.version)")
        [void]$notice.AppendLine()
        [void]$notice.AppendLine("SPDX: $($package.license)")
        if ($package.repository) { [void]$notice.AppendLine("Upstream: $($package.repository)") }
        $files = @(Get-ChildItem -LiteralPath $folder -File | Where-Object Name -Match '^(LICENSE|LICENCE|COPYING|NOTICE|UNLICENSE|OFL)' | Sort-Object Name)
        foreach ($child in @('LICENSES', 'licenses', 'third_party')) {
            $directory = Join-Path $folder $child
            if (Test-Path -LiteralPath $directory) { $files += @(Get-ChildItem -LiteralPath $directory -Recurse -File | Where-Object { $_.Name -match '(LICENSE|LICENCE|COPYING|NOTICE|OFL)' -and $_.Length -lt 100000 }) }
        }
        if ($files.Count -eq 0) { $missing += "$($package.name) $($package.version)"; [void]$notice.AppendLine('No standalone license text was bundled in this crate; consult the upstream SPDX terms and source before distribution.') }
        foreach ($file in $files) {
            [void]$notice.AppendLine()
            [void]$notice.AppendLine("License file: $($file.Name)")
            [void]$notice.AppendLine('````text')
            [void]$notice.AppendLine([IO.File]::ReadAllText($file.FullName))
            [void]$notice.AppendLine('````')
        }
    }
    [IO.File]::WriteAllText((Join-Path $repo 'THIRD_PARTY_NOTICES.md'), $notice.ToString(), [Text.UTF8Encoding]::new($false))
    Write-Output "Generated third-party notices for the locked Windows graph. Missing standalone texts: $($missing.Count)"
    $missing | ForEach-Object { Write-Output $_ }
} finally { Pop-Location }
