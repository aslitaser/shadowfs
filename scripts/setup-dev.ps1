# ShadowFS Development Setup Script for Windows
# Sets up development environment for ShadowFS

param(
    [switch]$NoColor,
    [switch]$SkipGitHooks
)

# Set strict mode
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# Color functions
function Write-Step {
    param([string]$Message)
    if (-not $NoColor) {
        Write-Host "==> " -NoNewline -ForegroundColor Yellow
    }
    Write-Host $Message
}

function Write-Success {
    param([string]$Message)
    if (-not $NoColor) {
        Write-Host "✓ " -NoNewline -ForegroundColor Green
    }
    Write-Host $Message
}

function Write-Error {
    param([string]$Message)
    if (-not $NoColor) {
        Write-Host "✗ " -NoNewline -ForegroundColor Red
    }
    Write-Host $Message
}

function Write-Info {
    param([string]$Message)
    if (-not $NoColor) {
        Write-Host "ℹ " -NoNewline -ForegroundColor Blue
    }
    Write-Host $Message
}

# Check if running as administrator
function Test-Administrator {
    $currentUser = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($currentUser)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

# Check Windows version
function Test-WindowsVersion {
    $version = [System.Environment]::OSVersion.Version
    # Windows 10 version 1809 is 10.0.17763
    return ($version.Major -gt 10) -or 
           ($version.Major -eq 10 -and $version.Build -ge 17763)
}

# Install Windows dependencies
function Install-WindowsDeps {
    Write-Step "Checking Windows dependencies..."
    
    # Check Windows version for ProjFS support
    if (-not (Test-WindowsVersion)) {
        Write-Error "Windows 10 version 1809 or later is required for ProjFS"
        return $false
    }
    
    # Check if ProjFS is available
    Write-Info "Checking Projected File System availability..."
    try {
        $projFS = Get-WindowsOptionalFeature -Online -FeatureName "Client-ProjFS" -ErrorAction SilentlyContinue
        if ($projFS -and $projFS.State -eq "Enabled") {
            Write-Success "Projected File System is enabled"
        } else {
            Write-Info "Enabling Projected File System feature..."
            if (-not (Test-Administrator)) {
                Write-Error "Administrator privileges required to enable ProjFS"
                Write-Info "Please run this script as Administrator"
                return $false
            }
            Enable-WindowsOptionalFeature -Online -FeatureName "Client-ProjFS" -NoRestart
            Write-Success "Projected File System enabled (restart may be required)"
        }
    }
    catch {
        Write-Error "Failed to check/enable Projected File System"
        return $false
    }
    
    # Check for Visual Studio Build Tools
    Write-Info "Checking for C++ build tools..."
    $vsWhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vsWhere) {
        $vsInstalls = & $vsWhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -format json | ConvertFrom-Json
        if ($vsInstalls.Count -gt 0) {
            Write-Success "Visual Studio C++ tools found"
        } else {
            Write-Error "Visual Studio C++ build tools not found"
            Write-Info "Install from: https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022"
            return $false
        }
    } else {
        Write-Error "Visual Studio not found"
        Write-Info "Install Visual Studio or Build Tools from: https://visualstudio.microsoft.com/"
        return $false
    }
    
    return $true
}

# Check Rust installation
function Test-Rust {
    Write-Step "Checking Rust installation..."
    
    try {
        $null = Get-Command rustup -ErrorAction Stop
    }
    catch {
        Write-Error "Rustup not found"
        Write-Info "Install Rust from: https://rustup.rs"
        return $false
    }
    
    # Ensure stable toolchain is installed
    rustup install stable
    rustup default stable
    
    # Install required components
    rustup component add rustfmt clippy
    
    # Set Windows GNU ABI if needed
    rustup target add x86_64-pc-windows-msvc
    
    Write-Success "Rust toolchain configured"
    return $true
}

# Setup git hooks
function Setup-GitHooks {
    if ($SkipGitHooks) {
        Write-Info "Skipping git hooks setup"
        return
    }
    
    Write-Step "Setting up git hooks..."
    
    if (-not (Test-Path .git)) {
        Write-Info "Not in a git repository, skipping hooks setup"
        return
    }
    
    # Create hooks directory
    New-Item -ItemType Directory -Force -Path .git\hooks | Out-Null
    
    # Create pre-commit hook
    $preCommitHook = @'
#!/bin/sh
# Pre-commit hook for ShadowFS

# Run format check
echo "Running format check..."
cargo fmt --all -- --check || {
    echo "Format check failed. Run 'cargo fmt' to fix."
    exit 1
}

# Run clippy
echo "Running clippy..."
cargo clippy --all-targets --all-features -- -D warnings || {
    echo "Clippy check failed."
    exit 1
}

echo "Pre-commit checks passed!"
'@
    
    Set-Content -Path .git\hooks\pre-commit -Value $preCommitHook -Encoding UTF8
    Write-Success "Git hooks installed"
}

# Main execution
function Main {
    Write-Step "Setting up ShadowFS development environment for Windows..."
    
    # Check Rust installation
    if (-not (Test-Rust)) {
        Write-Error "Failed to configure Rust toolchain"
        exit 1
    }
    
    # Install Windows dependencies
    if (-not (Install-WindowsDeps)) {
        Write-Error "Failed to install Windows dependencies"
        exit 1
    }
    
    # Setup git hooks
    Setup-GitHooks
    
    # Final steps
    Write-Success "Development environment setup complete!"
    Write-Info "Run '.\scripts\check.ps1' to verify everything is working"
}

# Run main function
Main