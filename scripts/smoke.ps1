$ErrorActionPreference = 'Stop'
$script:DatabasePath = Join-Path ([System.IO.Path]::GetTempPath()) ("codexpt-smoke-{0}.sqlite3" -f [guid]::NewGuid())

function Invoke-CodeXpt([string[]]$Arguments) {
    $output = & cargo run --quiet -- --db $script:DatabasePath @Arguments
    if ($LASTEXITCODE -ne 0) { throw "codexpt failed ($LASTEXITCODE): $($output -join [Environment]::NewLine)" }
    return ($output -join [Environment]::NewLine)
}

try {
    Invoke-CodeXpt @('init') | Out-Null
    $task = Invoke-CodeXpt @('task', 'create', '--title', 'Smoke review', '--description', 'Synthetic end-to-end check') | ConvertFrom-Json
    $id = $task.task_id
    if (-not $id) { throw 'task create did not return task_id' }

    Invoke-CodeXpt @('message', 'send', $id, '--sender', 'agent:codex', '--recipient', 'agent:chatgpt', '--body', 'Synthetic proposal') | Out-Null
    Invoke-CodeXpt @('task', 'transition', $id, '--expect', 'open', '--to', 'proposed', '--actor', 'agent:codex') | Out-Null
    Invoke-CodeXpt @('task', 'transition', $id, '--expect', 'proposed', '--to', 'under_review', '--actor', 'agent:chatgpt') | Out-Null
    Invoke-CodeXpt @('task', 'transition', $id, '--expect', 'under_review', '--to', 'resolved', '--actor', 'agent:chatgpt') | Out-Null
    $decision = Invoke-CodeXpt @('task', 'request-decision', $id, '--question', 'Approve synthetic smoke test?') | ConvertFrom-Json
    Invoke-CodeXpt @('task', 'answer-decision', $id, $decision.decision_id, '--answer', 'Approved for this disposable test') | Out-Null
    $approved = Invoke-CodeXpt @('task', 'transition', $id, '--expect', 'resolved', '--to', 'approved', '--actor', 'human:cesar') | ConvertFrom-Json
    $events = Invoke-CodeXpt @('events', $id) | ConvertFrom-Json
    $messages = Invoke-CodeXpt @('message', 'list', $id) | ConvertFrom-Json

    if ($approved.status -ne 'approved') { throw "expected approved; got $($approved.status)" }
    if ($events.Count -lt 7) { throw "expected at least 7 audit events; got $($events.Count)" }
    if ($messages.Count -ne 1) { throw "expected one recorded message; got $($messages.Count)" }
    Write-Output "CLI smoke passed: task=$id, events=$($events.Count), messages=$($messages.Count)"
}
finally {
    foreach ($suffix in @('', '-shm', '-wal')) {
        $path = "$script:DatabasePath$suffix"
        if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force }
    }
}
