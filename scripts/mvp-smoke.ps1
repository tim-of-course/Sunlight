$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$cargo = if ($env:CARGO) { $env:CARGO } else { 'cargo' }
$tmpRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('sun-mvp-smoke-' + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmpRoot | Out-Null

try {
    function Step($Message) {
        Write-Output "==> $Message"
    }

    function Assert-Contains($Text, $Needle, $Label) {
        if (!$Text.Contains($Needle)) {
            throw "Missing expected output for ${Label}: $Needle`nstdout was:`n$Text"
        }
    }

    function Assert-FileContent($Path, $Expected, $Label) {
        if (!(Test-Path -LiteralPath $Path -PathType Leaf)) {
            throw "Missing projected fixture file for ${Label}: $Path"
        }
        $actual = Get-Content -Raw -LiteralPath $Path
        if ($actual.EndsWith("`r`n")) {
            $actual = $actual.Substring(0, $actual.Length - 2)
        } elseif ($actual.EndsWith("`n")) {
            $actual = $actual.Substring(0, $actual.Length - 1)
        }
        if ($actual -ne $Expected) {
            throw "Unexpected projected fixture content for $Label"
        }
    }

    function Invoke-SunOk($Label, [string[]]$Arguments) {
        $stdout = Join-Path $tmpRoot ($Label + '.stdout')
        $stderr = Join-Path $tmpRoot ($Label + '.stderr')
        $process = Start-Process -FilePath $script:sunBin -ArgumentList $Arguments -NoNewWindow -Wait -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
        $out = Get-Content -Raw -LiteralPath $stdout
        $err = Get-Content -Raw -LiteralPath $stderr
        if ($null -eq $out) {
            $out = ''
        }
        if ($null -eq $err) {
            $err = ''
        }
        if ($process.ExitCode -ne 0) {
            throw "Command failed for ${Label} with status $($process.ExitCode)`nargs: $($Arguments -join ' ')`nstdout:`n$out`nstderr:`n$err"
        }
        return $out
    }

    function Invoke-Git([string[]]$Arguments) {
        $stdout = Join-Path $tmpRoot ('git-' + [System.Guid]::NewGuid().ToString('N') + '.stdout')
        $stderr = Join-Path $tmpRoot ('git-' + [System.Guid]::NewGuid().ToString('N') + '.stderr')
        $process = Start-Process -FilePath 'git' -ArgumentList $Arguments -NoNewWindow -Wait -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
        $out = Get-Content -Raw -LiteralPath $stdout
        $err = Get-Content -Raw -LiteralPath $stderr
        if ($null -eq $out) {
            $out = ''
        }
        if ($null -eq $err) {
            $err = ''
        }
        if ($process.ExitCode -ne 0) {
            throw "git $($Arguments -join ' ') failed with status $($process.ExitCode)`nstdout:`n$out`nstderr:`n$err"
        }
        return $out.TrimEnd("`r", "`n")
    }

    function Invoke-GitInRepo([string[]]$Arguments) {
        return Invoke-Git (@('-C', $script:exportRepo) + $Arguments)
    }

    function Json-StringField($Json, $Field) {
        $match = [regex]::Match($Json, '"' + [regex]::Escape($Field) + '":(?:null|"([^"]*)")')
        if (!$match.Success -or !$match.Groups[1].Success) {
            throw "Could not find JSON string field: $Field"
        }
        return $match.Groups[1].Value
    }

    if (!(Get-Command git -ErrorAction SilentlyContinue)) {
        throw 'git is required'
    }

    if (!$env:ZIG_LOCAL_CACHE_DIR) {
        $env:ZIG_LOCAL_CACHE_DIR = Join-Path $tmpRoot 'zig-local-cache'
    }
    if (!$env:ZIG_GLOBAL_CACHE_DIR) {
        $env:ZIG_GLOBAL_CACHE_DIR = Join-Path $tmpRoot 'zig-global-cache'
    }
    New-Item -ItemType Directory -Force -Path $env:ZIG_LOCAL_CACHE_DIR, $env:ZIG_GLOBAL_CACHE_DIR | Out-Null

    Step 'Building sun CLI'
    Push-Location $repoRoot
    try {
        & $cargo build -p sun --quiet
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build -p sun failed with status $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }

    $script:sunBin = Join-Path $repoRoot 'target/debug/sun.exe'
    if (!(Test-Path -LiteralPath $script:sunBin)) {
        $script:sunBin = Join-Path $repoRoot 'target/debug/sun'
    }
    if (!(Test-Path -LiteralPath $script:sunBin)) {
        throw "Built CLI not found: $script:sunBin"
    }

    Step 'Resolving basic-app view'
    $includeReady = 'topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_ui_0001'
    $out = Invoke-SunOk 'resolve-ready' @('view', 'resolve', '--fixture', 'basic-app', '--include', $includeReady, '--json')
    Assert-Contains $out '"command":"view.resolve"' 'resolve command'
    Assert-Contains $out '"conflict_ids":[]' 'resolve conflicts'
    Assert-Contains $out '"staleness_ids":[]' 'resolve staleness'
    $viewId = Json-StringField $out 'resolved_view_id'

    Step 'Materializing base projection to filesystem'
    $projectionRoot = Join-Path $tmpRoot 'projection-root'
    New-Item -ItemType Directory -Path $projectionRoot | Out-Null
    $out = Invoke-SunOk 'project-filesystem' @('project', 'materialize', '--view', 'view_base_0001', '--purpose', 'execution', '--fixture', 'basic-app', '--projection-root', $projectionRoot, '--json')
    Assert-Contains $out '"command":"projection.materialize"' 'projection command'
    Assert-Contains $out '"projection_id":"projection_exec_auth_profile_0001"' 'projection id'
    Assert-Contains $out '"resolved_view_id":"view_base_0001"' 'projection base view'
    Assert-Contains $out '"selected_strategy":"copy"' 'projection strategy'
    Assert-Contains $out '"files_written":5' 'projected file count'
    Assert-Contains $out '"executable_files":1' 'projected executable count'

    $projectedFiles = Get-ChildItem -LiteralPath $projectionRoot -File -Recurse
    if ($projectedFiles.Count -ne 5) {
        throw "Projected file count is $($projectedFiles.Count), expected 5"
    }
    foreach ($path in @('README.md', 'docs/guide.md', 'scripts/build.sh', 'src/auth.ts', 'src/profile.ts')) {
        $fullPath = Join-Path $projectionRoot $path
        if (!(Test-Path -LiteralPath $fullPath -PathType Leaf)) {
            throw "Projection root missing $path"
        }
    }
    Assert-FileContent (Join-Path $projectionRoot 'README.md') "# Fixture Basic App`n`nUses User.email for login." 'README.md'
    Assert-FileContent (Join-Path $projectionRoot 'src/auth.ts') "export function login(email: string) {`n  return email.trim().toLowerCase();`n}" 'src/auth.ts'

    if ($PSVersionTable.PSEdition -eq 'Core' -and !$IsWindows) {
        $scriptMode = [int][System.IO.File]::GetUnixFileMode((Join-Path $projectionRoot 'scripts/build.sh'))
        $sourceMode = [int][System.IO.File]::GetUnixFileMode((Join-Path $projectionRoot 'src/auth.ts'))
        if (($scriptMode -band 73) -eq 0) {
            throw 'scripts/build.sh is not executable'
        }
        if (($sourceMode -band 73) -ne 0) {
            throw 'src/auth.ts should not be executable'
        }
    }

    Step 'Checking projection status and inspect local root verification'
    $out = Invoke-SunOk 'status-projection' @('status', '--projection', 'projection_exec_auth_profile_0001', '--fixture', 'basic-app', '--projection-root', $projectionRoot, '--json')
    Assert-Contains $out '"command":"status.projection"' 'projection status command'
    Assert-Contains $out '"projection_id":"projection_exec_auth_profile_0001"' 'projection status id'
    Assert-Contains $out '"lifecycle_state":"materialized"' 'projection status lifecycle'
    Assert-Contains $out '"verification_state":"present"' 'projection status verification state'
    Assert-Contains $out '"files":5' 'projection status file count'
    Assert-Contains $out '"bytes":222' 'projection status byte count'

    $out = Invoke-SunOk 'inspect-projection' @('inspect', 'projection:projection_exec_auth_profile_0001', '--fixture', 'basic-app', '--projection-root', $projectionRoot, '--json')
    Assert-Contains $out '"command":"inspect.projection"' 'projection inspect command'
    Assert-Contains $out '"id":"projection_exec_auth_profile_0001"' 'projection inspect id'
    Assert-Contains $out '"local_root_verification":{' 'projection inspect local root verification'
    Assert-Contains $out '"verification_state":"present"' 'projection inspect verification state'
    Assert-Contains $out '"files":5' 'projection inspect file count'
    Assert-Contains $out '"bytes":222' 'projection inspect byte count'

    Step 'Planning ready projection and running fixture command'
    $out = Invoke-SunOk 'project-execution' @('project', 'materialize', '--view', $viewId, '--purpose', 'execution', '--fixture', 'basic-app', '--json')
    Assert-Contains $out '"command":"projection.materialize"' 'projection command'
    Assert-Contains $out '"projection_id":"projection_exec_auth_profile_0001"' 'projection id'
    Assert-Contains $out '"selected_strategy":"copy"' 'projection strategy'

    $out = Invoke-SunOk 'run-tests' @('run', '--view', $viewId, '--fixture', 'basic-app', '--json', '--', 'cargo', 'test')
    Assert-Contains $out '"command":"execution.run"' 'execution command'
    Assert-Contains $out '"status":"pass"' 'execution status'
    Assert-Contains $out '"execution_id":"exec_auth_profile_tests_0001"' 'execution id'

    Step 'Creating export-ready checkpoint'
    $out = Invoke-SunOk 'checkpoint' @('checkpoint', 'create', '--view', $viewId, '--fixture', 'basic-app', '--json')
    Assert-Contains $out '"command":"checkpoint.create"' 'checkpoint command'
    Assert-Contains $out '"checkpoint_id":"checkpoint_auth_profile_ready_0001"' 'checkpoint id'
    Assert-Contains $out '"export_ready":true' 'checkpoint export ready'
    $checkpointId = Json-StringField $out 'checkpoint_id'

    Step 'Preparing temporary Git repository'
    $script:exportRepo = Join-Path $tmpRoot 'export-repo'
    New-Item -ItemType Directory -Path $script:exportRepo | Out-Null
    Invoke-GitInRepo @('init', '--quiet') | Out-Null
    Invoke-GitInRepo @('config', 'user.name', 'Sunlight Smoke') | Out-Null
    Invoke-GitInRepo @('config', 'user.email', 'sunlight-smoke@example.invalid') | Out-Null
    "base`n" | Set-Content -NoNewline -LiteralPath (Join-Path $script:exportRepo 'README.md')
    Invoke-GitInRepo @('add', 'README.md') | Out-Null
    Invoke-GitInRepo @('commit', '--quiet', '-m', 'Base') | Out-Null
    $baseCommit = Invoke-GitInRepo @('rev-parse', '--verify', 'HEAD^{commit}')

    Step 'Executing local Git export'
    $targetRef = 'refs/heads/sunlight/mvp-smoke'
    $out = Invoke-SunOk 'git-export-local' @('git', 'export', '--checkpoint', $checkpointId, '--branch', $targetRef, '--fixture', 'basic-app', '--execute-local', '--repo', $script:exportRepo, '--json')
    Assert-Contains $out '"command":"git.export.execute"' 'git export command'
    Assert-Contains $out '"lifecycle_state":"exported"' 'git export lifecycle'
    Assert-Contains $out '"commit_created":true' 'git export commit'
    Assert-Contains $out '"ref_updated":true' 'git export ref'
    Assert-Contains $out '"export_map_written":true' 'git export map'
    $createdCommit = Json-StringField $out 'created_commit_id'

    Step 'Verifying exported Git ref and tree'
    $refCommit = Invoke-GitInRepo @('rev-parse', '--verify', "$targetRef^{commit}")
    if ($refCommit -ne $createdCommit) {
        throw "target ref points to $refCommit, expected $createdCommit"
    }
    Invoke-GitInRepo @('cat-file', '-e', "$createdCommit^{commit}") | Out-Null
    $parentCommit = Invoke-GitInRepo @('rev-parse', '--verify', "$createdCommit^")
    if ($parentCommit -ne $baseCommit) {
        throw "export parent is $parentCommit, expected $baseCommit"
    }

    $treePaths = Invoke-GitInRepo @('ls-tree', '-r', '--name-only', $createdCommit)
    foreach ($path in @('src/auth.rs', 'src/profile.rs', 'bin/run-auth-check', '.sunlight/export-manifest.json')) {
        if (!$treePaths.Contains($path)) {
            throw "exported commit tree missing $path"
        }
    }

    $runTree = Invoke-GitInRepo @('ls-tree', $createdCommit, 'bin/run-auth-check')
    $runMode = ($runTree -split '\s+')[0]
    if ($runMode -ne '100755') {
        throw "bin/run-auth-check mode is $runMode, expected 100755"
    }

    $authContent = Invoke-GitInRepo @('show', "${createdCommit}:src/auth.rs")
    $manifestContent = Invoke-GitInRepo @('show', "${createdCommit}:.sunlight/export-manifest.json")
    if ($authContent -ne 'pub fn auth() {}') {
        throw 'unexpected src/auth.rs content'
    }
    Assert-Contains $manifestContent '"policy":"approved_manifest_only"' 'export manifest'

    Step 'MVP smoke passed'
} finally {
    Remove-Item -Recurse -Force -LiteralPath $tmpRoot -ErrorAction SilentlyContinue
}
