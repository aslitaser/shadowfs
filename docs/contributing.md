# Contributing to ShadowFS

We welcome contributions to ShadowFS! This document provides guidelines for contributing to the project.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/yourusername/shadowfs.git`
3. Create a feature branch: `git checkout -b feature/your-feature-name`
4. Make your changes
5. Submit a pull request

## Development Setup

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone the repository
git clone https://github.com/aslitaser/shadowfs.git
cd shadowfs

# Build the project
cargo build

# Run tests
cargo test --workspace
```

## Code Style

- Follow Rust standard naming conventions
- Use `cargo fmt` before committing
- Ensure `cargo clippy` passes without warnings
- Add tests for new functionality
- Update documentation as needed

## Pull Request Process

1. Ensure all tests pass
2. Update the README.md with details of changes if applicable
3. Add entries to the CHANGELOG.md
4. The PR will be merged once reviewed and approved

## Testing

- Write unit tests for new functionality
- Add integration tests for platform-specific features
- Test on relevant platforms before submitting PR

## Documentation

- Add rustdoc comments to public APIs
- Update relevant documentation files
- Include examples where appropriate

## Commit Messages

Follow conventional commit format:
- `feat:` for new features
- `fix:` for bug fixes
- `docs:` for documentation changes
- `test:` for test additions/changes
- `refactor:` for code refactoring
- `perf:` for performance improvements

## Questions?

Feel free to open an issue for any questions or discussions!