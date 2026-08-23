# AGENTS.md

## Project

`mbtop` is a small Rust terminal system monitor designed for narrow tmux and dashboard panes. Keep the binary fast to start, dependency-light, and portable across terminals.

## Source and design conventions

- Application code lives in `src/main.rs`; keep the rendering and sampling paths straightforward and allocation-light.
- Use Ratatui and Crossterm primitives before introducing new UI dependencies.
- Preserve terminal-native styling: use reset/default foregrounds and ANSI palette colors, and leave backgrounds unset so terminal and tmux themes remain in control.
- Keep labels compact and readable. Unicode icons must be monochrome, single-cell where practical, and understandable without a font-specific icon pack.
- The dashboard currently supports outer areas down to `36×8`; below that, render the centered resize message and redraw on resize.
- Keep the VHS demo in `docs/mbtop.tape` and regenerate `docs/mbtop.gif` when visible UI styling changes.

## Verification

Before committing Rust changes, run:

```sh
cargo fmt --check
cargo check --locked
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

For release changes, also build the macOS targets:

```sh
cargo build --locked --release --target aarch64-apple-darwin
RUSTC=/Users/matheusseabra/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc \
RUSTDOC=/Users/matheusseabra/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc \
rustup run stable cargo build --locked --release --target x86_64-apple-darwin
```

## Release workflow

- Bump the version in `Cargo.toml`, `Cargo.lock`, and the README badge/help text together.
- Tag the pushed release commit as `vX.Y.Z` and publish arm64 and Intel macOS archives to the GitHub release.
- Update `matheuseabra/homebrew-tap/Formula/mbtop.rb` with both archive URLs and checksums.
- Validate the formula with `brew audit --strict --online matheuseabra/tap/mbtop` and `brew fetch --force matheuseabra/tap/mbtop`.
- Keep generated release archives out of the repository working tree.

## Change discipline

- Prefer small, focused commits with imperative messages.
- Keep documentation and the rendered demo aligned with user-visible behavior.
- Do not add secrets, local tool state, build output, or `.agents/` contents to Git.
