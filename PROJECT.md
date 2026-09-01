# Project Context

## Purpose

`run-if-present` is a small Unix command-line wrapper for hooks and similar automation. It exits
successfully and silently only when a command or path that the caller explicitly made optional is
confirmed absent. It does not hide failures after presence is established or when absence cannot
be confirmed.

The approved specification, [`docs/spec/run-if-present.md`](docs/spec/run-if-present.md), is the
canonical source for detailed product, implementation, verification, and release requirements.

## Implementation and verification

The implementation language is Rust. There is no `Cargo.toml` or implementation yet, so no project
build, test, or lint command is currently runnable.

Once the Cargo project exists, every merge and release candidate must pass:

```text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo package
```

Do not treat these checks as completed until they have actually succeeded. The remaining
verification contract is in Section 13 of the specification.

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
