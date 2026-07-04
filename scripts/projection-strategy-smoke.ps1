$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot

if ($env:SUNLIGHT_SMOKE_USE_WSL -ne '0') {
    $wsl = Get-Command wsl.exe -ErrorAction SilentlyContinue
    if ($wsl) {
        $wslRoot = wsl.exe wslpath -a $repoRoot
        if ($LASTEXITCODE -eq 0 -and $wslRoot) {
            wsl.exe bash "$wslRoot/scripts/projection-strategy-smoke.sh"
            exit $LASTEXITCODE
        }
    }
}

$bash = Get-Command bash -ErrorAction SilentlyContinue
if (!$bash) {
    throw 'bash is required to run scripts/projection-strategy-smoke.sh without WSL'
}

& $bash.Source (Join-Path $PSScriptRoot 'projection-strategy-smoke.sh')
exit $LASTEXITCODE
