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
    Write-Host "==> $Message"
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

function Invoke-SunJson($Label, [string[]]$Arguments) {
    Step $Label
    $json = & $sunBin @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Label failed with status $LASTEXITCODE`nstdout:`n$($json -join "`n")"
    }

    $jsonText = $json -join "`n"
    $parsed = $jsonText | ConvertFrom-Json
    return @{
        Text = $jsonText
        Json = $parsed
    }
}

function Invoke-SunJsonFailure($Label, [string[]]$Arguments) {
    Step $Label
    $json = & $sunBin @Arguments
    if ($LASTEXITCODE -eq 0) {
        throw "$Label unexpectedly succeeded`nstdout:`n$($json -join "`n")"
    }

    $jsonText = $json -join "`n"
    $parsed = $jsonText | ConvertFrom-Json
    return @{
        Text = $jsonText
        Json = $parsed
    }
}

function Assert-JsonValue($Actual, $Expected, $Label) {
    if ($Actual -ne $Expected) {
        throw "Unexpected JSON value for ${Label}: expected '$Expected', got '$Actual'"
    }
}

function Assert-JsonTrue($Actual, $Label) {
    if ($Actual -ne $true) {
        throw "Unexpected JSON value for ${Label}: expected true, got '$Actual'"
    }
}

function Assert-JsonFalse($Actual, $Label) {
    if ($Actual -ne $false) {
        throw "Unexpected JSON value for ${Label}: expected false, got '$Actual'"
    }
}

function Assert-JsonNull($Actual, $Label) {
    if ($null -ne $Actual) {
        throw "Unexpected JSON value for ${Label}: expected null, got '$Actual'"
    }
}

function Assert-JsonWarningsEmpty($Json, $Label) {
    if ($Json.warnings.Count -ne 0) {
        throw "Unexpected JSON warnings for ${Label}: $($Json.warnings -join ', ')"
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

    $initResult = Invoke-SunJson 'sun init --json in temp clone' @('init', '--json', '--repo', $clonePath)
    $initJsonText = $initResult.Text
    $init = $initResult.Json
    Assert-JsonTrue $init.ok 'init ok envelope'
    Assert-JsonWarningsEmpty $init 'init warnings'
    $initCommand = $init.data.command
    if (!$initCommand) {
        $initCommand = $init.command
    }
    Assert-JsonValue $initCommand 'repository.init' 'init command'

    $configPath = Join-Path $clonePath '.sunlight/config.toml'
    if (!(Test-Path -LiteralPath $configPath -PathType Leaf)) {
        throw "sun init did not create $configPath"
    }

    Push-Location $clonePath
    try {
        $compatProjectResult = Invoke-SunJson 'sun compat project fixture command' @('compat', 'project', '--session', 'session_agent_a', '--fixture', 'basic-app', '--json')
        $compatProject = $compatProjectResult.Json
        Assert-JsonTrue $compatProject.ok 'compat project ok envelope'
        Assert-JsonWarningsEmpty $compatProject 'compat project warnings'
        Assert-JsonValue $compatProject.data.command 'compat.project' 'compat project command'
        Assert-JsonValue $compatProject.data.projection_id 'projection_compat_agent_a_0001' 'compat project projection id'
        Assert-JsonValue $compatProject.data.baseline_manifest_digest 'sha256:compat_baseline' 'compat project baseline manifest digest'

        $compatDiffResult = Invoke-SunJson 'sun compat diff fixture command' @('compat', 'diff', '--projection', 'projection_compat_agent_a_0001', '--fixture', 'basic-app', '--json')
        $compatDiff = $compatDiffResult.Json
        Assert-JsonTrue $compatDiff.ok 'compat diff ok envelope'
        Assert-JsonWarningsEmpty $compatDiff 'compat diff warnings'
        Assert-JsonValue $compatDiff.data.command 'compat.diff' 'compat diff command'
        Assert-JsonValue $compatDiff.data.projection_id 'projection_compat_agent_a_0001' 'compat diff projection id'
        Assert-JsonValue $compatDiff.data.candidate_counts.total 12 'compat diff candidate count'
        Assert-JsonValue $compatDiff.data.selected_candidate_delta_ids[0] 'compat_delta_src_auth_ts_0001' 'compat diff selected safe default'
        Assert-JsonValue $compatDiff.data.quarantine_refs[0] 'quarantine://compat/projection_compat_agent_a_0001/env' 'compat diff quarantine ref'

        $statusProjectionResult = Invoke-SunJson 'sun status compatibility projection fixture command' @('status', '--projection', 'projection_compat_agent_a_0001', '--fixture', 'basic-app', '--json')
        $statusProjection = $statusProjectionResult.Json
        Assert-JsonTrue $statusProjection.ok 'status projection ok envelope'
        Assert-JsonWarningsEmpty $statusProjection 'status projection warnings'
        Assert-JsonValue $statusProjection.data.command 'status.projection' 'status projection command'
        Assert-JsonValue $statusProjection.data.ids.projection_id 'projection_compat_agent_a_0001' 'status projection id'
        Assert-JsonValue $statusProjection.data.projection.purpose 'compatibility' 'status projection purpose'
        Assert-JsonValue $statusProjection.data.projection.candidate_counts.total 12 'status projection candidate count'
        Assert-JsonValue $statusProjection.data.projection.last_import_attempt.operation_transaction_id 'op_compat_import_auth_0001' 'status projection last import operation id'

        $inspectProjectionResult = Invoke-SunJson 'sun inspect compatibility projection fixture command' @('inspect', 'projection:projection_compat_agent_a_0001', '--fixture', 'basic-app', '--json')
        $inspectProjection = $inspectProjectionResult.Json
        Assert-JsonTrue $inspectProjection.ok 'inspect projection ok envelope'
        Assert-JsonWarningsEmpty $inspectProjection 'inspect projection warnings'
        Assert-JsonValue $inspectProjection.data.command 'inspect.projection' 'inspect projection command'
        Assert-JsonValue $inspectProjection.data.ids.projection_id 'projection_compat_agent_a_0001' 'inspect projection id'
        Assert-JsonValue $inspectProjection.data.projection.purpose 'compatibility' 'inspect projection purpose'
        Assert-JsonValue $inspectProjection.data.compatibility_projection.candidate_summary.candidate_counts.total 12 'inspect projection candidate count'
        Assert-JsonValue $inspectProjection.data.compatibility_projection.last_import_attempt.operation_transaction_id 'op_compat_import_auth_0001' 'inspect projection last import operation id'

        $compatResult = Invoke-SunJson 'sun compat import fixture command' @('compat', 'import', '--projection', 'projection_compat_agent_a_0001', '--candidate', 'compat_delta_src_auth_ts_0001', '--fixture', 'basic-app', '--json')
        $compat = $compatResult.Json
        Assert-JsonTrue $compat.ok 'compat import ok envelope'
        Assert-JsonWarningsEmpty $compat 'compat import warnings'
        Assert-JsonValue $compat.data.command 'compat.import' 'compat import command'
        Assert-JsonValue $compat.data.operation_transaction_id 'op_compat_import_auth_0001' 'compat import operation id'
        Assert-JsonValue $compat.data.topic_revision_id 'rev_auth_nullability_compat_0001' 'compat import revision id'

        $compatFailureResult = Invoke-SunJsonFailure 'sun compat import generated fixture atomic failure' @('compat', 'import', '--projection', 'projection_compat_agent_a_0001', '--candidate', 'compat_delta_generated_schema_0001', '--fixture', 'basic-app', '--json')
        $compatFailure = $compatFailureResult.Json
        Assert-JsonFalse $compatFailure.ok 'compat import generated failure envelope'
        Assert-JsonValue $compatFailure.error.code 'compat_policy_failed' 'compat import generated failure code'
        Assert-JsonValue $compatFailure.error.message 'selected compatibility candidate failed import policy' 'compat import generated failure message'
        Assert-JsonValue $compatFailure.error.details.projection_id 'projection_compat_agent_a_0001' 'compat import generated failure projection id'
        Assert-JsonValue $compatFailure.error.details.candidate_delta_ids[0] 'compat_delta_generated_schema_0001' 'compat import generated failure candidate id'
        Assert-JsonValue $compatFailure.error.details.reason 'generated or binary candidates require an explicit policy conversion' 'compat import generated failure reason'
        Assert-JsonValue $compatFailure.error.details.imported_artifacts.Count 0 'compat import generated failure imported artifact count'
        Assert-JsonNull $compatFailure.error.details.operation_transaction_id 'compat import generated failure operation id'
        Assert-JsonNull $compatFailure.error.details.topic_revision_id 'compat import generated failure revision id'
        Assert-JsonNull $compatFailure.error.details.session_generation_id 'compat import generated failure generation id'
    } finally {
        Pop-Location
    }

    $targetRef = 'refs/heads/sunlight/external-super-search-validation'
    $exportResult = Invoke-SunJson 'sun git export execute local against temp clone' @('git', 'export', '--checkpoint', 'checkpoint_auth_profile_ready_0001', '--branch', $targetRef, '--fixture', 'basic-app', '--execute-local', '--repo', $clonePath, '--json')
    $export = $exportResult.Json
    Assert-JsonTrue $export.ok 'git export ok envelope'
    Assert-JsonWarningsEmpty $export 'git export warnings'
    Assert-JsonValue $export.data.command 'git.export.execute' 'git export command'
    Assert-JsonValue $export.data.lifecycle_state 'exported' 'git export lifecycle'
    Assert-JsonTrue $export.data.summary.commit_created 'git export commit_created'
    Assert-JsonTrue $export.data.summary.ref_updated 'git export ref_updated'
    Assert-JsonTrue $export.data.summary.export_map_written 'git export export_map_written'

    Step 'Verify temp clone export ref resolves'
    $refCommit = & git -C $clonePath rev-parse --verify "$targetRef^{commit}"
    if ($LASTEXITCODE -ne 0) {
        throw "target ref did not resolve in temp clone: $targetRef"
    }
    $createdCommit = $export.data.created_commit_id
    if ($createdCommit -and (($refCommit -join "`n").Trim() -ne $createdCommit)) {
        throw "target ref points to $(($refCommit -join "`n").Trim()), expected $createdCommit"
    }

    Step 'Verify original target repo remains clean'
    $targetStatusAfter = @(& git -C $targetRepoPath status --short)
    if ($LASTEXITCODE -ne 0) {
        throw "final git status --short failed with status $LASTEXITCODE"
    }
    if ($targetStatusAfter.Count -gt 0) {
        throw "Target repo was modified during external validation:`n$($targetStatusAfter -join "`n")"
    }

    Step 'External Super Search validation passed'
} finally {
    if ($tmpRoot) {
        Remove-Item -Recurse -Force -LiteralPath $tmpRoot -ErrorAction SilentlyContinue
    }
}
