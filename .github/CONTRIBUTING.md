# Contributing to Grafito

Thank you for your interest in contributing to Grafito! This document provides guidelines and instructions for contributing.

## Code of Conduct

By participating in this project, you agree to abide by our [Code of Conduct](CODE_OF_CONDUCT.md).

## How to Contribute

### Reporting Bugs

1. Check if the bug has already been reported in [Issues](https://github.com/Diez111/Grafito/issues)
2. Use the bug report template if available
3. Include:
   - Clear description of the bug
   - Steps to reproduce
   - Expected vs actual behavior
   - Grafito version and OS information
   - Screenshots if applicable

### Suggesting Features

1. Open an issue with the feature request label
2. Describe the feature and its use case
3. Explain why it would be valuable to users

### Pull Requests

1. **Fork** the repository
2. **Create a branch** from `main` with a descriptive name:
   ```bash
   git checkout -b feature/add-new-tool
   git checkout -b fix/crash-on-import
   ```
3. **Make your changes** following our coding standards
4. **Write tests** for new functionality
5. **Run verification** before submitting:
   ```bash
   cargo fmt --all
   cargo clippy --workspace -- -D warnings
   cargo test --workspace
   cargo build --workspace --release
   ```
6. **Commit** using [Conventional Commits](https://www.conventionalcommits.org/):
   - `feat:` new features
   - `fix:` bug fixes
   - `refactor:` code refactoring
   - `docs:` documentation changes
   - `test:` adding or updating tests
   - `chore:` maintenance tasks
7. **Push** your branch and open a Pull Request

## Development Setup

### Prerequisites

- Rust 1.78 or later
- System dependencies: `libgmp-dev`, `libmpfr-dev`, `libmpc-dev`, `m4`
- GPU with Vulkan, Metal, or DX12 support (for GPU compute shaders)

### Building

```bash
git clone https://github.com/Diez111/Grafito.git
cd grafito
cargo build --release
```

### Running Tests

```bash
cargo test --workspace
```

### Code Style

- Format with `cargo fmt`
- No warnings allowed (`cargo clippy -- -D warnings`)
- Document public APIs in Spanish with examples
- Use descriptive English names for symbols
- Follow Rust 2021 edition conventions

## Commit Guidelines

- Use [Conventional Commits](https://www.conventionalcommits.org/) format
- Write clear, descriptive commit messages
- Reference issue numbers when applicable: `fix: resolve crash on import (#123)`
- Keep commits atomic and focused

## Review Process

1. All PRs require at least 1 approval from a code owner
2. CI must pass (tests, clippy, fmt)
3. Code owners will be automatically requested for review
4. Address review feedback before merging

## Security

- Report security vulnerabilities privately via email (see [SECURITY.md](.github/SECURITY.md))
- Do not commit secrets, API keys, or credentials
- All commits must be GPG-signed

## Questions?

Feel free to open an issue for questions about contributing.
