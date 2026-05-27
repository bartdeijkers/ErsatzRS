# ErsatzRS dev entry point (Windows / PowerShell 7+). See BUILD.md.
#
# Usage:
#   .\run.ps1           # alias for `dev`
#   .\run.ps1 dev       # cargo leptos watch (hot-reload)
#   .\run.ps1 build     # one-shot build + run (CI parity)
#   .\run.ps1 serve     # rebuild once and serve (no watch loop)
#   .\run.ps1 seed      # apply migrations + seed admin fixture data
#   .\run.ps1 log       # tail ersatzrs.log

[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [ValidateSet('dev', 'build', 'serve', 'seed', 'log')]
    [string]$Command = 'dev'
)

$ErrorActionPreference = 'Stop'
Set-Location -Path $PSScriptRoot

# Default to a Windows-conventional FFmpeg 8.1 install if present. Users with
# a different layout should set ErsatzRS_FFMPEG__PATH / ErsatzRS_FFPROBE__PATH
# (or put ffmpeg on PATH) before running this script.
$ffmpegRoot = Join-Path $HOME '.local\ffmpeg-8.1'
$ffmpegBin = Join-Path $ffmpegRoot 'bin\ffmpeg.exe'
$ffprobeBin = Join-Path $ffmpegRoot 'bin\ffprobe.exe'
if ((Test-Path $ffmpegBin) -and -not $env:ErsatzRS_FFMPEG__PATH) {
    $env:ErsatzRS_FFMPEG__PATH = $ffmpegBin
}
if ((Test-Path $ffprobeBin) -and -not $env:ErsatzRS_FFPROBE__PATH) {
    $env:ErsatzRS_FFPROBE__PATH = $ffprobeBin
}

function Test-TargetSize {
    $targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { 'target' }
    if (-not (Test-Path $targetDir)) { return }

    $thresholdGiB = if ($env:ERSATZRS_TARGET_CLEAN_THRESHOLD_GIB) {
        [double]$env:ERSATZRS_TARGET_CLEAN_THRESHOLD_GIB
    } else {
        40
    }
    $thresholdBytes = $thresholdGiB * 1GB
    $sizeBytes = (Get-ChildItem -LiteralPath $targetDir -Recurse -Force -ErrorAction SilentlyContinue |
        Measure-Object -Property Length -Sum).Sum
    if ($null -eq $sizeBytes) { return }

    if ($sizeBytes -ge $thresholdBytes) {
        $sizeGiB = [math]::Round($sizeBytes / 1GB, 1)
        Write-Warning "$targetDir is $sizeGiB GiB; run 'cargo clean' before continuing large builds (threshold: $thresholdGiB GiB)"
    }
}

switch ($Command) {
    'dev' {
        Test-TargetSize
        Set-Location -Path 'crates/ersatzrs-web'
        cargo leptos watch
        exit $LASTEXITCODE
    }
    'build' {
        Test-TargetSize
        Push-Location 'crates/ersatzrs-web'
        try {
            cargo leptos build
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        } finally {
            Pop-Location
        }
        cargo run --bin ersatzrs-web 2>&1 | Tee-Object -FilePath 'ersatzrs.log'
        exit $LASTEXITCODE
    }
    'serve' {
        Test-TargetSize
        Set-Location -Path 'crates/ersatzrs-web'
        cargo leptos serve
        exit $LASTEXITCODE
    }
    'seed' {
        $dbPath = if ($env:ERSATZRS_DB_PATH) {
            $env:ERSATZRS_DB_PATH
        } else {
            Join-Path $HOME '.ersatzrs\ErsatzRS.db'
        }
        $fixture = if ($env:ERSATZRS_SEED_FIXTURE) {
            $env:ERSATZRS_SEED_FIXTURE
        } else {
            'tests/fixtures/seed_admin.sql'
        }
        if (-not (Test-Path $fixture)) {
            Write-Error "seed fixture not found: $fixture"
            exit 1
        }
        $dbDir = Split-Path -Parent $dbPath
        if ($dbDir -and -not (Test-Path $dbDir)) {
            New-Item -ItemType Directory -Path $dbDir -Force | Out-Null
        }
        Get-Content -Path $fixture -Raw | sqlite3 $dbPath
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        Write-Host "seeded $dbPath from $fixture"
    }
    'log' {
        Get-Content -Path 'ersatzrs.log' -Wait -Tail 200
    }
}
