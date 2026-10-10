$ErrorActionPreference = 'Stop'

$suffix = [guid]::NewGuid().ToString('N').Substring(0, 8)
$containerName = "codexpt-smoke-$suffix"
$dataVolume = "codexpt-smoke-data-$suffix"
$readOnlyVolume = "codexpt-smoke-readonly-$suffix"
$image = 'codexpt:release-candidate'
$oidcEnvironment = @(
    '-e', 'CODEXPT_OIDC_ISSUER=https://issuer.example',
    '-e', 'CODEXPT_OIDC_CLIENT_ID=synthetic-container-test',
    '-e', 'CODEXPT_OIDC_REDIRECT_URI=https://codexpt.example/api/auth/callback',
    '-e', 'CODEXPT_LOGIN_SUCCESS_URL=https://codexpt.example/',
    '-e', 'CODEXPT_API_HOST=codexpt.example',
    '-e', 'CODEXPT_WEB_ORIGIN=https://codexpt.example'
)

function Invoke-Docker {
    param(
        [Parameter(Mandatory)][string[]]$DockerArgs,
        [int]$ExpectedExitCode = 0
    )

    $output = & docker @DockerArgs 2>&1
    $exitCode = $LASTEXITCODE
    $text = $output -join "`n"
    if ($exitCode -ne $ExpectedExitCode) {
        throw "docker $($DockerArgs -join ' ') exited $exitCode; expected $ExpectedExitCode. Output: $text"
    }

    [pscustomobject]@{ ExitCode = $exitCode; Output = $text }
}

