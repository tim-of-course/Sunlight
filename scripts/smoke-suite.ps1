$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$bashScripts = @(
    'smoke-suite.sh',
    'validation-smoke.sh',
    'projection-strategy-smoke.sh',
    'mvp-smoke.sh'
)

$forceNativeChildSmokes = $false

function Test-HasCrlf($Path) {
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    for ($i = 0; $i -lt ($bytes.Length - 1); $i++) {
        if ($bytes[$i] -eq 13 -and $bytes[$i + 1] -eq 10) {
            return $true
        }
    }
    return $false
}

$requireWsl = (![string]::IsNullOrEmpty($env:SUNLIGHT_SMOKE_USE_WSL) -and $env:SUNLIGHT_SMOKE_USE_WSL -ne '0')

if ($env:SUNLIGHT_SMOKE_USE_WSL -ne '0') {
    $wsl = Get-Command wsl.exe -ErrorAction SilentlyContinue
    if ($wsl) {
        $wslRoot = wsl.exe wslpath -a $repoRoot
        if ($LASTEXITCODE -eq 0 -and $wslRoot) {
            $crlfScripts = @($bashScripts | Where-Object {
                Test-HasCrlf (Join-Path $PSScriptRoot $_)
            })
            if ($crlfScripts.Count -gt 0) {
                if ($env:SUNLIGHT_SMOKE_USE_WSL) {
                    throw "WSL smoke lane requested, but these shell scripts have CRLF line endings: $($crlfScripts -join ', ')"
                }
                Write-Warning "Falling back to Windows-native smoke lane because these shell scripts have CRLF line endings: $($crlfScripts -join ', ')"
                $forceNativeChildSmokes = $true
            } else {
                wsl.exe bash "$wslRoot/scripts/smoke-suite.sh"
                exit $LASTEXITCODE
            }
        }
        if ($requireWsl) {
            throw "WSL smoke lane requested, but wslpath could not resolve repository path: $repoRoot"
        }
    } elseif ($requireWsl) {
        throw 'WSL smoke lane requested, but wsl.exe was not found'
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

if ($forceNativeChildSmokes) {
    $env:SUNLIGHT_SMOKE_USE_WSL = '0'
}

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
