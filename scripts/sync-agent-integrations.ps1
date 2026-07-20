$ErrorActionPreference = "Stop"

$repo = Split-Path -Parent $PSScriptRoot
$source = Join-Path $repo "integrations\agent-skills\sunlight"
$target = Join-Path $repo "integrations\codex\plugins\sunlight\skills\sunlight"

$adapterRoot = [System.IO.Path]::GetFullPath(
    (Join-Path $repo "integrations\codex\plugins\sunlight")
)
$targetPath = [System.IO.Path]::GetFullPath($target)
if (-not $targetPath.StartsWith($adapterRoot + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to synchronize outside the Codex adapter root: $targetPath"
}

if (Test-Path -LiteralPath $target) {
    Remove-Item -LiteralPath $target -Recurse -Force
}

New-Item -ItemType Directory -Path (Split-Path -Parent $target) -Force | Out-Null
Copy-Item -LiteralPath $source -Destination $target -Recurse

Write-Output "Synchronized portable Sunlight skill into the Codex adapter."
