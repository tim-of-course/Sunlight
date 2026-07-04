$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot

if ($env:SUNLIGHT_SMOKE_USE_WSL -ne '0') {
    $wsl = Get-Command wsl.exe -ErrorAction SilentlyContinue
    if ($wsl) {
        $wslRoot = wsl.exe wslpath -a $repoRoot
        if ($LASTEXITCODE -eq 0 -and $wslRoot) {
            wsl.exe bash "$wslRoot/scripts/smoke-suite.sh"
            exit $LASTEXITCODE
        }
    }
}

$cargo = if ($env:CARGO) { $env:CARGO } else { 'cargo' }
$tmpRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('sun-smoke-suite-' + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmpRoot | Out-Null

if (!$env:ZIG_LOCAL_CACHE_DIR) {
    $env:ZIG_LOCAL_CACHE_DIR = Join-Path $tmpRoot 'zig-local-cache'
}
if (!$env:ZIG_GLOBAL_CACHE_DIR) {
    $env:ZIG_GLOBAL_CACHE_DIR = Join-Path $tmpRoot 'zig-global-cache'
}
New-Item -ItemType Directory -Force -Path $env:ZIG_LOCAL_CACHE_DIR, $env:ZIG_GLOBAL_CACHE_DIR | Out-Null

function Step($Message) {
    Write-Output "==> $Message"
}

function Invoke-Checked($Label, [scriptblock]$Command) {
    Step $Label
    & $Command
    if ($LASTEXITCODE -ne 0) {
        throw "$Label failed with status $LASTEXITCODE"
    }
}

try {
    Push-Location $repoRoot
    try {
        Invoke-Checked "$cargo fmt --check" { & $cargo fmt --check }
        Invoke-Checked "$cargo check" { & $cargo check }
        Invoke-Checked "$cargo test" { & $cargo test }
    } finally {
        Pop-Location
    }

    Invoke-Checked 'scripts/validation-smoke.ps1' { & (Join-Path $PSScriptRoot 'validation-smoke.ps1') }
    Invoke-Checked 'scripts/projection-strategy-smoke.ps1' { & (Join-Path $PSScriptRoot 'projection-strategy-smoke.ps1') }
    Invoke-Checked 'scripts/mvp-smoke.ps1' { & (Join-Path $PSScriptRoot 'mvp-smoke.ps1') }

    Step 'Smoke suite passed'
} finally {
    Remove-Item -Recurse -Force -LiteralPath $tmpRoot -ErrorAction SilentlyContinue
}
