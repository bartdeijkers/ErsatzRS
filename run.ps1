# ErsatzRS dev entry point (Windows / PowerShell 7+). See BUILD.md.
#
# Usage:
#   .\run.ps1           # alias for `dev`
#   .\run.ps1 dev       # build scanner companion + cargo leptos watch
#   .\run.ps1 check     # fast inner-loop type-check
#   .\run.ps1 test      # focused/local Rust tests through cargo-nextest
#   .\run.ps1 test-closeout # workspace nextest run plus Cargo doctests
#   .\run.ps1 build     # one-shot build + run (CI parity)
#   .\run.ps1 serve     # rebuild once and serve (no watch loop)
#   .\run.ps1 seed      # apply migrations + seed admin fixture data
#   .\run.ps1 log       # tail ersatzrs.log

[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [ValidateSet('dev', 'check', 'test', 'test-closeout', 'build', 'serve', 'seed', 'log')]
    [string]$Command = 'dev',

    [Parameter(Position = 1, ValueFromRemainingArguments = $true)]
    [string[]]$CommandArgs = @()
)

$ErrorActionPreference = 'Stop'
Set-Location -Path $PSScriptRoot

# Default to a Windows-conventional FFmpeg 8.1 install if present. Users with
# a different layout should set ErsatzRS_FFMPEG__PATH / ErsatzRS_FFPROBE__PATH
# (or put ffmpeg on PATH) before running this script.
function Set-LocalFfmpegOverrides {
    $ffmpegRoot = Join-Path $HOME '.local\ffmpeg-8.1'
    $ffmpegBin = Join-Path $ffmpegRoot 'bin\ffmpeg.exe'
    $ffprobeBin = Join-Path $ffmpegRoot 'bin\ffprobe.exe'
    if ((Test-Path $ffmpegBin) -and -not $env:ErsatzRS_FFMPEG__PATH) {
        $env:ErsatzRS_FFMPEG__PATH = $ffmpegBin
    }
    if ((Test-Path $ffprobeBin) -and -not $env:ErsatzRS_FFPROBE__PATH) {
        $env:ErsatzRS_FFPROBE__PATH = $ffprobeBin
    }
}

function Resolve-TargetDir {
    if ($env:CARGO_TARGET_DIR) { return $env:CARGO_TARGET_DIR }

    $configPath = '.cargo/config.toml'
    if (Test-Path $configPath) {
        $inBuild = $false
        foreach ($line in Get-Content $configPath) {
            if ($line -match '^\s*\[') {
                $inBuild = $line -match '^\s*\[build\]'
                continue
            }
            if ($inBuild -and $line -match '^\s*target-dir\s*=\s*[''"]?([^''"]+)[''"]?\s*$') {
                return $Matches[1].Trim()
            }
        }
    }

    return 'target'
}

function Test-TargetSize {
    $targetDir = Resolve-TargetDir
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
        Set-LocalFfmpegOverrides
        Set-Location -Path 'crates/ersatzrs-web'
        cargo build --package ersatzrs-scanner --bin ersatzrs-scanner
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        cargo leptos watch
        exit $LASTEXITCODE
    }
    'check' {
        Test-TargetSize
        if ($CommandArgs.Count -eq 0) {
            $CommandArgs = @('--workspace')
        }
        cargo check @CommandArgs
        exit $LASTEXITCODE
    }
    'test' {
        Test-TargetSize
        if ($CommandArgs.Count -eq 0) {
            Write-Error 'usage: .\run.ps1 test -p <crate> [nextest args or test filter]'
            exit 2
        }
        if (-not (Get-Command cargo-nextest -ErrorAction SilentlyContinue)) {
            Write-Error 'cargo-nextest is required; see BUILD.md installation instructions'
            exit 1
        }
        cargo nextest run @CommandArgs
        exit $LASTEXITCODE
    }
    'test-closeout' {
        Test-TargetSize
        if ($CommandArgs.Count -ne 0) {
            Write-Error 'usage: .\run.ps1 test-closeout'
            exit 2
        }
        if (-not (Get-Command cargo-nextest -ErrorAction SilentlyContinue)) {
            Write-Error 'cargo-nextest is required; see BUILD.md installation instructions'
            exit 1
        }
        cargo nextest run --workspace --all-features
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        cargo test --workspace --all-features --doc
        exit $LASTEXITCODE
    }
    'build' {
        Test-TargetSize
        Set-LocalFfmpegOverrides
        Push-Location 'crates/ersatzrs-web'
        try {
            cargo build --package ersatzrs-scanner --bin ersatzrs-scanner
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
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
        Set-LocalFfmpegOverrides
        Set-Location -Path 'crates/ersatzrs-web'
        cargo build --package ersatzrs-scanner --bin ersatzrs-scanner
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
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
