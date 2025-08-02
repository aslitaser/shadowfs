# ShadowFS Development Check Script for Windows
# Runs all code quality checks for the project

param(
    [switch]$NoColor
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

# Main execution
function Main {
    Write-Step "Running ShadowFS development checks..."
    
    # Check if cargo is installed
    try {
        $null = Get-Command cargo -ErrorAction Stop
    }
    catch {
        Write-Error "Cargo is not installed. Please install Rust."
        exit 1
    }
    
    # Format check
    Write-Step "Checking code formatting..."
    try {
        cargo fmt --all -- --check
        Write-Success "Code formatting check passed"
    }
    catch {
        Write-Error "Code formatting check failed. Run 'cargo fmt' to fix."
        exit 1
    }
    
    # Clippy lints
    Write-Step "Running clippy lints..."
    try {
        cargo clippy --all-targets --all-features -- -D warnings
        Write-Success "Clippy check passed"
    }
    catch {
        Write-Error "Clippy check failed"
        exit 1
    }
    
    # Run tests
    Write-Step "Running tests..."
    try {
        cargo test --all
        Write-Success "All tests passed"
    }
    catch {
        Write-Error "Tests failed"
        exit 1
    }
    
    # Generate documentation
    Write-Step "Checking documentation..."
    try {
        cargo doc --no-deps --quiet
        Write-Success "Documentation generation successful"
    }
    catch {
        Write-Error "Documentation generation failed"
        exit 1
    }
    
    Write-Success "All checks passed! 🎉"
}

# Run main function
Main