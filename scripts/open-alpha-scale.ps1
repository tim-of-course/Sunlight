param(
    [string]$TargetRepo = 'C:\Users\TimothyCardoza\Documents\AI-Apps\Phaser',
    [string]$EvidencePath,
    [switch]$ConfirmLocalSsd,
    [switch]$KeepFixture
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$sun = Join-Path $repoRoot 'target\release\sun.exe'
if (!(Test-Path -LiteralPath $sun -PathType Leaf)) {
    throw "Build the distributable Sunlight artifact first: cargo build --release -p sun"
}
if (!(Test-Path -LiteralPath $TargetRepo -PathType Container)) {
    throw "Target repository does not exist: $TargetRepo"
}
if ([string]::IsNullOrWhiteSpace($EvidencePath)) {
    $stamp = [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss')
    $EvidencePath = Join-Path $repoRoot "docs\acceptance\evidence\oa07-$stamp.json"
}
if (Test-Path -LiteralPath $EvidencePath) {
    throw "Refusing to overwrite existing evidence: $EvidencePath"
}

$tempBase = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$tempRoot = Join-Path $tempBase ('sun-oa07-' + [Guid]::NewGuid().ToString('N'))
$fixture = Join-Path $tempRoot 'target'
$clients = @()
$samples = [System.Collections.Generic.List[object]]::new()
$mcpToolCallCount = 0
$journey = $null
$comparableSunlightJourney = $null
$comparableSunlightActionCount = 1

function Invoke-Checked([string]$Label, [scriptblock]$Command) {
    $previousErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $output = & $Command 2>&1
        $exitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    if ($exitCode -ne 0) {
        throw "$Label failed with exit code $exitCode`n$($output -join "`n")"
    }
    return $output
}

function Get-TreeBytes([string]$Path) {
    if (!(Test-Path -LiteralPath $Path)) { return [int64]0 }
    $enumerationPath = [System.IO.Path]::GetFullPath($Path)
    if ($env:OS -eq 'Windows_NT' -and !$enumerationPath.StartsWith('\\?\')) {
        $enumerationPath = '\\?\' + $enumerationPath
    }
    return [int64]((Get-ChildItem -LiteralPath $enumerationPath -Recurse -Force -File |
        Measure-Object -Property Length -Sum).Sum)
}

function Get-NativeStateBytes([string]$Path) {
    if (!(Test-Path -LiteralPath $Path)) { return [int64]0 }
    $enumerationPath = [System.IO.Path]::GetFullPath($Path)
    if ($env:OS -eq 'Windows_NT' -and !$enumerationPath.StartsWith('\\?\')) {
        $enumerationPath = '\\?\' + $enumerationPath
    }
    $root = $enumerationPath.TrimEnd('\', '/') + '\'
    [int64]$sum = 0
    foreach ($file in @(Get-ChildItem -LiteralPath $enumerationPath -Recurse -Force -File)) {
        $relative = $file.FullName.Substring($root.Length)
        if ($relative -notmatch '^(cache|local|projections)[\\/]') {
            $sum += $file.Length
        }
    }
    return $sum
}

function New-McpClient([int]$Number) {
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $sun
    $start.UseShellExecute = $false
    $start.RedirectStandardInput = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $false
    $start.Arguments = 'mcp serve --repo "' + $fixture.Replace('"', '\"') + '"'
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $start
    [void]$process.Start()
    $process.StandardInput.AutoFlush = $true
    $client = @{
        Number = $Number
        Process = $process
        NextId = 2
        Pending = @{}
    }
    $initialize = @{
        jsonrpc = '2.0'; id = 1; method = 'initialize'
        params = @{
            protocolVersion = '2025-11-25'
            capabilities = @{}
            clientInfo = @{ name = "sun-oa07-$Number"; version = '1' }
        }
    } | ConvertTo-Json -Compress -Depth 20
    $process.StandardInput.WriteLine($initialize)
    $line = $process.StandardOutput.ReadLine()
    if ([string]::IsNullOrWhiteSpace($line)) {
        throw "MCP client $Number stopped during initialization"
    }
    $response = $line | ConvertFrom-Json
    if ($response.result.protocolVersion -ne '2025-11-25') {
        throw "MCP client $Number negotiated an unexpected protocol"
    }
    $notification = @{
        jsonrpc = '2.0'; method = 'notifications/initialized'; params = @{}
    } | ConvertTo-Json -Compress -Depth 10
    $process.StandardInput.WriteLine($notification)
    return $client
}

function Send-McpCall($Client, [string]$Tool, [hashtable]$Arguments) {
    $script:mcpToolCallCount++
    $id = [int]$Client.NextId
    $Client.NextId = $id + 1
    $request = @{
        jsonrpc = '2.0'; id = $id; method = 'tools/call'
        params = @{ name = $Tool; arguments = $Arguments }
    } | ConvertTo-Json -Compress -Depth 100
    $Client.Pending[$id] = [DateTime]::UtcNow
    $Client.Process.StandardInput.WriteLine($request)
    return $id
}

function Receive-McpCall($Client, [int]$Id) {
    $line = $Client.Process.StandardOutput.ReadLine()
    if ([string]::IsNullOrWhiteSpace($line)) {
        throw "MCP client $($Client.Number) stopped before response $Id"
    }
    $response = $line | ConvertFrom-Json
    if ([int]$response.id -ne $Id) {
        throw "MCP client $($Client.Number) returned response $($response.id), expected $Id"
    }
    $started = [DateTime]$Client.Pending[$Id]
    [void]$Client.Pending.Remove($Id)
    $elapsed = ([DateTime]::UtcNow - $started).TotalMilliseconds
    if ($null -ne $response.error) {
        throw "MCP protocol error: $($response.error | ConvertTo-Json -Compress -Depth 20)"
    }
    $envelope = $response.result.structuredContent
    if ($envelope.ok -ne $true) {
        throw "Sunlight tool error: $($envelope.error | ConvertTo-Json -Compress -Depth 50)"
    }
    return [pscustomobject]@{ Envelope = $envelope; ExternalMs = [math]::Round($elapsed, 3) }
}

function Invoke-McpCall($Client, [string]$Tool, [hashtable]$Arguments) {
    $id = Send-McpCall $Client $Tool $Arguments
    return Receive-McpCall $Client $id
}

function Add-Sample([string]$Operation, $Call) {
    $transport = $Call.Envelope.data.transport
    $samples.Add([pscustomobject]@{
        operation = $Operation
        external_ms = [double]$Call.ExternalMs
        queue_ms = [double]$transport.queue_ms
        worker_ms = [double]$transport.worker_ms
        automatic_concurrency_retries = [int]$transport.automatic_concurrency_retries
    })
}

function Get-Percentile([double[]]$Values, [double]$Percentile) {
    if ($Values.Count -eq 0) { return $null }
    $ordered = @($Values | Sort-Object)
    $index = [math]::Max(0, [math]::Ceiling($Percentile * $ordered.Count) - 1)
    return [math]::Round([double]$ordered[$index], 3)
}

function Get-LatencySummary([string]$Operation) {
    $matching = @($samples | Where-Object operation -eq $Operation)
    $values = [double[]]@($matching | ForEach-Object external_ms)
    return [ordered]@{
        count = $values.Count
        p50_ms = Get-Percentile $values 0.50
        p95_ms = Get-Percentile $values 0.95
        max_ms = if ($values.Count) { [math]::Round(($values | Measure-Object -Maximum).Maximum, 3) } else { $null }
    }
}

function Stop-McpClient($Client) {
    if ($null -eq $Client -or $null -eq $Client.Process) { return }
    try { $Client.Process.StandardInput.Close() } catch {}
    if (!$Client.Process.WaitForExit(5000)) {
        try { $Client.Process.Kill() } catch {}
        [void]$Client.Process.WaitForExit(5000)
    }
    $Client.Process.Dispose()
}

try {
    New-Item -ItemType Directory -Path $tempRoot | Out-Null
    $sourceCommit = (Invoke-Checked 'read target commit' { git -C $TargetRepo rev-parse HEAD } | Select-Object -Last 1).Trim()
    $sourceStatus = @(& git -C $TargetRepo status --short)
    $trackedFileCount = @(& git -C $TargetRepo ls-files).Count

    $cloneTimer = [Diagnostics.Stopwatch]::StartNew()
    [void](Invoke-Checked 'clone target locally' { git clone --quiet --no-hardlinks --local -- $TargetRepo $fixture })
    $cloneTimer.Stop()
    [void](Invoke-Checked 'remove disposable origin' { git -C $fixture remote remove origin })
    $fixtureRemotes = @(& git -C $fixture remote)
    if ($fixtureRemotes.Count -ne 0) { throw 'Disposable fixture still has a Git remote' }

    $logicalTrackedBytes = [int64]0
    foreach ($relative in @(& git -C $fixture ls-files)) {
        $path = Join-Path $fixture $relative
        if (Test-Path -LiteralPath $path -PathType Leaf) {
            $logicalTrackedBytes += (Get-Item -LiteralPath $path).Length
        }
    }
    $conventionalBytes = Get-TreeBytes $fixture
    $conventionalChangeRoot = Join-Path $fixture 'sunlight_alpha'
    if (Test-Path -LiteralPath $conventionalChangeRoot) {
        throw "Conventional comparison path already exists: $conventionalChangeRoot"
    }
    $fixtureStatusBeforeBaseline = @(Invoke-Checked 'read pre-baseline fixture status' {
        git -C $fixture status --short
    })
    $baselineJourneyTimer = [Diagnostics.Stopwatch]::StartNew()
    $baselineAuthoringTimer = [Diagnostics.Stopwatch]::StartNew()
    New-Item -ItemType Directory -Path $conventionalChangeRoot | Out-Null
    $utf8WithoutBom = New-Object System.Text.UTF8Encoding -ArgumentList $false
    for ($round = 1; $round -le 5; $round++) {
        for ($worker = 1; $worker -le 4; $worker++) {
            $path = Join-Path $conventionalChangeRoot "worker-$worker-$round.txt"
            [System.IO.File]::WriteAllText(
                $path,
                "OA-07 worker $worker, revision $round`n",
                $utf8WithoutBom
            )
        }
    }
    $baselineAuthoringTimer.Stop()
    $conventionalChangedPaths = @(Get-ChildItem -LiteralPath $conventionalChangeRoot -File |
        ForEach-Object { 'sunlight_alpha/' + $_.Name })
    if ($conventionalChangedPaths.Count -ne 20) {
        throw "Conventional comparison created $($conventionalChangedPaths.Count) paths, expected 20"
    }
    $baselineTimer = [Diagnostics.Stopwatch]::StartNew()
    Push-Location (Join-Path $fixture 'assets\control')
    try {
        [void](Invoke-Checked 'conventional target test' { bun test ./src/lib })
    } finally {
        Pop-Location
    }
    $baselineTimer.Stop()
    $baselineJourneyTimer.Stop()
    $resolvedConventionalChangeRoot = [System.IO.Path]::GetFullPath($conventionalChangeRoot)
    $resolvedFixtureRoot = [System.IO.Path]::GetFullPath($fixture).TrimEnd('\', '/') + '\'
    if (!$resolvedConventionalChangeRoot.StartsWith($resolvedFixtureRoot, [StringComparison]::OrdinalIgnoreCase) -or
        (Split-Path -Leaf $resolvedConventionalChangeRoot) -ne 'sunlight_alpha') {
        throw "Refusing to remove unexpected conventional comparison path: $resolvedConventionalChangeRoot"
    }
    Remove-Item -LiteralPath $resolvedConventionalChangeRoot -Recurse -Force
    $fixtureStatusAfterBaselineCleanup = @(Invoke-Checked 'verify baseline cleanup status' {
        git -C $fixture status --short
    })
    $baselineCleanupVerified = !(Test-Path -LiteralPath $resolvedConventionalChangeRoot) -and
        (($fixtureStatusAfterBaselineCleanup -join "`n") -eq ($fixtureStatusBeforeBaseline -join "`n"))
    if (!$baselineCleanupVerified) {
        throw 'Conventional comparison cleanup did not restore the disposable fixture exactly'
    }

    $initTimer = [Diagnostics.Stopwatch]::StartNew()
    $initText = (& $sun init --repo $fixture --json) -join "`n"
    if ($LASTEXITCODE -ne 0) { throw "sun init failed`n$initText" }
    $initTimer.Stop()
    $init = $initText | ConvertFrom-Json
    if ($init.ok -ne $true) { throw "sun init returned failure: $initText" }
    $journey = [Diagnostics.Stopwatch]::StartNew()
    $comparableSunlightJourney = [Diagnostics.Stopwatch]::StartNew()
    Push-Location $fixture
    try {
        $statusText = (& $sun status --json) -join "`n"
        $statusExitCode = $LASTEXITCODE
    } finally {
        Pop-Location
    }
    if ($statusExitCode -ne 0) { throw "sun status after init failed`n$statusText" }
    $initializedStatus = $statusText | ConvertFrom-Json
    if ($initializedStatus.ok -ne $true) {
        throw "sun status after init returned failure: $statusText"
    }
    $baseCheckpoint = [string]$initializedStatus.data.repository.base_checkpoint_id
    $baseView = [string]$initializedStatus.data.ids.resolved_view_id
    if ([string]::IsNullOrWhiteSpace($baseCheckpoint) -or
        [string]::IsNullOrWhiteSpace($baseView)) {
        throw "sun status after init omitted base checkpoint or resolved view identity"
    }
    $sunlightRoot = Join-Path $fixture '.sunlight'
    $initialSunlightBytes = Get-TreeBytes $sunlightRoot
    $initialNativeStateBytes = Get-NativeStateBytes $sunlightRoot

    for ($i = 0; $i -lt 4; $i++) {
        $clients += ,(New-McpClient ($i + 1))
    }

    $topicCalls = @()
    for ($i = 0; $i -lt 4; $i++) {
        $topicCalls += Send-McpCall $clients[$i] 'topic_create' @{
            slug = "oa07-worker-$($i + 1)"
            display_name = "OA-07 worker $($i + 1)"
            owner = "oa07-agent-$($i + 1)"
            visibility = 'local'
            acceptance_criteria = @('scale acceptance evidence remains exact')
        }
    }
    $authors = @()
    for ($i = 0; $i -lt 4; $i++) {
        $call = Receive-McpCall $clients[$i] $topicCalls[$i]
        Add-Sample 'mutation' $call
        $topicId = [string]$call.Envelope.data.ids.topic_id
        $sessionCall = Invoke-McpCall $clients[$i] 'session_start' @{
            topic = $topicId; view = $baseView; actor = "oa07-agent-$($i + 1)"
        }
        Add-Sample 'mutation' $sessionCall
        $authors += [pscustomobject]@{
            Client = $clients[$i]
            TopicId = $topicId
            SessionId = [string]$sessionCall.Envelope.data.ids.session_id
            RevisionId = [string]$sessionCall.Envelope.data.ids.topic_revision_id
        }
    }

    for ($round = 1; $round -le 5; $round++) {
        $writeCalls = @()
        for ($i = 0; $i -lt 4; $i++) {
            $writeCalls += Send-McpCall $authors[$i].Client 'artifact_write' @{
                path = "sunlight_alpha/worker-$($i + 1)-$round.txt"
                session = $authors[$i].SessionId
                expect_hash = 'new'
                content = "OA-07 worker $($i + 1), revision $round`n"
                classification = 'source'
            }
        }
        for ($i = 0; $i -lt 4; $i++) {
            $call = Receive-McpCall $authors[$i].Client $writeCalls[$i]
            Add-Sample 'mutation' $call
            $authors[$i].RevisionId = [string]$call.Envelope.data.ids.topic_revision_id
        }
    }

    for ($i = 0; $i -lt 4; $i++) {
        $complete = Invoke-McpCall $authors[$i].Client 'topic_complete' @{
            topic = $authors[$i].TopicId
            revision = $authors[$i].RevisionId
            session = $authors[$i].SessionId
            summary = "OA-07 worker $($i + 1) completed five independent artifacts"
        }
        Add-Sample 'mutation' $complete
    }

    $includes = @($authors | ForEach-Object {
        @{ topic = $_.TopicId; revision = $_.RevisionId }
    })
    $resolveArgs = @{ base = $baseCheckpoint; include = $includes }
    $resolved = $null
    for ($i = 0; $i -lt 10; $i++) {
        $call = Invoke-McpCall $clients[0] 'view_resolve' $resolveArgs
        Add-Sample 'resolution' $call
        $resolved = $call
        if ($i -eq 0) {
            $comparableSunlightActionCount = $mcpToolCallCount + 1
            $comparableSunlightJourney.Stop()
        }
    }
    $resolvedView = [string]$resolved.Envelope.data.ids.resolved_view_id

    for ($i = 0; $i -lt 20; $i++) {
        $call = Invoke-McpCall $clients[0] 'repository_status' @{}
        Add-Sample 'status' $call
        $call = Invoke-McpCall $clients[0] 'artifact_list' @{ prefix = 'lib'; view = $resolvedView }
        Add-Sample 'list' $call
        $call = Invoke-McpCall $clients[0] 'artifact_search' @{ query = 'defmodule'; view = $resolvedView }
        Add-Sample 'search' $call
        $call = Invoke-McpCall $clients[0] 'artifact_read' @{ path = 'README.md'; view = $resolvedView }
        Add-Sample 'read' $call
    }

    $queueIds = @()
    for ($i = 0; $i -lt 4; $i++) {
        $queueIds += Send-McpCall $clients[0] 'repository_status' @{}
    }
    foreach ($id in $queueIds) {
        $call = Receive-McpCall $clients[0] $id
        Add-Sample 'queue_burst' $call
    }

    $firstProjection = Invoke-McpCall $clients[0] 'project_materialize' @{
        view = $resolvedView; purpose = 'inspection'
    }
    Add-Sample 'projection' $firstProjection
    $cachedProjection = Invoke-McpCall $clients[0] 'project_materialize' @{
        view = $resolvedView; purpose = 'inspection'
    }
    Add-Sample 'projection' $cachedProjection

    $executions = @()
    $comparableSunlightJourney.Start()
    for ($i = 0; $i -lt 2; $i++) {
        $execution = Invoke-McpCall $clients[0] 'execution_run' @{
            view = $resolvedView
            program = 'bun'
            args = @('test', './src/lib')
            cwd = 'assets/control'
            network = 'not_enforced'
        }
        if ($execution.Envelope.data.result.status -ne 'pass') {
            throw "Exact-view target test did not pass: $($execution.Envelope | ConvertTo-Json -Depth 30)"
        }
        Add-Sample 'execution' $execution
        $executions += ,$execution
        if ($i -eq 0) {
            $comparableSunlightActionCount++
            $comparableSunlightJourney.Stop()
        }
    }

    $checkpoints = @()
    $comparableSunlightJourney.Start()
    for ($i = 0; $i -lt 2; $i++) {
        $checkpoint = Invoke-McpCall $clients[0] 'checkpoint_create' @{
            view = $resolvedView
            execution = [string]$executions[$i].Envelope.data.ids.execution_id
        }
        Add-Sample 'checkpoint' $checkpoint
        $checkpoints += ,$checkpoint
        if ($i -eq 0) {
            $comparableSunlightActionCount++
            $comparableSunlightJourney.Stop()
        }
    }

    $journey.Stop()
    $finalSunlightBytes = Get-TreeBytes $sunlightRoot
    $finalNativeStateBytes = Get-NativeStateBytes $sunlightRoot
    $growthBytes = $finalNativeStateBytes - $initialNativeStateBytes
    $latency = [ordered]@{}
    foreach ($operation in @('status', 'list', 'search', 'read', 'mutation', 'resolution')) {
        $latency[$operation] = Get-LatencySummary $operation
    }
    $queueSamples = @($samples | Where-Object operation -eq 'queue_burst')
    $allRetries = @($samples | ForEach-Object automatic_concurrency_retries)
    $maxRetries = if ($allRetries.Count) { ($allRetries | Measure-Object -Maximum).Maximum } else { 0 }
    $queueValues = [double[]]@($queueSamples | ForEach-Object queue_ms)

    $firstMaterialization = $firstProjection.Envelope.data.materialization
    $cachedMaterialization = $cachedProjection.Envelope.data.materialization
    $firstElapsed = [double]$firstMaterialization.elapsed_ms
    $cachedElapsed = [double]$cachedMaterialization.elapsed_ms
    $cachedRatio = if ($firstElapsed -gt 0) { $cachedElapsed / $firstElapsed } else { $null }
    $amplification = if ($null -ne $firstMaterialization.storage_amplification) {
        [double]$firstMaterialization.storage_amplification
    } else {
        $null
    }
    $executionMetrics = @($executions | ForEach-Object {
        [ordered]@{
            execution_id = $_.Envelope.data.ids.execution_id
            projection_id = $_.Envelope.data.ids.projection_id
            result = $_.Envelope.data.result.status
            external_ms = $_.ExternalMs
            transport = $_.Envelope.data.transport
            phase_timings_ms = $_.Envelope.data.phase_timings_ms
        }
    })
    $firstExactTestCommandMs = [double]$executions[0].Envelope.data.phase_timings_ms.command
    $conventionalTestMs = [double]$baselineTimer.Elapsed.TotalMilliseconds
    $exactToConventionalTestRatio = if ($conventionalTestMs -gt 0) {
        $firstExactTestCommandMs / $conventionalTestMs
    } else {
        $null
    }
    $conventionalActionCount = $conventionalChangedPaths.Count + 1
    $fullSunlightActionCount = $mcpToolCallCount + 1
    $actionCountRatio = $comparableSunlightActionCount / $conventionalActionCount
    $sunlightToConventionalJourneyRatio = if ($baselineJourneyTimer.Elapsed.TotalMilliseconds -gt 0) {
        $comparableSunlightJourney.Elapsed.TotalMilliseconds / $baselineJourneyTimer.Elapsed.TotalMilliseconds
    } else {
        $null
    }
    $sourceStatusAfter = @(Invoke-Checked 'read final target status' { git -C $TargetRepo status --short })
    $sourceCommitAfter = (Invoke-Checked 'read final target commit' { git -C $TargetRepo rev-parse HEAD } | Select-Object -Last 1).Trim()
    $fixtureCommitAfter = (Invoke-Checked 'read final fixture commit' { git -C $fixture rev-parse HEAD } | Select-Object -Last 1).Trim()
    $fixtureRemotesAfter = @(Invoke-Checked 'read final disposable remotes' { git -C $fixture remote })
    $sourceStatusUnchanged = (($sourceStatusAfter -join "`n") -eq ($sourceStatus -join "`n"))

    $checks = [ordered]@{
        init_under_120s = $initTimer.Elapsed.TotalSeconds -le 120
        status_p95 = $latency.status.p95_ms -le 750
        list_p95 = $latency.list.p95_ms -le 1500
        search_p95 = $latency.search.p95_ms -le 2000
        read_p95 = $latency.read.p95_ms -le 750
        mutation_p95 = $latency.mutation.p95_ms -le 2000
        resolution_p95 = $latency.resolution.p95_ms -le 2000
        queue_p95 = (Get-Percentile $queueValues 0.95) -le 2000
        queue_max = ($queueValues | Measure-Object -Maximum).Maximum -le 10000
        retries_not_exhausted = $maxRetries -le 8
        first_projection = $firstElapsed -le 60000
        cached_projection = $firstElapsed -gt 0 -and $cachedElapsed -le 5000 -and
            $cachedRatio -le 0.25 -and $cachedMaterialization.cache_hit -eq $true -and
            $cachedMaterialization.integrity_revalidated -eq $true
        storage_amplification = $null -ne $amplification -and $amplification -le 1.25
        incremental_growth = $growthBytes -le 5MB
        journey_under_15m = $journey.Elapsed.TotalMinutes -le 15
        windows_host = $env:OS -eq 'Windows_NT'
        local_ssd_confirmed = $ConfirmLocalSsd.IsPresent
        target_file_count_in_scope = $trackedFileCount -ge 5000 -and $trackedFileCount -le 25000
        conventional_scope_matched = $conventionalChangedPaths.Count -eq 20 -and $baselineCleanupVerified
        four_distinct_sessions = @($authors | Select-Object -ExpandProperty SessionId -Unique).Count -eq 4
        no_remote = $fixtureRemotesAfter.Count -eq 0
        fixture_head_unchanged = $fixtureCommitAfter -eq $sourceCommit
        source_head_unchanged = $sourceCommitAfter -eq $sourceCommit
        source_status_unchanged = $sourceStatusUnchanged
    }
    $passed = @($checks.Values | Where-Object { $_ -ne $true }).Count -eq 0

    $evidence = [ordered]@{
        schema = 'sunlight.open-alpha.oa07.v1'
        result = if ($passed) { 'pass' } else { 'fail' }
        recorded_at_utc = [DateTime]::UtcNow.ToString('o')
        threshold_source = 'docs/acceptance/open_alpha_thresholds.md'
        sunlight = [ordered]@{
            source_commit = (& git -C $repoRoot rev-parse HEAD).Trim()
            executable = $sun
            executable_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $sun).Hash.ToLowerInvariant()
            source_worktree_status = @(& git -C $repoRoot status --short)
        }
        platform = [ordered]@{
            os = [Environment]::OSVersion.VersionString
            powershell = $PSVersionTable.PSVersion.ToString()
            filesystem_root = [System.IO.Path]::GetPathRoot($fixture)
            filesystem = ([System.IO.DriveInfo]::new([System.IO.Path]::GetPathRoot($fixture))).DriveFormat
            local_ssd_operator_confirmed = $ConfirmLocalSsd.IsPresent
        }
        target = [ordered]@{
            source_path = [System.IO.Path]::GetFullPath($TargetRepo)
            source_commit = $sourceCommit
            initial_status = $sourceStatus
            final_status = $sourceStatusAfter
            tracked_files = $trackedFileCount
            logical_tracked_bytes = $logicalTrackedBytes
            disposable_path = $fixture
            remotes_after_clone = $fixtureRemotes
            remotes_final = $fixtureRemotesAfter
        }
        conventional_baseline = [ordered]@{
            clone_ms = [math]::Round($cloneTimer.Elapsed.TotalMilliseconds, 3)
            authoring_ms = [math]::Round($baselineAuthoringTimer.Elapsed.TotalMilliseconds, 3)
            test_ms = [math]::Round($baselineTimer.Elapsed.TotalMilliseconds, 3)
            journey_ms = [math]::Round($baselineJourneyTimer.Elapsed.TotalMilliseconds, 3)
            repository_bytes = $conventionalBytes
            changed_path_count = $conventionalChangedPaths.Count
            changed_paths = $conventionalChangedPaths
            action_count = $conventionalActionCount
            action_scope = '20 direct working-tree writes and one target test; clone and verified cleanup are harness setup/teardown'
            cleanup_verified_before_sunlight_init = $baselineCleanupVerified
        }
        conventional_comparison = [ordered]@{
            conventional_test_external_ms = [math]::Round($conventionalTestMs, 3)
            exact_view_first_test_command_ms = [math]::Round($firstExactTestCommandMs, 3)
            exact_view_first_to_conventional_test_ratio = if ($null -ne $exactToConventionalTestRatio) {
                [math]::Round($exactToConventionalTestRatio, 6)
            } else {
                $null
            }
            timing_basis = 'same bun test command; conventional external wall time versus Sunlight execution command phase'
            sunlight_journey_ms = [math]::Round($comparableSunlightJourney.Elapsed.TotalMilliseconds, 3)
            conventional_journey_ms = [math]::Round($baselineJourneyTimer.Elapsed.TotalMilliseconds, 3)
            sunlight_to_conventional_journey_ratio = if ($null -ne $sunlightToConventionalJourneyRatio) {
                [math]::Round($sunlightToConventionalJourneyRatio, 6)
            } else {
                $null
            }
            sunlight_action_count = $comparableSunlightActionCount
            conventional_action_count = $conventionalActionCount
            sunlight_to_conventional_action_ratio = [math]::Round($actionCountRatio, 6)
            action_count_comparable = $true
            action_count_basis = 'Sunlight post-ingest status plus MCP tool calls versus 20 direct writes plus one conventional target test'
        }
        sunlight_run = [ordered]@{
            init_ms = [math]::Round($initTimer.Elapsed.TotalMilliseconds, 3)
            journey_ms = [math]::Round($journey.Elapsed.TotalMilliseconds, 3)
            base_checkpoint_id = $baseCheckpoint
            resolved_view_id = $resolvedView
            topics = @($authors | ForEach-Object TopicId)
            sessions = @($authors | ForEach-Object SessionId)
            revisions = @($authors | ForEach-Object RevisionId)
            executions = @($executions | ForEach-Object { $_.Envelope.data.ids.execution_id })
            execution_metrics = $executionMetrics
            checkpoints = @($checkpoints | ForEach-Object { $_.Envelope.data.ids.checkpoint_id })
            latency = $latency
            queue = [ordered]@{
                p50_ms = Get-Percentile $queueValues 0.50
                p95_ms = Get-Percentile $queueValues 0.95
                max_ms = [math]::Round(($queueValues | Measure-Object -Maximum).Maximum, 3)
            }
            maximum_automatic_concurrency_retries = $maxRetries
            first_projection = $firstMaterialization
            cached_projection = $cachedMaterialization
            cached_elapsed_ratio = if ($null -ne $cachedRatio) {
                [math]::Round($cachedRatio, 6)
            } else {
                $null
            }
            initial_sunlight_bytes = $initialSunlightBytes
            final_sunlight_bytes = $finalSunlightBytes
            initial_native_state_bytes = $initialNativeStateBytes
            final_native_state_bytes = $finalNativeStateBytes
            incremental_native_state_bytes = $growthBytes
            total_sunlight_growth_bytes = $finalSunlightBytes - $initialSunlightBytes
            mcp_tool_call_count = $mcpToolCallCount
            post_ingest_cli_domain_call_count = 1
            initialization_setup_call_count = 1
            comparable_action_count = $comparableSunlightActionCount
            full_benchmark_action_count = $fullSunlightActionCount
            authoring_mode = 'four native MCP authors sharing one canonical repository; no per-author projection requested'
        }
        checks = $checks
        sample_count = $samples.Count
        samples = $samples
        safety = [ordered]@{
            target_source_modified = !$sourceStatusUnchanged -or $sourceCommitAfter -ne $sourceCommit
            disposable_remote_removed = $fixtureRemotesAfter.Count -eq 0
            fixture_head_changed = $fixtureCommitAfter -ne $sourceCommit
            push_performed = $false
        }
    }

    $evidenceDirectory = Split-Path -Parent $EvidencePath
    New-Item -ItemType Directory -Path $evidenceDirectory -Force | Out-Null
    $evidence | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath $EvidencePath -Encoding utf8
    Write-Output "OA-07 result: $($evidence.result)"
    Write-Output "Evidence: $([System.IO.Path]::GetFullPath($EvidencePath))"
    if (!$passed) {
        throw "OA-07 thresholds failed; evidence was retained at $EvidencePath"
    }
} finally {
    foreach ($client in $clients) { Stop-McpClient $client }
    if (!$KeepFixture -and (Test-Path -LiteralPath $tempRoot)) {
        $resolvedTempRoot = [System.IO.Path]::GetFullPath($tempRoot)
        if (!$resolvedTempRoot.StartsWith($tempBase, [StringComparison]::OrdinalIgnoreCase) -or
            !(Split-Path -Leaf $resolvedTempRoot).StartsWith('sun-oa07-')) {
            throw "Refusing to remove unexpected fixture path: $resolvedTempRoot"
        }
        $cleanupPath = if ($env:OS -eq 'Windows_NT') { '\\?\' + $resolvedTempRoot } else { $resolvedTempRoot }
        Get-ChildItem -LiteralPath $cleanupPath -Recurse -Force -File -ErrorAction SilentlyContinue |
            ForEach-Object { try { $_.IsReadOnly = $false } catch {} }
        $cleanupError = $null
        for ($attempt = 1; $attempt -le 3 -and (Test-Path -LiteralPath $resolvedTempRoot); $attempt++) {
            try {
                Remove-Item -LiteralPath $cleanupPath -Recurse -Force -ErrorAction Stop
                $cleanupError = $null
            } catch {
                $cleanupError = $_
                if ($attempt -lt 3) { Start-Sleep -Milliseconds 250 }
            }
        }
        if (Test-Path -LiteralPath $resolvedTempRoot) {
            Write-Warning "Disposable OA-07 fixture cleanup was incomplete after three bounded attempts: $cleanupError"
        }
    }
}
