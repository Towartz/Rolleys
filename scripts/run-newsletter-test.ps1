# Rose-Baileys WhatsApp Newsletter / Channel Live Test Runner
# Runs the isolated channel test harness in crates/baileys-live-test

param(
    [string]$Db = ".sessions/live_status_session.db",
    [string]$Channel = "",
    [string]$Delete = "",
    [switch]$Keep = $false,
    [switch]$SkipMedia = $false,
    [switch]$Release = $false
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$WorkspaceRoot = Split-Path -Parent $ScriptDir

Set-Location $WorkspaceRoot

$ArgsList = @("run", "-p", "baileys-live-test", "--bin", "live-newsletter-test")
if ($Release) {
    $ArgsList += "--release"
}
$ArgsList += "--"

if (-not [string]::IsNullOrWhiteSpace($Db)) {
    $ArgsList += @("--db", $Db)
}

if (-not [string]::IsNullOrWhiteSpace($Channel)) {
    $ArgsList += @("--channel", $Channel)
}

if (-not [string]::IsNullOrWhiteSpace($Delete)) {
    $ArgsList += @("--delete", $Delete)
}

if ($Keep) {
    $ArgsList += "--keep"
}

if ($SkipMedia) {
    $ArgsList += "--skip-media"
}

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "   Rose-Baileys Newsletter / Channel Live Test Runner         " -ForegroundColor Cyan
Write-Host "   DB:       $Db                                             " -ForegroundColor Gray
if ([string]::IsNullOrWhiteSpace($Channel)) {
    Write-Host "   Channel:  Isolated Private Test Channel (Auto-Cleanup: $(-not $Keep))" -ForegroundColor Green
} else {
    Write-Host "   Channel:  $Channel                                        " -ForegroundColor Yellow
}
Write-Host "============================================================" -ForegroundColor Cyan

cargo @ArgsList
