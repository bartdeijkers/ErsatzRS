#requires -Version 7
# Synthetic FFmpeg proof only; never reads or changes a server database.
param(
    [switch]$PrepareOnly,
    [string]$WorkingDirectory,
    [string]$ManifestPath,
    [string]$FfmpegPath,
    [string]$ReportPath
)
$ErrorActionPreference = 'Stop'
if ($PrepareOnly) {
    $root = Join-Path ([IO.Path]::GetTempPath()) ('ersatzrs-drawvg-' + [Guid]::NewGuid().ToString('N'))
    [IO.Directory]::CreateDirectory($root) | Out-Null
    [IO.File]::WriteAllText((Join-Path $root 'owned-by-test'), 'drawvg-paths')
    Write-Output $root
    exit 0
}
if ([IO.Path]::GetFileName($WorkingDirectory) -notmatch '^ersatzrs-drawvg-[a-f0-9]{32}$' -or
    !(Test-Path -LiteralPath (Join-Path $WorkingDirectory 'owned-by-test')) -or
    [IO.File]::ReadAllText((Join-Path $WorkingDirectory 'owned-by-test')) -ne 'drawvg-paths') {
    throw 'The test must own its temporary directory'
}
try {
    $version = (& $FfmpegPath -version 2>$null | Select-Object -First 1)
    if ($version -notmatch '^ffmpeg version (n?9[.])') { throw 'FFmpeg 9 is required' }
    [IO.File]::WriteAllText((Join-Path $WorkingDirectory "label's [cut].txt"), 'OK', [Text.UTF8Encoding]::new($false))
    $results = @()
    foreach ($line in [IO.File]::ReadAllLines($ManifestPath)) {
        $path, $filter = $line.Split("`t", 2)
        $canonical = [IO.Path]::GetFullPath($path)
        if (!$canonical.StartsWith($WorkingDirectory.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
            throw 'Fixture path escapes the owned directory'
        }
        [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($canonical)) | Out-Null
        [IO.File]::WriteAllText($canonical, "setcolor red`nrect 8 8 24 24 fill`n", [Text.UTF8Encoding]::new($false))
        $rgbPath = Join-Path $WorkingDirectory 'decoded.rgb'
        $psi = [Diagnostics.ProcessStartInfo]::new($FfmpegPath)
        $psi.UseShellExecute = $false
        $psi.RedirectStandardError = $true
        $psi.RedirectStandardOutput = $true
        foreach ($argument in @('-nostdin','-hide_banner','-loglevel','error','-y','-f','lavfi','-i',
            'color=c=blue:s=64x64:r=1', '-vf', $filter, '-frames:v','1','-pix_fmt','rgb24','-f','rawvideo',$rgbPath)) {
            $psi.ArgumentList.Add($argument)
        }
        $process = [Diagnostics.Process]::Start($psi)
        $stderrTask = $process.StandardError.ReadToEndAsync()
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        if (!$process.WaitForExit(15000)) {
            $process.Kill($true)
            $process.WaitForExit()
            throw 'Render exceeded the 15-second bound'
        }
        $stderr = $stderrTask.GetAwaiter().GetResult()
        $null = $stdoutTask.GetAwaiter().GetResult()
        if ($process.ExitCode -ne 0) { throw "Render failed: $stderr" }
        $bytes = [IO.File]::ReadAllBytes($rgbPath)
        if ($bytes.Length -ne 64*64*3) { throw 'A complete decoded frame is required' }
        $marker = (16*64+16)*3
        $background = (48*64+48)*3
        if ($bytes[$marker] -lt 230 -or $bytes[$marker+1] -gt 25 -or $bytes[$marker+2] -gt 25) { throw 'Red marker absent' }
        if ($bytes[$background] -gt 25 -or $bytes[$background+1] -gt 25 -or $bytes[$background+2] -lt 230) { throw 'Blue source absent' }
        $white = 0
        for ($y=0; $y -lt 24; $y++) { for ($x=36; $x -lt 64; $x++) {
            $i = ($y*64+$x)*3
            if ($bytes[$i] -gt 200 -and $bytes[$i+1] -gt 200 -and $bytes[$i+2] -gt 200) { $white++ }
        }}
        if ($white -lt 5) { throw 'Following native Drawtext overlay is absent' }
        $results += @{white_text_pixels=$white; filter=$filter; decoded_bytes=$bytes.Length; marker='red'; background='blue'; exit_code=0}
        $process.Dispose()
    }
    if ($results.Count -ne 6) { throw 'All six Windows path cases are required' }
    $report = @{version=$version; cases=$results; live_state_changed=$false; passed=$true} | ConvertTo-Json -Depth 5
    if ($ReportPath) { [IO.File]::WriteAllText($ReportPath, $report) }
    Write-Output $report
} finally {
    Remove-Item -LiteralPath $WorkingDirectory -Recurse -Force
}
