param(
    [string]$TargetRepo
)

$ErrorActionPreference = 'Stop'

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path

if ([string]::IsNullOrWhiteSpace($TargetRepo)) {
    $TargetRepo = Join-Path (Split-Path -Parent $repoRoot) 'Super Search'
}

if ([System.IO.Path]::IsPathRooted($TargetRepo)) {
    $targetRepoPath = [System.IO.Path]::GetFullPath($TargetRepo)
} else {
    $targetRepoPath = [System.IO.Path]::GetFullPath((Join-Path (Get-Location) $TargetRepo))
}

$tmpRoot = $null

function Step($Message) {
    Write-Output "==> $Message"
}

function Require-Command($Name) {
    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if (!$command) {
        throw "Required command was not found on PATH: $Name"
    }
}

function Invoke-Checked($Label, [scriptblock]$Command) {
    Step $Label
    & $Command
    if ($LASTEXITCODE -ne 0) {
        throw "$Label failed with status $LASTEXITCODE"
    }
}

try {
    Require-Command 'git'
    Require-Command 'mix'
    Require-Command 'bun'
    Require-Command 'cargo'

    if (!(Test-Path -LiteralPath $targetRepoPath -PathType Container)) {
        throw "Target repo does not exist: $targetRepoPath"
    }

    Invoke-Checked 'Verify target is a Git repository' {
        & git -C $targetRepoPath rev-parse --is-inside-work-tree | Out-Null
    }

    $targetStatus = @(& git -C $targetRepoPath status --short)
    if ($LASTEXITCODE -ne 0) {
        throw "git status --short failed with status $LASTEXITCODE"
    }
    if ($targetStatus.Count -gt 0) {
        throw "Target repo must be clean before external validation:`n$($targetStatus -join "`n")"
    }

    Invoke-Checked 'mix test in target repo' {
        Push-Location $targetRepoPath
        try {
            & mix test
        } finally {
            Pop-Location
        }
    }

    Invoke-Checked 'bun run test in target repo' {
        Push-Location $targetRepoPath
        try {
            & bun run test
        } finally {
            Pop-Location
        }
    }

    Invoke-Checked 'Build sun CLI' {
        Push-Location $repoRoot
        try {
            & cargo build -p sun --quiet
        } finally {
            Pop-Location
        }
    }

    $sunBin = Join-Path $repoRoot 'target/debug/sun.exe'
    if (!(Test-Path -LiteralPath $sunBin)) {
        $sunBin = Join-Path $repoRoot 'target/debug/sun'
    }
    if (!(Test-Path -LiteralPath $sunBin)) {
        throw "Built sun CLI was not found under target/debug"
    }

    $tmpRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('sun-external-super-search-' + [System.Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $tmpRoot | Out-Null
    $clonePath = Join-Path $tmpRoot 'Super Search'

    Invoke-Checked 'Clone target repo into temp directory' {
        & git clone --no-hardlinks -- $targetRepoPath $clonePath
    }

    Step 'sun init --json in temp clone'
    $initJson = & $sunBin init --json --repo $clonePath
    if ($LASTEXITCODE -ne 0) {
        throw "sun init --json failed with status $LASTEXITCODE`nstdout:`n$($initJson -join "`n")"
    }

    $initJsonText = $initJson -join "`n"
    $init = $initJsonText | ConvertFrom-Json
    $initCommand = $init.data.command
    if (!$initCommand) {
        $initCommand = $init.command
    }
    if ($initCommand -ne 'repository.init') {
        throw "sun init JSON did not include command repository.init:`n$initJsonText"
    }

    $configPath = Join-Path $clonePath '.sunlight/config.toml'
    if (!(Test-Path -LiteralPath $configPath -PathType Leaf)) {
        throw "sun init did not create $configPath"
    }

    Step 'External Super Search validation passed'
} finally {
    if ($tmpRoot) {
        Remove-Item -Recurse -Force -LiteralPath $tmpRoot -ErrorAction SilentlyContinue
    }
}
