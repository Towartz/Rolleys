# Rose-Baileys Android JNI & UniFFI Build Pipeline
# Compiles native .so libraries for arm64-v8a, armeabi-v7a, and x86_64,
# generates UniFFI Kotlin bindings, and packages them into AuroraChat_Android.

param(
    [string]$TargetAbi = "all", # "all", "arm64", "armv7", "x86_64"
    [switch]$Release = $true
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$WorkspaceRoot = Split-Path -Parent $ScriptDir
$AndroidJniDir = Join-Path $WorkspaceRoot "..\AuroraChat_Android\android\app\src\main\jniLibs"
$AndroidJavaDir = Join-Path $WorkspaceRoot "..\AuroraChat_Android\android\app\src\main\java"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "   Rose-Baileys Android UniFFI & JNI Build Automation       " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan

# 1. Build host uniffi-bindgen tool
Write-Host "[1/4] Building host uniffi-bindgen tool..." -ForegroundColor Yellow
Set-Location $WorkspaceRoot
cargo build -p baileys-ffi --bin uniffi-bindgen

# 2. Compile host library to generate Kotlin bindings
Write-Host "[2/4] Generating UniFFI Kotlin bindings..." -ForegroundColor Yellow
cargo build -p baileys-ffi
$HostDll = Join-Path $WorkspaceRoot "target\debug\baileys_ffi.dll"
if (-not (Test-Path $HostDll)) {
    $HostDll = Join-Path $WorkspaceRoot "target\debug\libbaileys_ffi.so"
}
if (-not (Test-Path $HostDll)) {
    $HostDll = Join-Path $WorkspaceRoot "target\debug\libbaileys_ffi.dylib"
}

cargo run -p baileys-ffi --bin uniffi-bindgen -- generate `
    --library $HostDll `
    --language kotlin `
    --no-format `
    --out-dir $AndroidJavaDir

Write-Host "  [+] Kotlin bindings updated in: $AndroidJavaDir\uniffi\baileys_ffi\" -ForegroundColor Green

# 3. Compile Android targets with cargo-ndk
$ProfileFlag = if ($Release) { "--release" } else { "" }
$SubDir = if ($Release) { "release" } else { "debug" }

$AbisToBuild = @()
switch ($TargetAbi.ToLower()) {
    "arm64"   { $AbisToBuild += @{ Target = "aarch64-linux-android"; Abi = "arm64-v8a" } }
    "armv7"   { $AbisToBuild += @{ Target = "armv7-linux-androideabi"; Abi = "armeabi-v7a" } }
    "x86_64"  { $AbisToBuild += @{ Target = "x86_64-linux-android"; Abi = "x86_64" } }
    "all"     {
        $AbisToBuild += @{ Target = "aarch64-linux-android"; Abi = "arm64-v8a" }
        $AbisToBuild += @{ Target = "armv7-linux-androideabi"; Abi = "armeabi-v7a" }
        $AbisToBuild += @{ Target = "x86_64-linux-android"; Abi = "x86_64" }
    }
}

Write-Host "[3/4] Compiling Android ABIs via cargo-ndk..." -ForegroundColor Yellow

# Check if cargo-ndk is installed
$HasCargoNdk = Get-Command "cargo-ndk" -ErrorAction SilentlyContinue

if ($HasCargoNdk) {
    foreach ($entry in $AbisToBuild) {
        $target = $entry.Target
        $abi = $entry.Abi
        Write-Host "  -> Building $target ($abi)..." -ForegroundColor Cyan
        
        cargo ndk --target $target -o $AndroidJniDir build -p baileys-ffi $ProfileFlag
        Write-Host "  [+] $abi library installed to $AndroidJniDir\$abi\libbaileys_ffi.so" -ForegroundColor Green
    }
} else {
    Write-Host "  [!] cargo-ndk not found on PATH. To build for Android devices, install it via:" -ForegroundColor Yellow
    Write-Host "      cargo install cargo-ndk" -ForegroundColor White
    Write-Host "      rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android" -ForegroundColor White
    Write-Host "      Ensure ANDROID_NDK_HOME is set in your environment." -ForegroundColor White
}

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "   Rose-Baileys Android UniFFI Setup Complete!              " -ForegroundColor Green
Write-Host "============================================================" -ForegroundColor Cyan
