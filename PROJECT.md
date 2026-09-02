# Project Context

## Purpose

`run-if-present` is a small Unix command-line wrapper for hooks and similar automation. It exits
successfully and silently only when a command or path that the caller explicitly made optional is
confirmed absent. It does not hide failures after presence is established or when absence cannot
be confirmed.

The approved specification, [`docs/spec/run-if-present.md`](docs/spec/run-if-present.md), is the
canonical source for detailed product, implementation, verification, and release requirements.

## Implementation and verification

The project is implemented in Rust 2021 with a minimum supported Rust version of 1.85. The Cargo
package manifest and locked dependency graph are present in `Cargo.toml` and `Cargo.lock`; the
implementation is in `src/`, with behavior and boundary coverage in `tests/`.

Run the locally reproducible checks with the repository's locked dependencies:

```text
cargo build --locked
cargo test --all-targets --all-features --locked
cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo package --locked
```

The hosted checks are separate evidence. `.github/workflows/verify.yml` runs behavior tests on
Linux and macOS with Rust 1.85.0 and stable, plus the format, lint, and package checks on Rust
1.85.0. `.github/workflows/release-artifacts.yml` builds the packaged binaries and smoke-tests each
binary's `--version` output. Do not claim those hosted platform or release-artifact checks are
complete until the corresponding workflow succeeds. The remaining verification contract is in
Section 13 of the specification.

## Project constraints

- Confirmed absence is the only runtime condition that the wrapper may translate to silent exit
  0. Inspection failures, unusable executables, and failures after presence or resolution must
  remain visible.
- Section 11 of the specification is authoritative for runtime boundaries.
- Section 17 of the specification is authoritative for public features and APIs excluded from
  version 0.1.
- Creating a public GitHub repository, pushing a tag, or publishing to crates.io or GitHub Releases
  requires explicit human approval at the time of that action. Approval of the specification or
  implementation does not authorize publication.
