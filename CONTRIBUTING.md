# Contributing to Linux Universal Clipboard History

Thank you for considering contributing to the Linux Universal Clipboard History Manager! We welcome code contributions, bug reports, feature suggestions, and documentation improvements.

## Development Setup

### Prerequisites
- **Rust Toolchain**: 1.80+ (`rustup default stable`)
- **System Libraries**:
  - Debian/Ubuntu: `sudo apt install libgtk-4-dev libadwaita-1-dev libx11-dev libxtst-dev pkg-config`
  - Fedora: `sudo dnf install gtk4-devel libadwaita-devel libX11-devel libXtst-devel pkgconf`
  - Arch Linux: `sudo pacman -S gtk4 libadwaita libx11 libxtst pkgconf`

### Building the Project
```bash
# Clone the repository
git clone https://github.com/alierenaltindag/clipboard-history.git
cd clipboard-history

# Build all workspace crates
cargo build --workspace

# Run automated tests
cargo test --workspace

# Run Clippy lints
cargo clippy --workspace --all-targets -- -D warnings

# Check code formatting
cargo fmt --all -- --check
```

## Pull Request Guidelines

1. **Clean Architecture**: Follow the hexagonal boundaries (`crates/core`, `crates/daemon`, `crates/gui`, `crates/cli`).
2. **Memory Safety & Leaks**: Ensure no memory leaks or unbounded memory growths.
3. **Format & Lint**: Ensure `cargo fmt` and `cargo clippy --workspace --all-targets -- -D warnings` pass cleanly.
4. **Tests**: Include unit and integration tests for any new features or bug fixes.
