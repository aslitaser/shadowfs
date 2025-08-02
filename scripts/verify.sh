#!/usr/bin/env bash
# ShadowFS Project Verification Script
# Verifies the project structure and compilation

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Function to print colored output
print_step() {
    echo -e "\n${YELLOW}==>${NC} $1"
}

print_success() {
    echo -e "${GREEN}✓${NC} $1"
}

print_error() {
    echo -e "${RED}✗${NC} $1"
    exit 1
}

print_info() {
    echo -e "${BLUE}ℹ${NC} $1"
}

# Required files and directories
REQUIRED_FILES=(
    "Cargo.toml"
    ".gitignore"
    "LICENSE"
    "README.md"
    ".rustfmt.toml"
    ".clippy.toml"
    "rust-toolchain.toml"
    ".cargo/config.toml"
    ".github/workflows/ci.yml"
    ".github/dependabot.yml"
)

REQUIRED_DIRS=(
    "shadowfs-core"
    "shadowfs-windows"
    "shadowfs-macos"
    "shadowfs-linux"
    "shadowfs-cli"
    "docs"
    "examples"
    "scripts"
)

WORKSPACE_MEMBERS=(
    "shadowfs-core"
    "shadowfs-windows"
    "shadowfs-macos"
    "shadowfs-linux"
    "shadowfs-cli"
)

# Check required files exist
check_files() {
    print_step "Checking required files..."
    
    local missing=0
    for file in "${REQUIRED_FILES[@]}"; do
        if [ -f "$file" ]; then
            print_success "$file"
        else
            print_error "Missing file: $file"
            ((missing++))
        fi
    done
    
    if [ $missing -eq 0 ]; then
        print_success "All required files present"
    fi
}

# Check required directories exist
check_directories() {
    print_step "Checking required directories..."
    
    local missing=0
    for dir in "${REQUIRED_DIRS[@]}"; do
        if [ -d "$dir" ]; then
            print_success "$dir/"
        else
            print_error "Missing directory: $dir"
            ((missing++))
        fi
    done
    
    if [ $missing -eq 0 ]; then
        print_success "All required directories present"
    fi
}

# Check module files in each crate
check_crate_structure() {
    local crate=$1
    print_info "Checking $crate structure..."
    
    # Check Cargo.toml exists
    if [ ! -f "$crate/Cargo.toml" ]; then
        print_error "$crate/Cargo.toml not found"
    fi
    
    # Check src directory exists
    if [ ! -d "$crate/src" ]; then
        print_error "$crate/src/ not found"
    fi
    
    # Check main source file
    if [ "$crate" = "shadowfs-cli" ]; then
        if [ ! -f "$crate/src/main.rs" ]; then
            print_error "$crate/src/main.rs not found"
        fi
    else
        if [ ! -f "$crate/src/lib.rs" ]; then
            print_error "$crate/src/lib.rs not found"
        fi
    fi
    
    print_success "$crate structure verified"
}

# Validate Cargo.toml formatting
check_cargo_toml() {
    print_step "Validating Cargo.toml files..."
    
    # Check workspace Cargo.toml
    if ! cargo metadata --no-deps --format-version 1 > /dev/null 2>&1; then
        print_error "Invalid workspace Cargo.toml"
    fi
    
    print_success "All Cargo.toml files are valid"
}

