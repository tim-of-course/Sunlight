$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$bashScript = Join-Path $PSScriptRoot 'validation-smoke.sh'

function Test-HasCrlf($Path) {
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    for ($i = 0; $i -lt ($bytes.Length - 1); $i++) {
        if ($bytes[$i] -eq 13 -and $bytes[$i + 1] -eq 10) {
            return $true
        }
    }
    return $false
}

if ($env:SUNLIGHT_SMOKE_USE_WSL -ne '0') {
    $wsl = Get-Command wsl.exe -ErrorAction SilentlyContinue
    if ($wsl) {
        $wslRoot = wsl.exe wslpath -a $repoRoot
        if ($LASTEXITCODE -eq 0 -and $wslRoot) {
            if (Test-HasCrlf $bashScript) {
                Write-Warning 'Falling back to Windows-native smoke lane because validation-smoke.sh has CRLF line endings'
            } else {
                wsl.exe bash "$wslRoot/scripts/validation-smoke.sh"
                exit $LASTEXITCODE
            }
        }
    }
}

$cargo = if ($env:CARGO) { $env:CARGO } else { 'cargo' }
$tmpRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('sun-validation-smoke-' + [System.Guid]::NewGuid().ToString('N'))
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

    function Invoke-SunOk($Label, [string[]]$Arguments) {
        $stdout = Join-Path $tmpRoot ($Label + '.stdout')
        $stderr = Join-Path $tmpRoot ($Label + '.stderr')
        $process = Start-Process -FilePath $script:sunBin -ArgumentList $Arguments -NoNewWindow -Wait -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
        $out = Get-Content -Raw -LiteralPath $stdout
        $err = Get-Content -Raw -LiteralPath $stderr
        if ($process.ExitCode -ne 0) {
            throw "Command failed for ${Label} with status $($process.ExitCode)`nargs: $($Arguments -join ' ')`nstdout:`n$out`nstderr:`n$err"
        }
        return $out
    }

    function Invoke-SunFail($Label, [string[]]$Arguments) {
        $stdout = Join-Path $tmpRoot ($Label + '.stdout')
        $stderr = Join-Path $tmpRoot ($Label + '.stderr')
        $process = Start-Process -FilePath $script:sunBin -ArgumentList $Arguments -NoNewWindow -Wait -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
        $out = Get-Content -Raw -LiteralPath $stdout
        $err = Get-Content -Raw -LiteralPath $stderr
        if ($process.ExitCode -eq 0) {
            throw "Command unexpectedly succeeded for ${Label}`nargs: $($Arguments -join ' ')`nstdout:`n$out`nstderr:`n$err"
        }
        return $out
    }

    function Json-StringField($Json, $Field) {
        $match = [regex]::Match($Json, '"' + [regex]::Escape($Field) + '":"([^"]*)"')
        if (!$match.Success) {
            throw "Could not find JSON string field: $Field"
        }
        return $match.Groups[1].Value
    }

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

    $initRepo = Join-Path $tmpRoot 'init-repo'
    New-Item -ItemType Directory -Path $initRepo | Out-Null

    Step 'Init real temporary repository'
    $out = Invoke-SunOk 'init' @('init', '--json', '--repo', $initRepo)
    Assert-Contains $out '"command":"repository.init"' 'init command'
    Assert-Contains $out '"ok":true' 'init ok'
    if (!(Test-Path -LiteralPath (Join-Path $initRepo '.sunlight/config.toml'))) {
        throw 'sun init did not create .sunlight/config.toml'
    }

    $out = Invoke-SunOk 'init-idempotent' @('init', '--json', '--repo', $initRepo)
    Assert-Contains $out '"command":"repository.init"' 'init idempotent command'
    Assert-Contains $out '"ok":true' 'init idempotent ok'

    Push-Location $initRepo
    try {
        $out = Invoke-SunOk 'policy-check-commit' @('policy', 'check-commit', '--json')
    } finally {
        Pop-Location
    }
    Assert-Contains $out '"command":"policy.check-commit"' 'policy check commit command'
    Assert-Contains $out '"ok":true' 'policy check commit ok'
    Assert-Contains $out '"managed_ignore_blocks_checked":1' 'policy check commit managed ignore blocks'
    Assert-Contains $out '"candidate_paths_checked":0' 'policy check commit candidate paths'
    Assert-Contains $out '"blocked":0' 'policy check commit blocked'

    Step 'Read/list/search fixture artifacts'
    $out = Invoke-SunOk 'read' @('read', 'src/auth.ts', '--session', 'session_agent_a', '--fixture', 'basic-app', '--json')
    Assert-Contains $out '"command":"artifact.read"' 'read command'
    Assert-Contains $out '"artifact_id":"artifact_src_auth_ts"' 'read artifact'
    Assert-Contains $out '"content_hash":"sha256:auth_base"' 'read hash'

    $out = Invoke-SunFail 'read-missing' @('read', 'src/missing.ts', '--session', 'session_agent_a', '--fixture', 'basic-app', '--json')
    Assert-Contains $out '"code":"path_not_found"' 'read missing'
    Assert-Contains $out '"session_generation_id":"gen_agent_a_0001"' 'read missing generation'

    $out = Invoke-SunOk 'list' @('list', 'src', '--session', 'session_agent_a', '--fixture', 'basic-app', '--json')
    Assert-Contains $out '"command":"artifact.list"' 'list command'
    Assert-Contains $out '"path":"src/auth.ts"' 'list auth'
    Assert-Contains $out '"path":"src/profile.ts"' 'list profile'

    $out = Invoke-SunOk 'search' @('search', 'User.email', '--session', 'session_agent_a', '--fixture', 'basic-app', '--json')
    Assert-Contains $out '"command":"artifact.search"' 'search command'
    Assert-Contains $out '"path":"README.md"' 'search readme'
    Assert-Contains $out '"path":"docs/guide.md"' 'search guide'
    Assert-Contains $out '"path":"src/profile.ts"' 'search profile'

    Step 'Exercise fixture writes and preconditions'
    $patchFile = Join-Path $tmpRoot 'auth.patch'
    @'
