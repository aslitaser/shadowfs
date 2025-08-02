#!/usr/bin/env bash
# ShadowFS Development Setup Script
# Sets up development environment for ShadowFS

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

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

print_info() {
    echo -e "${BLUE}ℹ${NC} $1"
}

# Detect OS
detect_os() {
    case "$(uname -s)" in
        Linux*)     echo "linux";;
        Darwin*)    echo "macos";;
        *)          echo "unknown";;
    esac
}

# Install Linux dependencies
install_linux_deps() {
    print_step "Installing Linux dependencies..."
    
    if command -v apt-get &> /dev/null; then
        print_info "Detected Debian/Ubuntu system"
        sudo apt-get update
        sudo apt-get install -y fuse3 libfuse3-dev pkg-config
    elif command -v dnf &> /dev/null; then
        print_info "Detected Fedora/RHEL system"
        sudo dnf install -y fuse3 fuse3-devel pkgconfig
    elif command -v pacman &> /dev/null; then
        print_info "Detected Arch Linux system"
        sudo pacman -S --needed fuse3 pkgconf
    else
        print_error "Unsupported Linux distribution"
        print_info "Please install FUSE 3 development packages manually"
        return 1
    fi
    
    print_success "Linux dependencies installed"
}

# Install macOS dependencies
install_macos_deps() {
    print_step "Installing macOS dependencies..."
    
    # Check for Homebrew
    if ! command -v brew &> /dev/null; then
        print_error "Homebrew not found. Please install from https://brew.sh"
        return 1
    fi
    
    # Install macFUSE (for development/testing)
    print_info "Installing macFUSE for development..."
    brew install --cask macfuse
    
    print_success "macOS dependencies installed"
    print_info "Note: FSKit support requires macOS 15.0+"
}

# Check Rust installation
check_rust() {
    print_step "Checking Rust installation..."
    
    if ! command -v rustup &> /dev/null; then
        print_error "Rustup not found"
        print_info "Install Rust from: https://rustup.rs"
        return 1
    fi
    
    # Ensure stable toolchain is installed
    rustup install stable
    rustup default stable
    
    # Install required components
    rustup component add rustfmt clippy
    
    print_success "Rust toolchain configured"
}

# Setup git hooks
setup_git_hooks() {
    print_step "Setting up git hooks..."
    
    # Create hooks directory if it doesn't exist
    mkdir -p .git/hooks
    
    # Create pre-commit hook
    cat > .git/hooks/pre-commit << 'EOF'
#!/usr/bin/env bash
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
EOF
    
    chmod +x .git/hooks/pre-commit
    print_success "Git hooks installed"
}

# Main execution
main() {
    print_step "Setting up ShadowFS development environment..."
    
    # Detect OS
    OS=$(detect_os)
    print_info "Detected OS: $OS"
    
    # Check Rust installation
    if ! check_rust; then
        print_error "Failed to configure Rust toolchain"
        exit 1
    fi
    
    # Install OS-specific dependencies
    case "$OS" in
        linux)
            if ! install_linux_deps; then
                print_error "Failed to install Linux dependencies"
                exit 1
            fi
            ;;
        macos)
            if ! install_macos_deps; then
                print_error "Failed to install macOS dependencies"
                exit 1
            fi
            ;;
        *)
            print_error "Unsupported operating system"
            exit 1
            ;;
    esac
    
    # Setup git hooks
    if [ -d .git ]; then
        setup_git_hooks
    else
        print_info "Not in a git repository, skipping hooks setup"
    fi
    
    # Final steps
    print_success "Development environment setup complete!"
    print_info "Run './scripts/check.sh' to verify everything is working"
}

# Run main function
main "$@"