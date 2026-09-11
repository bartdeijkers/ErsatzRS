# Offline command-contract tests: no GitHub writes or credentials required.
$ErrorActionPreference = 'Stop'
$stageScript = Join-Path $PSScriptRoot 'stage-beta-draft.ps1'
$cases = @(
    @{ Name = 'absent'; Json = '{"data":{"repository":{"release":null}}}'; Write = 'create' },
    @{ Name = 'draft'; Json = '{"data":{"repository":{"release":{"isDraft":true}}}}'; Write = 'upload' },
    @{ Name = 'published'; Json = '{"data":{"repository":{"release":{"isDraft":false}}}}'; Fails = $true },
    @{ Name = 'query failure'; Json = ''; QueryExit = 1; Fails = $true },
    @{ Name = 'partial query error'; Json = '{"data":{"repository":{"release":null}},"errors":[{"message":"unavailable"}]}'; Fails = $true },
    @{ Name = 'missing repository'; Json = '{"data":{"repository":null}}'; Fails = $true },
    @{ Name = 'invalid response'; Json = 'not json'; Fails = $true },
    @{ Name = 'create failure'; Json = '{"data":{"repository":{"release":null}}}'; Write = 'create'; WriteExit = 1; Fails = $true },
    @{ Name = 'upload failure'; Json = '{"data":{"repository":{"release":{"isDraft":true}}}}'; Write = 'upload'; WriteExit = 1; Fails = $true }
)

function gh {
    # Shadow the CLI for this script scope, including its native exit code.
    if ($args[0] -eq 'api') {
        $global:LASTEXITCODE = [int]$case.QueryExit
        return $case.Json
    }
    $writes.Add(@($args))
    $global:LASTEXITCODE = [int]$case.WriteExit
}

foreach ($case in $cases) {
    $writes = [System.Collections.Generic.List[object]]::new()
    $failed = $false
    try { & $stageScript -Repository 'example/project' -Tag 'v0.0.0-beta.1' -Archive 'candidate.zip' }
    catch {
        if (-not $case.Fails) { throw }
        $failed = $true
    }
    if ($failed -ne [bool]$case.Fails) { throw "$($case.Name): unexpected success/failure" }
    $expectedWrites = if ($case.Write) { 1 } else { 0 }
    if ($writes.Count -ne $expectedWrites) { throw "$($case.Name): unexpected mutation" }
    if ($case.Write) {
        $argv = $writes[0]
        if ($argv[0] -ne 'release' -or $argv[1] -ne $case.Write -or
            $argv[2] -ne 'v0.0.0-beta.1' -or $argv[3] -ne 'candidate.zip') {
            throw "$($case.Name): wrong release command"
        }
        if ($case.Write -eq 'create') {
            foreach ($flag in @('--draft', '--prerelease', '--verify-tag')) {
                if ($argv -notcontains $flag) { throw "Missing $flag" }
            }
        }
    }
    Write-Host "PASS: $($case.Name)"
}
# The last case deliberately simulates a failed native command. Do not leak
# that mock status into GitHub Actions' PowerShell exit-code wrapper.
exit 0