--- a/src/auth.ts
+++ b/src/auth.ts
@@ -1,3 +1,4 @@
 export function login(email: string) {
-  return email.trim().toLowerCase();
+  const normalized = email.trim().toLowerCase();
+  return normalized;
 }
'@ | Set-Content -NoNewline -LiteralPath $patchFile

    $out = Invoke-SunOk 'patch' @('patch', 'src/auth.ts', '--session', 'session_agent_a', '--fixture', 'basic-app', '--expect-hash', 'sha256:auth_base', '--patch-file', $patchFile, '--json')
    Assert-Contains $out '"command":"artifact.patch"' 'patch command'
    Assert-Contains $out '"operation_transaction_id":"op_auth_trim_guard_0001"' 'patch operation'
    Assert-Contains $out '"after_hash":"sha256:auth_trim_guard"' 'patch hash'

    $contentFile = Join-Path $tmpRoot 'session.ts'
    "export const sessionLabel = `"SessionStore`";`n" | Set-Content -NoNewline -LiteralPath $contentFile
    $out = Invoke-SunOk 'write' @('write', 'src/session.ts', '--session', 'session_agent_a', '--fixture', 'basic-app', '--expect-hash', 'new', '--content-file', $contentFile, '--classification', 'source', '--json')
    Assert-Contains $out '"command":"artifact.write"' 'write command'
    Assert-Contains $out '"artifact_id":"artifact_src_session_ts"' 'write artifact'
    Assert-Contains $out '"after_hash":"sha256:session_new"' 'write hash'

    $out = Invoke-SunFail 'patch-stale' @('patch', 'src/auth.ts', '--session', 'session_agent_a', '--fixture', 'basic-app', '--expect-hash', 'sha256:stale_auth', '--patch-file', $patchFile, '--json')
    Assert-Contains $out '"code":"precondition_failed"' 'stale patch'
    Assert-Contains $out '"session_generation_id":"gen_agent_a_0001"' 'stale patch generation'

    Step 'Resolve compatible and conflicted views'
    $includeReady = 'topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_ui_0001'
    $out = Invoke-SunOk 'resolve-ready' @('view', 'resolve', '--fixture', 'basic-app', '--include', $includeReady, '--json')
    Assert-Contains $out '"command":"view.resolve"' 'resolve ready command'
    Assert-Contains $out '"conflict_ids":[]' 'resolve ready conflicts'
    Assert-Contains $out '"staleness_ids":[]' 'resolve ready staleness'
    $viewId = Json-StringField $out 'resolved_view_id'

    $out = Invoke-SunOk 'resolve-conflict' @('view', 'resolve', '--fixture', 'basic-app', '--include', 'topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_auth_overlap_0001', '--json')
    Assert-Contains $out '"view":null' 'resolve conflict no view'
    Assert-Contains $out '"conflict_ids":["conflict_src_auth_ts_0001"]' 'resolve conflict id'
    Assert-Contains $out '"kind":"same_artifact_conflict"' 'resolve conflict kind'

    Step 'Materialize, run, and checkpoint ready view'
    $out = Invoke-SunOk 'project-execution' @('project', 'materialize', '--view', $viewId, '--purpose', 'execution', '--fixture', 'basic-app', '--json')
    Assert-Contains $out '"command":"projection.materialize"' 'execution projection command'
    Assert-Contains $out '"projection_id":"projection_exec_auth_profile_0001"' 'execution projection id'
    Assert-Contains $out '"selected_strategy":"copy"' 'execution projection strategy'

    $out = Invoke-SunOk 'run' @('run', '--view', $viewId, '--fixture', 'basic-app', '--json', '--', 'cargo', 'test')
    Assert-Contains $out '"command":"execution.run"' 'run command'
    Assert-Contains $out '"execution_id":"exec_auth_profile_tests_0001"' 'run execution'
    Assert-Contains $out '"status":"pass"' 'run result'

    $out = Invoke-SunOk 'checkpoint' @('checkpoint', 'create', '--view', $viewId, '--fixture', 'basic-app', '--json')
    Assert-Contains $out '"command":"checkpoint.create"' 'checkpoint command'
    Assert-Contains $out '"checkpoint_id":"checkpoint_auth_profile_ready_0001"' 'checkpoint id'
    Assert-Contains $out '"export_ready":true' 'checkpoint export ready'

    $out = Invoke-SunOk 'policy-check-export' @('policy', 'check-export', '--checkpoint', 'checkpoint_auth_profile_ready_0001', '--fixture', 'basic-app', '--json')
    Assert-Contains $out '"command":"policy.check-export"' 'policy check export command'
    Assert-Contains $out '"validation_report_id":"validation_export_auth_profile_ready_0001"' 'policy check export validation report'
    Assert-Contains $out '"failures":[]' 'policy check export failures'

    Step 'Compatibility project, diff, import, and Git export write plan'
    $out = Invoke-SunOk 'compat-project' @('compat', 'project', '--session', 'session_agent_a', '--fixture', 'basic-app', '--json')
    Assert-Contains $out '"command":"compat.project"' 'compat project command'
    Assert-Contains $out '"projection_id":"projection_compat_agent_a_0001"' 'compat projection id'
    Assert-Contains $out '"baseline_manifest_digest":"sha256:compat_baseline"' 'compat project baseline manifest digest'

    $out = Invoke-SunOk 'compat-diff' @('compat', 'diff', '--projection', 'projection_compat_agent_a_0001', '--fixture', 'basic-app', '--json')
    Assert-Contains $out '"command":"compat.diff"' 'compat diff command'
    Assert-Contains $out '"selected_candidate_delta_ids":["compat_delta_src_auth_ts_0001"]' 'compat diff selected safe default'
    Assert-Contains $out '"quarantine_refs":["quarantine://compat/projection_compat_agent_a_0001/env"]' 'compat diff quarantine refs'

    $out = Invoke-SunOk 'compat-import' @('compat', 'import', '--projection', 'projection_compat_agent_a_0001', '--candidate', 'compat_delta_src_auth_ts_0001', '--fixture', 'basic-app', '--json')
    Assert-Contains $out '"command":"compat.import"' 'compat import command'
    Assert-Contains $out '"operation_transaction_id":"op_compat_import_auth_0001"' 'compat import operation'
    Assert-Contains $out '"topic_revision_id":"rev_auth_nullability_compat_0001"' 'compat import revision'

    $out = Invoke-SunOk 'git-export-write-plan' @('git', 'export', '--checkpoint', 'checkpoint_auth_profile_ready_0001', '--branch', 'refs/heads/sunlight/auth-profile-ready', '--fixture', 'basic-app', '--write-plan', '--json')
    Assert-Contains $out '"command":"git.export.write_plan"' 'git export write plan command'
    Assert-Contains $out '"validation_report_id":"validation_export_auth_profile_ready_0001"' 'git export validation report'
    Assert-Contains $out '"planned_commit_id":"git_sha1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"' 'git export planned commit'

    Step 'Validation smoke passed'
} finally {
    Remove-Item -Recurse -Force -LiteralPath $tmpRoot -ErrorAction SilentlyContinue
}