# Check module declarations
check_modules() {
    print_step "Checking module declarations..."
    
    # Check shadowfs-core modules
    if [ -f "shadowfs-core/src/lib.rs" ]; then
        local expected_modules=("traits" "types" "error" "override_store" "stats")
        for module in "${expected_modules[@]}"; do
            if ! grep -q "pub mod $module;" "shadowfs-core/src/lib.rs"; then
                print_error "Missing module declaration: $module in shadowfs-core"
            fi
            if [ ! -f "shadowfs-core/src/$module.rs" ]; then
                print_error "Missing module file: shadowfs-core/src/$module.rs"
            fi
        done
        print_success "shadowfs-core modules verified"
    fi
    
    # Check platform crate modules
    for platform in windows macos linux; do
        if [ -f "shadowfs-$platform/src/lib.rs" ]; then
            case $platform in
                windows)
                    if ! grep -q "pub mod projfs;" "shadowfs-$platform/src/lib.rs"; then
                        print_error "Missing projfs module in shadowfs-windows"
                    fi
                    if ! grep -q "pub mod bindings;" "shadowfs-$platform/src/lib.rs"; then
                        print_error "Missing bindings module in shadowfs-windows"
                    fi
                    ;;
                macos)
                    if ! grep -q "pub mod fskit;" "shadowfs-$platform/src/lib.rs"; then
                        print_error "Missing fskit module in shadowfs-macos"
                    fi
                    if ! grep -q "pub mod bindings;" "shadowfs-$platform/src/lib.rs"; then
                        print_error "Missing bindings module in shadowfs-macos"
                    fi
                    ;;
                linux)
                    if ! grep -q "pub mod fuse;" "shadowfs-$platform/src/lib.rs"; then
                        print_error "Missing fuse module in shadowfs-linux"
                    fi
                    ;;
            esac
            print_success "shadowfs-$platform modules verified"
        fi
    done
}

# Run cargo check on all workspace members
run_cargo_check() {
    print_step "Running cargo check on all workspace members..."
    
    local platform=$(uname -s)
    
    for member in "${WORKSPACE_MEMBERS[@]}"; do
        # Skip platform-specific crates on wrong platform
        case "$member" in
            "shadowfs-windows")
                if [[ "$platform" != "MINGW"* ]] && [[ "$platform" != "CYGWIN"* ]]; then
                    print_info "Skipping $member (Windows only)"
                    continue
                fi
                ;;
            "shadowfs-macos")
                if [[ "$platform" != "Darwin" ]]; then
                    print_info "Skipping $member (macOS only)"
                    continue
                fi
                ;;
            "shadowfs-linux")
                if [[ "$platform" != "Linux" ]]; then
                    print_info "Skipping $member (Linux only)"
                    continue
                fi
                ;;
        esac
        
        print_info "Checking $member..."
        if ! cargo check --package "$member" 2>&1; then
            print_error "Compilation failed for $member"
        fi
        print_success "$member compiles successfully"
    done
    
    # Check platform-independent crates
    print_info "Checking platform-independent crates..."
    if ! cargo check --package shadowfs-core --package shadowfs-cli --package shadowfs-ffi 2>&1; then
        print_error "Platform-independent crates compilation failed"
    fi
    print_success "Platform-independent crates compile successfully"
}

# Print next steps
print_next_steps() {
    echo -e "\n${GREEN}================================${NC}"
    echo -e "${GREEN}✅ Verification complete!${NC}"
    echo -e "${GREEN}================================${NC}\n"
    
    echo -e "${YELLOW}Next steps:${NC}"
    echo -e "1. Run development setup: ${BLUE}./scripts/setup-dev.sh${NC}"
    echo -e "2. Run code quality checks: ${BLUE}./scripts/check.sh${NC}"
    echo -e "3. Build the project: ${BLUE}cargo build${NC}"
    echo -e "4. Run tests: ${BLUE}cargo test --workspace${NC}"
    echo -e "5. Build documentation: ${BLUE}cargo doc --open${NC}"
    echo -e "\n${YELLOW}Quick commands:${NC}"
    echo -e "- Format code: ${BLUE}cargo fmt${NC}"
    echo -e "- Run lints: ${BLUE}cargo clippy${NC}"
    echo -e "- Run CLI: ${BLUE}cargo run --bin shadowfs -- --help${NC}"
    echo -e "\n${GREEN}Happy coding! 🚀${NC}"
}

# Main execution
main() {
    print_step "Verifying ShadowFS project structure..."
    
    # Check if in project root
    if [ ! -f "Cargo.toml" ] || ! grep -q "shadowfs" "Cargo.toml"; then
        print_error "Must be run from ShadowFS project root"
    fi
    
    # Run all checks
    check_files
    check_directories
    
    # Check each crate structure
    for crate in "${WORKSPACE_MEMBERS[@]}"; do
        check_crate_structure "$crate"
    done
    
    check_cargo_toml
    check_modules
    run_cargo_check
    
    # All checks passed
    print_next_steps
}

# Run main function
main "$@"