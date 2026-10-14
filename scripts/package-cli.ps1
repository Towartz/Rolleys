# Rose-Baileys Standalone Artifact Packaging & Checksum Generator
param(
    [string]$ArtifactsDir = "dist",
    [string]$Version = ""
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$WorkspaceRoot = Split-Path -Parent $ScriptDir
$DistPath = Join-Path $WorkspaceRoot $ArtifactsDir

if ([string]::IsNullOrWhiteSpace($Version)) {
    $CargoToml = Get-Content (Join-Path $WorkspaceRoot "Cargo.toml") -Raw
    if ($CargoToml -match 'version\s*=\s*"([^"]+)"') {
        $Version = $matches[1]
    } else {
        $Version = "0.1.0"
    }
}

Write-Host "[*] Packaging artifacts and generating SHA-256 in: $DistPath" -ForegroundColor Cyan

if (-not (Test-Path $DistPath)) {
    Write-Host "[-] Directory does not exist: $DistPath" -ForegroundColor Red
    exit 1
}

$ChecksumFile = Join-Path $DistPath "SHA256SUMS.txt"
$ChecksumLines = @()

$Files = Get-ChildItem -Path $DistPath -File | Where-Object { $_.Name -ne "SHA256SUMS.txt" }
foreach ($file in $Files) {
    $hash = (Get-FileHash -Path $file.FullName -Algorithm SHA256).Hash.ToLower()
    $ChecksumLines += "$hash  $($file.Name)"
    Write-Host "  • $($file.Name) -> $hash" -ForegroundColor Gray
}

$ChecksumLines | Out-File -FilePath $ChecksumFile -Encoding utf8 -Force
Write-Host "[+] SHA256SUMS.txt generated with $($ChecksumLines.Count) entries." -ForegroundColor Green
