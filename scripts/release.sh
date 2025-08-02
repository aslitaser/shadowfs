#!/usr/bin/env bash
# ShadowFS Release Automation Script
# Automates the release process for ShadowFS

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Configuration
CRATES=(
    "shadowfs-core"
    "shadowfs-windows"
    "shadowfs-macos"
    "shadowfs-linux"
    "shadowfs-ffi"
    "shadowfs-cli"
)

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

# Get current version from Cargo.toml
get_current_version() {
    grep -E '^version = ".*"$' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/'
}

# Update version in workspace Cargo.toml
update_version() {
    local new_version=$1
    print_step "Updating version to $new_version..."
    
    # Update workspace version
    sed -i.bak "s/^version = \".*\"/version = \"$new_version\"/" Cargo.toml
    rm Cargo.toml.bak
    
    print_success "Version updated to $new_version"
}

# Generate changelog entry
generate_changelog() {
    local version=$1
    local date=$(date +%Y-%m-%d)
    
    print_step "Generating changelog entry..."
    
    # Create CHANGELOG.md if it doesn't exist
    if [ ! -f CHANGELOG.md ]; then
        cat > CHANGELOG.md << EOF
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

EOF
    fi
    
    # Get commits since last tag
    local last_tag=$(git describe --tags --abbrev=0 2>/dev/null || echo "")
    local commit_range="${last_tag:+$last_tag..}HEAD"
    
    # Create temporary changelog entry
    cat > changelog_entry.tmp << EOF

## [$version] - $date

### Added
EOF
    
    # Add new features
    git log $commit_range --pretty=format:"- %s" --grep="^feat:" >> changelog_entry.tmp || true
    
    cat >> changelog_entry.tmp << EOF

### Fixed
EOF
    
    # Add fixes
    git log $commit_range --pretty=format:"- %s" --grep="^fix:" >> changelog_entry.tmp || true
    
    cat >> changelog_entry.tmp << EOF

### Changed
EOF
    
    # Add other changes
    git log $commit_range --pretty=format:"- %s" --grep="^(refactor|perf|style):" >> changelog_entry.tmp || true
    
    # Insert new entry after header
    sed -i.bak '/^# Changelog/,/^## \[/ {
        /^## \[/i\
'"$(cat changelog_entry.tmp)"'
    }' CHANGELOG.md || {
        # If no existing entries, append to end
        cat changelog_entry.tmp >> CHANGELOG.md
    }
    
    rm CHANGELOG.md.bak changelog_entry.tmp
    print_success "Changelog entry generated"
}

# Run pre-release checks
run_checks() {
    print_step "Running pre-release checks..."
    
    # Run format check
    cargo fmt --all -- --check || {
        print_error "Format check failed"
        return 1
    }
    
    # Run clippy
    cargo clippy --all-targets --all-features -- -D warnings || {
        print_error "Clippy check failed"
        return 1
    }
    
    # Run tests
    cargo test --all || {
        print_error "Tests failed"
        return 1
    }
    
    print_success "All checks passed"
}

# Build release binaries
build_releases() {
    print_step "Building release binaries..."
    
    # Create release directory
    mkdir -p target/release-artifacts
    
    # Build for current platform
    cargo build --release --package shadowfs-cli
    
    # Copy binary to release artifacts
    case "$(uname -s)" in
        Linux*)
            cp target/release/shadowfs target/release-artifacts/shadowfs-linux-x86_64
            ;;
        Darwin*)
            cp target/release/shadowfs target/release-artifacts/shadowfs-macos-x86_64
            ;;
    esac
    
    print_success "Release binaries built"
    print_info "Cross-platform builds should be done in CI"
}

# Create git tag
create_tag() {
    local version=$1
    local tag="v$version"
    
    print_step "Creating git tag $tag..."
    
    # Commit changes
    git add -A
    git commit -m "release: $version" || {
        print_info "No changes to commit"
    }
    
    # Create annotated tag
    git tag -a "$tag" -m "Release $version"
    
    print_success "Git tag $tag created"
    print_info "Run 'git push && git push --tags' to push changes"
}

# Main release process
main() {
    # Check if we're in the repo root
    if [ ! -f Cargo.toml ] || [ ! -d shadowfs-core ]; then
        print_error "Must be run from repository root"
        exit 1
    fi
    
    # Get version argument
    if [ $# -ne 1 ]; then
        print_error "Usage: $0 <new-version>"
        print_info "Example: $0 0.2.0"
        exit 1
    fi
    
    local new_version=$1
    local current_version=$(get_current_version)
    
    print_info "Current version: $current_version"
    print_info "New version: $new_version"
    
    # Confirm release
    read -p "Continue with release? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        print_info "Release cancelled"
        exit 0
    fi
    
    # Run checks
    if ! run_checks; then
        print_error "Pre-release checks failed"
        exit 1
    fi
    
    # Update version
    update_version "$new_version"
    
    # Generate changelog
    generate_changelog "$new_version"
    
    # Build releases
    build_releases
    
    # Create tag
    create_tag "$new_version"
    
    print_success "Release $new_version prepared successfully!"
    print_info "Next steps:"
    print_info "  1. Review the changes: git diff HEAD~1"
    print_info "  2. Push changes: git push && git push --tags"
    print_info "  3. Create GitHub release from tag"
    print_info "  4. Upload release artifacts from target/release-artifacts/"
    print_info "  5. Publish crates: cargo publish -p <crate-name>"
}

# Run main function
main "$@"