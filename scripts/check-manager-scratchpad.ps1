$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$scratchpad = Join-Path $repoRoot 'docs/development_manager_scratchpad.md'
$limit = 20000

if (!(Test-Path -LiteralPath $scratchpad)) {
    throw "Scratchpad not found: $scratchpad"
}

$text = Get-Content -Raw -LiteralPath $scratchpad
$length = $text.Length

if ($length -ge $limit) {
    throw "Scratchpad is $length characters; keep it under $limit."
}

Write-Output "Scratchpad length OK: $length/$limit characters."
