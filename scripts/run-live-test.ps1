# Rose-Baileys Live WhatsApp Broadcast Test Runner
# Runs the isolated live test harness in crates/baileys-live-test

param(
    [string]$Phone = "6283162498175",
    [string]$Recipient = "",
    [switch]$BroadcastAll = $false,
    [switch]$Revoke = $false,
    [string]$Db = ".sessions/live_status_session.db",
    [switch]$Clean = $false,
    [switch]$Release = $false
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$WorkspaceRoot = Split-Path -Parent $ScriptDir

Set-Location $WorkspaceRoot

$ArgsList = @("run", "-p", "baileys-live-test", "--bin", "live-status-test")
if ($Release) {
    $ArgsList += "--release"
}
$ArgsList += "--"
$ArgsList += @("--phone", $Phone)

if ($Revoke) {
    $ArgsList += "--revoke"
}

if ($BroadcastAll) {
    $ArgsList += "--broadcast-all"
}

if (-not [string]::IsNullOrWhiteSpace($Recipient)) {
    $ArgsList += @("--recipient", $Recipient)
}

if (-not [string]::IsNullOrWhiteSpace($Db)) {
    $ArgsList += @("--db", $Db)
}

if ($Clean) {
    $ArgsList += "--clean"
}

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "   Rose-Baileys Isolated Live Test Runner                     " -ForegroundColor Cyan
Write-Host "   Phone: +$Phone                                            " -ForegroundColor Gray
Write-Host "   DB:    $Db                                                " -ForegroundColor Gray
if ($Revoke) {
    Write-Host "   Mode:  REVOKE / DELETE ONLY                               " -ForegroundColor Yellow
} elseif ($BroadcastAll) {
    Write-Host "   Mode:  BROADCAST TO ALL CONTACTS                          " -ForegroundColor Red
} else {
    Write-Host "   Mode:  SAFE TEST (Private to Self / Specified Contact)    " -ForegroundColor Green
}
Write-Host "============================================================" -ForegroundColor Cyan

cargo @ArgsList
