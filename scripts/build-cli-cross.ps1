# Rose-Baileys Multiplatform CLI Build Automation (PowerShell)
# Builds baileys-cli for Windows (x64, ARM64), Linux (musl, glibc), and macOS.

param(
    [string]$Target = "host",      # "host", "x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc", "x86_64-unknown-linux-musl", "x86_64-unknown-linux-gnu", "all-windows", "all"
    [switch]$Release = $true,
    [switch]$StaticCrt = $false,   # Enable static C runtime (+crt-static) on Windows MSVC
    [switch]$Package = $true,      # Automatically package into .zip / .tar.gz with SHA256 checksums
    [string]$OutDir = "dist",
    [string]$Version = ""
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$WorkspaceRoot = Split-Path -Parent $ScriptDir
$DistPath = Join-Path $WorkspaceRoot $OutDir

# Determine CLI version from Cargo.toml if not passed
if ([string]::IsNullOrWhiteSpace($Version)) {
    $CargoToml = Get-Content (Join-Path $WorkspaceRoot "Cargo.toml") -Raw
    if ($CargoToml -match 'version\s*=\s*"([^"]+)"') {
        $Version = $matches[1]
    } else {
        $Version = "0.1.0"
    }
}

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "   Rose-Baileys CLI Multiplatform Build Pipeline (v$Version) " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan

if (-not (Test-Path $DistPath)) {
    New-Item -ItemType Directory -Path $DistPath -Force | Out-Null
}

# Resolve Target List
$TargetsToBuild = @()
$HostTarget = "x86_64-pc-windows-msvc" # Default Windows host

switch ($Target.ToLower()) {
    "host" {
        $TargetsToBuild += @{ Triple = $HostTarget; OS = "windows"; Arch = "x86_64" }
    }
    "x86_64-pc-windows-msvc" {
        $TargetsToBuild += @{ Triple = "x86_64-pc-windows-msvc"; OS = "windows"; Arch = "x86_64" }
    }
    "aarch64-pc-windows-msvc" {
        $TargetsToBuild += @{ Triple = "aarch64-pc-windows-msvc"; OS = "windows"; Arch = "aarch64" }
    }
    "x86_64-unknown-linux-musl" {
        $TargetsToBuild += @{ Triple = "x86_64-unknown-linux-musl"; OS = "linux"; Arch = "x86_64-musl" }
    }
    "x86_64-unknown-linux-gnu" {
        $TargetsToBuild += @{ Triple = "x86_64-unknown-linux-gnu"; OS = "linux"; Arch = "x86_64-gnu" }
    }
    "aarch64-unknown-linux-musl" {
        $TargetsToBuild += @{ Triple = "aarch64-unknown-linux-musl"; OS = "linux"; Arch = "aarch64-musl" }
    }
    "aarch64-unknown-linux-gnu" {
        $TargetsToBuild += @{ Triple = "aarch64-unknown-linux-gnu"; OS = "linux"; Arch = "aarch64-gnu" }
    }
    "aarch64-linux-android" {
        $TargetsToBuild += @{ Triple = "aarch64-linux-android"; OS = "android"; Arch = "arm64" }
    }
    "android-arm64" {
        $TargetsToBuild += @{ Triple = "aarch64-linux-android"; OS = "android"; Arch = "arm64" }
    }
    "all-windows" {
        $TargetsToBuild += @{ Triple = "x86_64-pc-windows-msvc"; OS = "windows"; Arch = "x86_64" }
        $TargetsToBuild += @{ Triple = "aarch64-pc-windows-msvc"; OS = "windows"; Arch = "aarch64" }
    }
    "all" {
        $TargetsToBuild += @{ Triple = "x86_64-pc-windows-msvc"; OS = "windows"; Arch = "x86_64" }
        $TargetsToBuild += @{ Triple = "aarch64-pc-windows-msvc"; OS = "windows"; Arch = "aarch64" }
        $TargetsToBuild += @{ Triple = "x86_64-unknown-linux-musl"; OS = "linux"; Arch = "x86_64-musl" }
        $TargetsToBuild += @{ Triple = "x86_64-unknown-linux-gnu"; OS = "linux"; Arch = "x86_64-gnu" }
        $TargetsToBuild += @{ Triple = "aarch64-unknown-linux-musl"; OS = "linux"; Arch = "aarch64-musl" }
        $TargetsToBuild += @{ Triple = "aarch64-unknown-linux-gnu"; OS = "linux"; Arch = "aarch64-gnu" }
    }
    default {
        $TargetsToBuild += @{ Triple = $Target; OS = "custom"; Arch = "custom" }
    }
}

$ProfileFlag = if ($Release) { "--release" } else { "" }
$SubDir = if ($Release) { "release" } else { "debug" }

Set-Location $WorkspaceRoot

foreach ($t in $TargetsToBuild) {
    $triple = $t.Triple
    Write-Host "`n[*] Building target: $triple..." -ForegroundColor Yellow

    $EnvRustFlags = $env:RUSTFLAGS
    if ($t.OS -eq "windows" -and $StaticCrt) {
        $env:RUSTFLAGS = "$EnvRustFlags -C target-feature=+crt-static -C link-arg=/STACK:16777216"
    }

    try {
        $TargetDir = Join-Path $WorkspaceRoot "target"

        if ($triple -eq $HostTarget -or $triple -eq "host") {
            # Native host build
            if ($Release) {
                cargo build --target-dir $TargetDir -p baileys-cli --release
            } else {
                cargo build --target-dir $TargetDir -p baileys-cli
            }
            $BinarySource = Join-Path $WorkspaceRoot "target\$SubDir\baileys-cli.exe"
        } elseif ($triple -eq "x86_64-unknown-linux-musl") {
            Write-Host "  -> Using NDK Clang sysroot + rust-lld for pure Linux $triple..." -ForegroundColor Cyan
            $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
            $sysroot = "C:\Android\ndk\29.0.14206865\toolchains\llvm\prebuilt\windows-x86_64\sysroot"
            $env:CC_x86_64_unknown_linux_musl = "C:\Android\ndk\29.0.14206865\toolchains\llvm\prebuilt\windows-x86_64\bin\clang.exe"
            $env:AR_x86_64_unknown_linux_musl = "C:\Android\ndk\29.0.14206865\toolchains\llvm\prebuilt\windows-x86_64\bin\llvm-ar.exe"
            $env:CFLAGS_x86_64_unknown_linux_musl = "--target=x86_64-unknown-linux-musl --sysroot=$sysroot -I$sysroot\usr\include -I$sysroot\usr\include\x86_64-linux-android -D__errno=__errno_location"
            $env:RUSTFLAGS = "-C linker=rust-lld -C target-feature=+crt-static -C link-arg=--defsym=__errno=__errno_location"
            if ($Release) {
                cargo build --target-dir $TargetDir -p baileys-cli --target $triple --release
            } else {
                cargo build --target-dir $TargetDir -p baileys-cli --target $triple
            }
            $BinarySource = Join-Path $WorkspaceRoot "target\$triple\$SubDir\baileys-cli"
        } elseif ($t.OS -eq "android") {
            Write-Host "  -> Using 'cargo-ndk' for Android $triple..." -ForegroundColor Cyan
            $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
            cargo ndk --target $triple --platform 24 build --target-dir $TargetDir -p baileys-cli $ProfileFlag
            $BinarySource = Join-Path $WorkspaceRoot "target\$triple\$SubDir\baileys-cli"
        } else {
            # Cross build using cargo or cross if available
            $HasCross = Get-Command "cross" -ErrorAction SilentlyContinue
            $HasZigbuild = Get-Command "cargo-zigbuild" -ErrorAction SilentlyContinue

            if ($HasCross) {
                Write-Host "  -> Using Docker-based 'cross' for $triple..." -ForegroundColor Cyan
                if ($Release) {
                    cross build --target-dir $TargetDir -p baileys-cli --target $triple --release
                } else {
                    cross build --target-dir $TargetDir -p baileys-cli --target $triple
                }
            } elseif ($HasZigbuild) {
                Write-Host "  -> Using 'cargo-zigbuild' for $triple..." -ForegroundColor Cyan
                if ($Release) {
                    cargo zigbuild --target-dir $TargetDir -p baileys-cli --target $triple --release
                } else {
                    cargo zigbuild --target-dir $TargetDir -p baileys-cli --target $triple
                }
            } else {
                Write-Host "  -> Attempting standard 'cargo build --target $triple'..." -ForegroundColor Cyan
                if ($Release) {
                    cargo build --target-dir $TargetDir -p baileys-cli --target $triple --release
                } else {
                    cargo build --target-dir $TargetDir -p baileys-cli --target $triple
                }
            }
            $BinExt = if ($t.OS -eq "windows") { ".exe" } else { "" }
            $BinarySource = Join-Path $WorkspaceRoot "target\$triple\$SubDir\baileys-cli$BinExt"
        }

        if (Test-Path $BinarySource) {
            Write-Host "  [+] Build succeeded: $BinarySource" -ForegroundColor Green
            $BinSize = (Get-Item $BinarySource).Length / 1MB
            Write-Host ("  [+] Binary size: {0:N2} MB" -f $BinSize) -ForegroundColor Gray

            # Copy binary to dist folder
            $BinName = [System.IO.Path]::GetFileName($BinarySource)
            $DistBinName = "baileys-cli-$triple" + [System.IO.Path]::GetExtension($BinarySource)
            $DistBinPath = Join-Path $DistPath $DistBinName
            Copy-Item $BinarySource $DistBinPath -Force

            # Packaging
            if ($Package) {
                $ArchiveBaseName = "baileys-cli-v$Version-$triple"
                $StagingDir = Join-Path $DistPath "staging_$triple"
                if (Test-Path $StagingDir) { Remove-Item $StagingDir -Recurse -Force }
                New-Item -ItemType Directory -Path $StagingDir -Force | Out-Null

                # Copy binary as standard executable name inside archive
                Copy-Item $BinarySource (Join-Path $StagingDir $BinName) -Force
                
                # Copy documentation if available
                $ReadmePath = Join-Path $WorkspaceRoot "README.md"
                if (Test-Path $ReadmePath) {
                    Copy-Item $ReadmePath (Join-Path $StagingDir "README.md") -Force
                }

                if ($t.OS -eq "windows") {
                    $ZipFile = Join-Path $DistPath "$ArchiveBaseName.zip"
                    if (Test-Path $ZipFile) { Remove-Item $ZipFile -Force }
                    Compress-Archive -Path "$StagingDir\*" -DestinationPath $ZipFile -CompressionLevel Optimal
                    Write-Host "  [+] Packaged archive: $ZipFile" -ForegroundColor Green
                } else {
                    $TarFile = Join-Path $DistPath "$ArchiveBaseName.tar.gz"
                    if (Test-Path $TarFile) { Remove-Item $TarFile -Force }
                    tar -czf $TarFile -C $StagingDir .
                    Write-Host "  [+] Packaged archive: $TarFile" -ForegroundColor Green
                }

                Remove-Item $StagingDir -Recurse -Force
            }
        } else {
            Write-Host "  [-] Binary not found at expected path: $BinarySource" -ForegroundColor Red
        }
    } catch {
        Write-Host "  [-] Failed building target ${triple}: $_" -ForegroundColor Red
    } finally {
        $env:RUSTFLAGS = $EnvRustFlags
    }
}

# Generate unified SHA256SUMS.txt
if ($Package) {
    Write-Host "`n[*] Generating SHA256SUMS.txt..." -ForegroundColor Yellow
    $ChecksumFile = Join-Path $DistPath "SHA256SUMS.txt"
    $ChecksumLines = @()
    $Archives = Get-ChildItem -Path $DistPath -Include *.zip, *.tar.gz, *.exe -File -Recurse
    foreach ($file in $Archives) {
        if ($file.Name -ne "SHA256SUMS.txt") {
            $hash = (Get-FileHash -Path $file.FullName -Algorithm SHA256).Hash.ToLower()
            $ChecksumLines += "$hash  $($file.Name)"
        }
    }
    $ChecksumLines | Out-File -FilePath $ChecksumFile -Encoding utf8 -Force
    Write-Host "  [+] Checksums written to: $ChecksumFile" -ForegroundColor Green
}

Write-Host "`n============================================================" -ForegroundColor Cyan
Write-Host "   Rose-Baileys CLI Build Complete! Artifacts in: $OutDir    " -ForegroundColor Green
Write-Host "============================================================" -ForegroundColor Cyan
