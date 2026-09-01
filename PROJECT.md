# Project Context

## Purpose

`run-if-present` is a small Unix command-line wrapper for hooks and similar automation. It exits
successfully and silently only when a command or path that the caller explicitly made optional is
confirmed absent. Once presence is established, or when absence cannot be confirmed, it preserves
the failure instead of hiding it. The approved behavioral contract is
[`docs/spec/run-if-present.md`](docs/spec/run-if-present.md).

## Implementation contract

- The implementation language is Rust, with Rust 1.85 as the minimum supported version.
- The Cargo package, crate, and installed binary are all named `run-if-present`.
- Runtime dependencies are `clap` 4.6.6 and `which` 8.0.6. Keep `Cargo.lock` committed.
- The release targets are `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`,
  `x86_64-apple-darwin`, and `aarch64-apple-darwin`. Windows is unsupported.
- The project and crate use the SPDX license expression `MIT OR Apache-2.0`.
- Keep paths and arguments as operating-system strings; valid non-UTF-8 Unix input must not be
  rejected or converted lossily.
- Launch successful commands with Unix process replacement so the child retains its environment,
  streams, exit status, and signals as specified in Sections 7–10 of the specification.

## Verification

There is no `Cargo.toml` or implementation yet, so no project build, test, or lint command is
currently runnable. Do not report the following as completed until the Cargo project exists and
the commands have actually succeeded.

Every future merge and release candidate must pass:

```text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo package
```

It must also satisfy the platform, Rust 1.85, behavior-test, and release-artifact smoke-test matrix
in Section 13 of the specification. That matrix is a required outcome, not a claim that suitable
commands or automation already exist.

## Repository layout

- `docs/spec/run-if-present.md` is the approved source of product, behavior, verification, and
  release requirements.
- `PROJECT.md` is the concise, always-read project context. Keep detailed requirements in the
  specification rather than duplicating them here.

No implementation directory layout has been established yet.

## Project constraints

- Confirmed absence is the only runtime condition that the wrapper may translate to silent exit
  0. Inspection failures, unusable executables, and failures after presence or resolution must
  remain visible.
- Do not add configuration files read by the wrapper at runtime, persistent state, telemetry,
  runtime network access, shell evaluation, or privilege-management behavior.
- Do not add public features or APIs excluded from version 0.1; Section 17 of the specification is
  authoritative.
- Creating a public GitHub repository, pushing a tag, or publishing to crates.io or GitHub Releases
  requires explicit human approval at the time of that action. Approval of the specification or
  implementation does not authorize publication.
- Do not choose a new public behavior, dependency, release surface, or external action without
  returning to specification work.
