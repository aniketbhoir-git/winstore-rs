# One command for the required tests (docs/designs/unattended-testing.md).
# Runs the UI build, L2 (UI, Vitest), then L1 (Rust). Writes src-tauri/target/test-report.md.
# Exit 0: each level passed. Exit 1: a level failed or a tool is absent.
# Never runs ignored tests (the real-winget test stays out).
$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
$ui = Join-Path $root 'ui'
$tauri = Join-Path $root 'src-tauri'
$report = Join-Path $tauri 'target/test-report.md'
$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if (Test-Path $cargoBin) { $env:Path = "$cargoBin;$env:Path" }

$lines = New-Object System.Collections.Generic.List[string]
$failed = $false

function Test-Tool([string]$name) { return [bool](Get-Command $name -ErrorAction SilentlyContinue) }

function Invoke-Step([string]$dir, [string]$exe, [string[]]$arguments) {
    Push-Location $dir
    try {
        $out = & $exe @arguments 2>&1 | ForEach-Object { "$_" }
        $code = $LASTEXITCODE
    } finally { Pop-Location }
    $out | ForEach-Object { Write-Host $_ }
    return @{ Code = $code; Output = ($out -join "`n") }
}

# --- UI build (the Tauri build embeds ui/dist) ---
$uiOk = $true
if (-not (Test-Tool 'npm')) {
    $lines.Add('L2 UI: fail (npm not found)'); $lines.Add('L1 Rust: fail (npm not found)'); $failed = $true; $uiOk = $false
} else {
    $nm = Join-Path $ui 'node_modules'
    $lock = Join-Path $ui 'package-lock.json'
    $needInstall = (-not (Test-Path $nm)) -or ((Get-Item $lock).LastWriteTimeUtc -gt (Get-Item $nm).LastWriteTimeUtc)
    if ($needInstall) {
        $r = Invoke-Step $ui 'npm' @('ci')
        if ($r.Code -ne 0) { $uiOk = $false }
        else { (Get-Item $nm).LastWriteTimeUtc = [DateTime]::UtcNow }
    }
    if ($uiOk) {
        $r = Invoke-Step $ui 'npm' @('run', 'build')
        if ($r.Code -ne 0) { $uiOk = $false }
    }
    if (-not $uiOk) { $lines.Add('L2 UI: fail (UI install or build failed)'); $lines.Add('L1 Rust: fail (UI build failed)'); $failed = $true }
}

# --- L2 UI ---
if ($uiOk) {
    $r = Invoke-Step $ui 'npm' @('test')
    if ($r.Code -eq 0) {
        $m = [regex]::Match($r.Output, '(?m)^\s*Tests\s+(\d+) passed')
        $n = if ($m.Success) { $m.Groups[1].Value } else { '?' }
        $lines.Add("L2 UI: pass ($n passed)")
    } else {
        $lines.Add('L2 UI: fail'); $failed = $true
        $names = [regex]::Matches($r.Output, '(?m)^\s*FAIL\s+(.+)$') | ForEach-Object { $_.Groups[1].Value.Trim() } | Select-Object -Unique
        foreach ($nm in $names) { $lines.Add("  - $nm") }
    }
}

# --- L1 Rust ---
if ($uiOk) {
    if (-not (Test-Tool 'cargo')) {
        $lines.Add('L1 Rust: fail (cargo not found)'); $failed = $true
    } else {
        $steps = @(
            @('fmt', '--all', '--', '--check'),
            @('clippy', '--all-targets', '--all-features', '--', '-D', 'warnings'),
            @('build', '--locked'),
            @('test', '--locked')
        )
        $l1Fail = $null
        $testOut = ''
        foreach ($s in $steps) {
            $r = Invoke-Step $tauri 'cargo' $s
            if ($s[0] -eq 'test') { $testOut = $r.Output }
            if ($r.Code -ne 0) { $l1Fail = "cargo $($s[0]) failed"; break }
        }
        if ($l1Fail) {
            $lines.Add("L1 Rust: fail ($l1Fail)"); $failed = $true
            foreach ($m in [regex]::Matches($testOut, '(?m)^test (\S+) \.\.\. FAILED')) {
                $lines.Add("  - $($m.Groups[1].Value)")
            }
        } else {
            $p = 0; $i = 0
            foreach ($m in [regex]::Matches($testOut, '(?m)^test result: .*? (\d+) passed;.*? (\d+) ignored')) {
                $p += [int]$m.Groups[1].Value; $i += [int]$m.Groups[2].Value
            }
            $lines.Add("L1 Rust: pass ($p passed, $i ignored)")
        }
    }
}

New-Item -ItemType Directory -Force -Path (Split-Path $report) | Out-Null
$text = ($lines -join "`n") + "`n"
Set-Content -Path $report -Value $text -Encoding utf8
Write-Host "`n--- test report ---`n$text"
if ($env:GITHUB_STEP_SUMMARY) { Add-Content -Path $env:GITHUB_STEP_SUMMARY -Value "## Test report`n`n$text" }
if ($failed) { exit 1 } else { exit 0 }
