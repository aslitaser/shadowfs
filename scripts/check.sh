#!/usr/bin/env bash
# ShadowFS Development Check Script
# Runs all code quality checks for the project

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print colored output
print_step() {
    echo -e "${YELLOW}==>${NC} $1"
}

print_success() {
    echo -e "${GREEN}✓${NC} $1"
}

print_error() {
    echo -e "${RED}✗${NC} $1"
}

# Main execution
main() {
    print_step "Running ShadowFS development checks..."
    
    # Check if cargo is installed
    if ! command -v cargo &> /dev/null; then
        print_error "Cargo is not installed. Please install Rust."
        exit 1
    fi
    
    # Format check
    print_step "Checking code formatting..."
    if cargo fmt --all -- --check; then
        print_success "Code formatting check passed"
    else
        print_error "Code formatting check failed. Run 'cargo fmt' to fix."
        exit 1
    fi
    
    # Clippy lints
    print_step "Running clippy lints..."
    if cargo clippy --all-targets --all-features -- -D warnings; then
        print_success "Clippy check passed"
    else
        print_error "Clippy check failed"
        exit 1
    fi
    
    # Run tests
    print_step "Running tests..."
    if cargo test --all; then
        print_success "All tests passed"
    else
        print_error "Tests failed"
        exit 1
    fi
    
    # Generate documentation
    print_step "Checking documentation..."
    if cargo doc --no-deps --quiet; then
        print_success "Documentation generation successful"
    else
        print_error "Documentation generation failed"
        exit 1
    fi
    
    print_success "All checks passed! 🎉"
}

# Run main function
main "$@"