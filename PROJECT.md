# Project Context

## Purpose

`run-if-present` is a small Unix command-line wrapper for hooks and similar automation. It exits
successfully and silently only when a command or path that the caller explicitly made optional is
confirmed absent. It does not hide failures after presence is established or when absence cannot
be confirmed.

The approved specification, [`docs/spec/run-if-present.md`](docs/spec/run-if-present.md), is the
canonical source for detailed product, implementation, verification, and release requirements.

## Implementation and verification

The package is a Rust 2021 binary crate. `Cargo.toml` is the canonical package and version source,
and `Cargo.lock` is committed so the application and its normal dependency graph can be verified
with `--locked`. The minimum supported Rust version is 1.85.

For local work, install and select the minimum toolchain without committing the generated local
configuration:

```sh
mise use --env local --pin rust@1.85.0
mise exec -E local -- cargo metadata --locked --no-deps
```

Run Rust commands through `mise exec -E local --`. The current local verification set is:

```sh
mise exec -E local -- cargo fmt --all -- --check
mise exec -E local -- cargo clippy --all-targets --all-features --locked -- -D warnings
mise exec -E local -- cargo test --all-targets --all-features --locked
mise exec -E local -- cargo package --locked
fdfind -0 --type f --extension sh . | xargs -0 -r -n1 bash -n
"$ACTIONLINT_TMP/actionlint" -version  # must report 1.7.12
"$ACTIONLINT_TMP/actionlint"
```

`actionlint` must be the verified 1.7.12 binary obtained by the bootstrap procedure in plan Step 8;
`ACTIONLINT_TMP` is its temporary directory. The release helper behavior is exercised by the Rust
test suite, including static workflow boundaries and deterministic archive generation.

The implementation and checks above have been run locally on Linux x86_64. The committed GitHub
Actions matrix covers Linux and macOS with Rust 1.85.0 and stable, but hosted runs, publication,
and an actual release remain external and must not be claimed as verified from local results.
The remaining verification contract is in Section 13 of the specification.

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
