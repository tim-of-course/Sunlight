$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$bashScript = Join-Path $PSScriptRoot 'projection-strategy-smoke.sh'

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
                Write-Warning 'Falling back to Windows-native smoke lane because projection-strategy-smoke.sh has CRLF line endings'
            } else {
                wsl.exe bash "$wslRoot/scripts/projection-strategy-smoke.sh"
                exit $LASTEXITCODE
            }
        }
    }
}

$cargo = if ($env:CARGO) { $env:CARGO } else { 'cargo' }
$tmpRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('sun-projection-strategy-smoke-' + [System.Guid]::NewGuid().ToString('N'))
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

    function Assert-NotContains($Text, $Needle, $Label) {
        if ($Text.Contains($Needle)) {
            throw "Unexpected output for ${Label}: $Needle`nstdout was:`n$Text"
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

    Step 'Resolving deterministic basic-app view'
    $includeReady = 'topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_ui_0001'
    $out = Invoke-SunOk 'resolve-ready' @('view', 'resolve', '--fixture', 'basic-app', '--include', $includeReady, '--json')
    Assert-Contains $out '"command":"view.resolve"' 'resolve command'
    Assert-Contains $out '"conflict_ids":[]' 'resolve conflicts'
    Assert-Contains $out '"staleness_ids":[]' 'resolve staleness'
    Assert-Contains $out '"tree_identity":{"kind":"SingleRepoTree","repository_id":"repo_fixture_basic_app","tree_hash":"tree_fixture_' 'resolve tree identity'
    $viewId = Json-StringField $out 'resolved_view_id'

    Step 'Verifying copy fallback and local-only metadata'
    $out = Invoke-SunOk 'default-copy' @('project', 'materialize', '--view', $viewId, '--purpose', 'execution', '--fixture', 'basic-app', '--json')
    Assert-Contains $out '"command":"projection.materialize"' 'copy command'
    Assert-Contains $out '"selected_strategy":"copy"' 'copy selected strategy'
    Assert-Contains $out '"strategy":"copy"' 'copy strategy'
    Assert-Contains $out '"source":"resolved_content_tree"' 'copy source'
    Assert-Contains $out '"created_from_content_tree":"tree_fixture_' 'copy content tree'
    Assert-Contains $out '"local_materialization":{"privacy_class":"local_only","projection_id":"projection_exec_auth_profile_0001"' 'copy local metadata'
    Assert-Contains $out '"root_ref":{"value":"local://.sunlight/projections/execution/projection_exec_auth_profile_0001","privacy":"local_only_path","privacy_class":"local_only"}' 'copy local root'
    Assert-Contains $out ':execution:copy:read_only_source_private_outputs' 'copy cache key'
    Assert-Contains $out '"store_integrity_policy":"verify_before_reuse"' 'copy integrity policy'
    Assert-NotContains $out $tmpRoot 'local-only metadata excludes smoke temp path'

    Step 'Verifying explicit reflink strategy selection JSON'
    $out = Invoke-SunOk 'reflink' @('project', 'materialize', '--view', $viewId, '--purpose', 'execution', '--strategy', 'reflink', '--fixture', 'basic-app', '--json')
    Assert-Contains $out '"selected_strategy":"reflink"' 'reflink selected strategy'
    Assert-Contains $out '"strategy":"reflink"' 'reflink strategy'
    Assert-Contains $out ':execution:reflink:read_only_source_private_outputs' 'reflink cache key'
    Assert-Contains $out '"local_materialization":{"privacy_class":"local_only","projection_id":"projection_exec_auth_profile_0001"' 'reflink local metadata'
    Assert-Contains $out '"source":"resolved_content_tree"' 'reflink source'

    Step 'Verifying ineligible preferred strategy falls back to copy'
    $out = Invoke-SunOk 'hardlink-copy-fallback' @('project', 'materialize', '--view', $viewId, '--purpose', 'execution', '--strategy', 'hardlink_readonly', '--fixture', 'basic-app', '--json')
    Assert-Contains $out '"selected_strategy":"copy"' 'hardlink fallback selected strategy'
    Assert-Contains $out '"strategy":"copy"' 'hardlink fallback strategy'
    Assert-Contains $out ':execution:copy:read_only_source_private_outputs' 'hardlink fallback cache key'
    Assert-Contains $out '"writable_policy":"read_only_source_private_outputs"' 'hardlink fallback writable policy'

    Step 'Verifying unsupported required strategy failure'
    $out = Invoke-SunFail 'hardlink-required' @('project', 'materialize', '--view', $viewId, '--purpose', 'execution', '--strategy', 'hardlink_readonly', '--no-copy-fallback', '--fixture', 'basic-app', '--json')
    Assert-Contains $out '"ok":false' 'required failure envelope'
    Assert-Contains $out '"code":"projection_materialization_hardlink_readonly_requires_read_only_policy"' 'required failure code'
    Assert-Contains $out '"message":"read-only hardlink materialization requires a read-only projection policy"' 'required failure message'
    Assert-Contains $out "`"resolved_view_id`":`"$viewId`"" 'required failure view id'
    Assert-Contains $out '"strategy":"hardlink_readonly"' 'required failure strategy'
    Assert-Contains $out '"projection_id":null' 'required failure no projection'

    Step 'Projection strategy smoke passed'
} finally {
    Remove-Item -Recurse -Force -LiteralPath $tmpRoot -ErrorAction SilentlyContinue
}