try {
    & docker build --tag $image .
    if ($LASTEXITCODE -ne 0) {
        throw 'Docker image build failed.'
    }

    $missingVolume = Invoke-Docker -ExpectedExitCode 1 -DockerArgs (@(
        'run', '--rm'
    ) + $oidcEnvironment + @($image))
    if ($missingVolume.Output -notmatch 'persistent volume must be mounted at /data') {
        throw 'The container did not fail closed when /data was not mounted.'
    }

    Invoke-Docker -DockerArgs @('volume', 'create', $readOnlyVolume) | Out-Null
    $readOnlyMount = Invoke-Docker -ExpectedExitCode 1 -DockerArgs (@(
        'run', '--rm', '--mount', "type=volume,source=$readOnlyVolume,target=/data,readonly"
    ) + $oidcEnvironment + @($image))
    if ($readOnlyMount.Output -notmatch 'unable to open database file') {
        throw 'The container did not fail closed when its database volume was read-only.'
    }

    Invoke-Docker -DockerArgs @('volume', 'create', $dataVolume) | Out-Null

    foreach ($invalidDatabasePath in @(
        'relative.sqlite3',
        '/tmp/codexpt.sqlite3',
        '/data/../tmp/codexpt.sqlite3'
    )) {
        $invalidPath = Invoke-Docker -ExpectedExitCode 1 -DockerArgs (@(
            'run', '--rm', '-v', "${dataVolume}:/data"
        ) + $oidcEnvironment + @('-e', "CODEXPT_DB_PATH=$invalidDatabasePath", $image))
        if ($invalidPath.Output -notmatch 'CODEXPT_DB_PATH must be a normalized path below /data') {
            throw "The container accepted an unsafe database path: $invalidDatabasePath"
        }
    }

    Invoke-Docker -DockerArgs @(
        'run', '--rm', '--user', '0', '--entrypoint', '/bin/sh',
        '-v', "${dataVolume}:/data", $image, '-c', 'ln -s /tmp /data/escape'
    ) | Out-Null
    $symlinkEscape = Invoke-Docker -ExpectedExitCode 1 -DockerArgs (@(
        'run', '--rm', '-v', "${dataVolume}:/data"
    ) + $oidcEnvironment + @('-e', 'CODEXPT_DB_PATH=/data/escape/codexpt.sqlite3', $image))
    if ($symlinkEscape.Output -notmatch 'CODEXPT_DB_PATH resolves outside the mounted /data volume') {
        throw 'The container accepted a database path that escapes /data through a symlink.'
    }

    $missingIdentity = Invoke-Docker -ExpectedExitCode 2 -DockerArgs @(
        'run', '--rm', '-v', "${dataVolume}:/data", $image
    )
    if ($missingIdentity.Output -notmatch 'CODEXPT_OIDC_ISSUER') {
        throw 'The container did not fail closed when OIDC configuration was missing.'
    }

    Invoke-Docker -DockerArgs (@(
        'run', '-d', '--read-only', '--tmpfs', '/tmp:rw,nosuid,size=16m',
        '--name', $containerName, '-v', "${dataVolume}:/data"
    ) + $oidcEnvironment + @($image)) | Out-Null

    $health = ''
    for ($attempt = 0; $attempt -lt 30; $attempt++) {
        $healthResult = Invoke-Docker -DockerArgs @(
            'inspect', '--format', '{{.State.Health.Status}}', $containerName
        )
        $health = $healthResult.Output.Trim()
        if ($health -eq 'healthy') { break }
        Start-Sleep -Seconds 2
    }
    if ($health -ne 'healthy') {
        throw "Container health check did not become healthy; last status: $health"
    }

    $runtimeUser = (Invoke-Docker -DockerArgs @(
        'inspect', '--format', '{{.Config.User}}', $containerName
    )).Output.Trim()
    if ($runtimeUser -ne '10001:10001') {
        throw "Unexpected container runtime user: $runtimeUser"
    }

    $ready = Invoke-Docker -DockerArgs @(
        'exec', $containerName, 'curl', '--fail', '--silent',
        '-H', 'Host: codexpt.example',
        'http://127.0.0.1:4173/api/health/ready'
    )
    if ($ready.Output -notmatch '"status":"ready"') {
        throw "Readiness did not report ready: $($ready.Output)"
    }

    $frontend = Invoke-Docker -DockerArgs @(
        'exec', $containerName, 'curl', '--fail', '--silent', '--output', '/dev/null',
        '--write-out', '%{http_code}', '-H', 'Host: codexpt.example', 'http://127.0.0.1:4173/'
    )
    if ($frontend.Output.Trim() -ne '200') {
        throw "Embedded frontend returned HTTP $($frontend.Output)"
    }

    Invoke-Docker -DockerArgs @(
        'exec', $containerName, '/usr/local/bin/codexpt', 'identity', 'bootstrap-owner',
        '--db', '/data/codexpt.sqlite3', '--issuer', 'https://issuer.example',
        '--subject', 'synthetic-smoke-subject', '--email', 'smoke@example.invalid',
        '--workspace-name', 'Container-Smoke'
    ) | Out-Null

    $baselineTaskIds = @()
    for ($index = 1; $index -le 8; $index++) {
        $createdTask = Invoke-Docker -DockerArgs @(
            'exec', $containerName, '/usr/local/bin/codexpt', '--db', '/data/codexpt.sqlite3',
            'task', 'create', '--title', "confirmed-smoke-$index", '--description', 'committed before SIGTERM'
        )
        $baselineTaskIds += ($createdTask.Output | ConvertFrom-Json).task_id
    }
    if (@($baselineTaskIds | Select-Object -Unique).Count -ne 8) {
        throw 'The deterministic SIGTERM baseline did not produce eight unique confirmed task IDs.'
    }

    $writer = 'i=1; while [ "$i" -le 50 ]; do /usr/local/bin/codexpt --db /data/codexpt.sqlite3 task create --title "shutdown-smoke-$i" --description synthetic >/dev/null 2>&1; if [ "$i" -eq 1 ]; then : > /tmp/writer-started; fi; i=$((i+1)); sleep 0.05; done'
    Invoke-Docker -DockerArgs @('exec', '-d', $containerName, '/bin/sh', '-c', $writer) | Out-Null
    $writerStarted = $false
    for ($attempt = 0; $attempt -lt 30; $attempt++) {
        & docker exec $containerName test -f /tmp/writer-started 2>$null | Out-Null
        $markerExitCode = $LASTEXITCODE
        if ($markerExitCode -notin @(0, 1)) {
            throw "Checking the concurrent writer marker failed with exit code $markerExitCode."
        }
        if ($markerExitCode -eq 0) {
            $writerStarted = $true
            break
        }
        Start-Sleep -Milliseconds 100
    }
    if (-not $writerStarted) {
        throw 'The concurrent task writer did not signal that it had started.'
    }
    Invoke-Docker -DockerArgs @('stop', '--timeout', '10', $containerName) | Out-Null

    $exitCode = (Invoke-Docker -DockerArgs @(
        'inspect', '--format', '{{.State.ExitCode}}', $containerName
    )).Output.Trim()
    if ($exitCode -ne '0') {
        throw "Container did not shut down cleanly on SIGTERM; exit code $exitCode"
    }

    Invoke-Docker -DockerArgs @('start', $containerName) | Out-Null
    Start-Sleep -Seconds 2
    $tasks = Invoke-Docker -DockerArgs @(
        'exec', $containerName, '/usr/local/bin/codexpt', '--db', '/data/codexpt.sqlite3', 'task', 'list'
    )
    $taskRows = @($tasks.Output | ConvertFrom-Json)
    $taskIds = @($taskRows | ForEach-Object { $_.task_id })
    if (@($taskIds | Select-Object -Unique).Count -ne $taskIds.Count) {
        throw 'The task database contains duplicate task IDs after SIGTERM and restart.'
    }
    foreach ($taskId in $baselineTaskIds) {
        if ($taskId -notin $taskIds) {
            throw "Confirmed task $taskId did not survive SIGTERM and container restart."
        }
    }
    if ($baselineTaskIds.Count -ne 8) {
        throw 'The confirmed SIGTERM baseline did not contain exactly eight tasks.'
    }
    $taskCount = $taskRows.Count

    $integrity = Invoke-Docker -DockerArgs @(
        'exec', $containerName, '/usr/local/bin/codexpt', '--db', '/data/codexpt.sqlite3', 'db-check'
    )
    if (($integrity.Output | ConvertFrom-Json).integrity -ne 'ok') {
        throw "SQLite integrity check failed after SIGTERM: $($integrity.Output)"
    }

    $duplicateBootstrap = Invoke-Docker -ExpectedExitCode 1 -DockerArgs @(
        'exec', $containerName, '/usr/local/bin/codexpt', 'identity', 'bootstrap-owner',
        '--db', '/data/codexpt.sqlite3', '--issuer', 'https://issuer.example',
        '--subject', 'synthetic-smoke-subject', '--email', 'smoke@example.invalid',
        '--workspace-name', 'Container-Smoke'
    )
    if ($duplicateBootstrap.Output -notmatch 'identity state conflicts') {
        throw 'Owner identity did not survive the restart or bootstrap was not idempotently denied.'
    }

    Invoke-Docker -DockerArgs @(
        'exec', $containerName, '/usr/local/bin/codexpt', 'backup',
        '--db', '/data/codexpt.sqlite3', '--output', '/data/smoke-backup.sqlite3'
    ) | Out-Null
    Invoke-Docker -DockerArgs @(
        'exec', $containerName, '/usr/local/bin/codexpt', 'restore',
        '--backup', '/data/smoke-backup.sqlite3', '--db', '/data/restored.sqlite3'
    ) | Out-Null
    $restoredTasks = Invoke-Docker -DockerArgs @(
        'exec', $containerName, '/usr/local/bin/codexpt', '--db', '/data/restored.sqlite3', 'task', 'list'
    )
    if (@($restoredTasks.Output | ConvertFrom-Json).Count -ne $taskCount) {
        throw 'Restored SQLite backup did not preserve the synthetic task set.'
    }
    $restoredIds = @(($restoredTasks.Output | ConvertFrom-Json) | ForEach-Object { $_.task_id })
    foreach ($taskId in $baselineTaskIds) {
        if ($taskId -notin $restoredIds) {
            throw "Confirmed task $taskId is missing from the restored backup."
        }
    }
    $restoredIntegrity = Invoke-Docker -DockerArgs @(
        'exec', $containerName, '/usr/local/bin/codexpt', '--db', '/data/restored.sqlite3', 'db-check'
    )
    if (($restoredIntegrity.Output | ConvertFrom-Json).integrity -ne 'ok') {
        throw "Restored SQLite database failed its integrity check: $($restoredIntegrity.Output)"
    }

    Write-Host "Container smoke passed: image=$image uid=$runtimeUser health=$health frontend=200 tasks=$taskCount backup=restored"
}
finally {
    & docker rm --force $containerName 2>$null | Out-Null
    & docker volume rm $dataVolume $readOnlyVolume 2>$null | Out-Null
}
