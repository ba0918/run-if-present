# `run-if-present` 0.1 specification

Status: Approved

Intended destination: `docs/spec/run-if-present.md` in the `run-if-present` repository.

## 1. Outcome

`run-if-present` is a small Unix CLI that exits successfully and silently when an explicitly
optional command or path is absent, but exposes every failure after presence is established or
when absence cannot be confirmed.

Success is observable when a hook can omit an unavailable dependency without hiding a broken
installed dependency. A counter-example is converting permission errors, missing script
interpreters, or executed-command failures into exit 0.

## 2. Terms

### Confirmed absence

Confirmed absence means the wrapper completed the relevant filesystem or executable search and
found no candidate. It never means that inspection failed.

Success is observable when a missing guard exits 0 silently while an inaccessible search location
reports an error. A counter-example is mapping every unsuccessful executable search to absence.

### Wrapper failure and executed-command result

A wrapper failure occurs before process replacement or when replacement itself fails. After
replacement, the result belongs to the executed command and is not translated by the wrapper.

Success is observable when the command retains its exit status and terminating signal. A
counter-example is spawning a child and mapping its result to a wrapper-defined code.

## 3. Supported systems

The first release supports Linux and macOS on x86_64 and aarch64. Windows is unsupported.

Success is observable through builds for all four release targets, behavior tests on Linux and
macOS, and a `--version` smoke test for each artifact. A counter-example is claiming Windows
support because the crate happens to compile there.

## 4. Command-line interface

### 4.1 Grammar

```text
run-if-present [--chdir DIR] command COMMAND [ARG...]
run-if-present [--chdir DIR] path PATH -- COMMAND [ARG...]
```

`--chdir` is accepted only before the subcommand. In `command` mode, every token after `COMMAND`
is child input, including tokens beginning with `-`. In `path` mode, `--` is required between the
guard and command.

Success is observable when `run-if-present command printf --help` passes `--help` to `printf` and
omitting the `path` separator exits 2. A counter-example is consuming child options as wrapper
options after `COMMAND` begins.

### 4.2 Public options and empty values

The first release exposes only `--chdir DIR`, top-level and subcommand `--help`, and `--version`.
Invalid syntax is written to stderr and exits 2. An explicitly empty command, guard, or `--chdir`
value is invalid syntax; empty child arguments remain valid and unchanged.

Success is observable when an empty guard is rejected but an empty child argument is preserved. A
counter-example is treating an empty wrapper-owned path as optional absence or adding verbose,
quiet, dry-run, JSON, skip-reason, shell, file-only, or directory-only options.

## 5. Evaluation order and working directory

With `--chdir`, the wrapper expands its leading tilde when applicable, changes directory, evaluates
relative guards, relative executables, and relative `PATH` entries there, then replaces itself.
Absolute paths retain their meaning.

A missing target directory is confirmed absence and exits 0 silently. An existing non-directory,
permission failure, or other inspection or change-directory failure reports a diagnostic and exits
1.

Success is observable when relative guards and relative `PATH` entries resolve from the new
directory. A counter-example is evaluating the condition in the caller's directory first.

## 6. Tilde expansion

Only an exact `~` or leading `~/` in wrapper-owned paths—the `path` guard and `--chdir`—is
expanded. Executables and child arguments are never rewritten. `~user` remains a literal relative
path. On Unix, expansion uses `std::env::home_dir()`: a non-empty `HOME` value is used first, and
an unset or empty `HOME` falls back to the operating system's user database. Non-UTF-8 values are
retained. If neither source yields a non-empty home path, the wrapper exits 1 with a diagnostic;
an empty path returned by the user database is not expanded relative to the current directory.

Success is observable when a quoted `~/bin` guard expands but a child argument `~/input` does not.
A counter-example is invoking a shell for expansion or treating an unknown home as absence.

## 7. `command` condition

### 7.1 Resolution

An executable containing a path separator is an explicit absolute or relative path. A bare name is
searched in `PATH`. Aliases, shell functions, builtins, pipelines, redirections, and shell syntax
are never resolved. An unset or empty `PATH` supplies no candidates. Relative `PATH` entries use
the effective working directory from Section 5.

Success is observable when an external executable is found and a shell-only function is skipped.
A counter-example is invoking `sh -c` to make a command appear present.

### 7.2 Candidate classification

Search continues past candidates that fail the wrapper's preflight checks (an existing regular
file with Unix execute permission) and selects the first candidate that passes those checks.
Passing preflight is not a guarantee that the operating system will permit process replacement.
If none is found, outcomes have this priority:

1. An inspection failure that prevents a complete search exits 1 with a diagnostic.
2. At least one existing candidate that fails preflight exits 126 with a diagnostic.
3. No candidate is confirmed absence and exits 0 silently.

Discovery is local: each `PATH` entry is taken literally, joined with the bare name, and inspected
in order with the standard library, so that these distinctions remain observable.

Success is observable when an earlier candidate that fails preflight does not hide a later
candidate that passes preflight, but a preflight-failure-only result exits 126. A launch failure
after preflight never resumes search. A counter-example is mapping every unsuccessful search
result directly to silent success.

If inspecting an earlier search location fails but a later candidate passes preflight, the later
candidate is selected. Inspection failure has priority only when no candidate passes preflight.

### 7.3 Resolution-to-execution race

After selection, every process-replacement failure is visible and search never resumes at a later
candidate. A file that disappears or an
executable script whose interpreter is missing exits 127. Permission or executable-format failure
exits 126. Other replacement failures exit 1.

Success is observable when deletion after resolution is an error. A counter-example is rerunning
optional-absence logic after resolution succeeded.

## 8. `path` condition

The guard accepts any filesystem entry whose followed target exists, including a file or
directory. A dangling symbolic link is confirmed absence. A symbolic-link loop, permission error,
or other inability to determine existence exits 1 with a diagnostic.

An absent guard exits 0 silently. A present guard proceeds directly to execution: a missing launch
target exits 127, an uninvokable target exits 126, and another replacement failure exits 1.

Success is observable when a present directory runs the command and a dangling link skips it. A
counter-example is silently skipping a missing executable after the guard was present.

## 9. Process transparency

Successful launch uses Unix process replacement. The wrapper does not intentionally change the
environment, standard input, standard output, standard error, credentials, or resource limits,
except for an explicitly changed working directory. Normal Unix `exec` semantics still apply,
including closure of file descriptors marked close-on-exec. The wrapper adds or removes no
environment variables.

Paths and arguments remain operating-system strings. Valid non-UTF-8 Unix bytes are neither
converted nor rejected.

Success is observable when streams, environment, exit status, signal, and non-UTF-8 arguments pass
through. A counter-example is lossy UTF-8 conversion before execution.

## 10. Output and exit contract

| Situation | Output | Result |
|---|---|---|
| Optional condition confirmed absent | none | exit 0 |
| Help or version | stdout | exit 0 |
| Invalid syntax or empty wrapper value | stderr | exit 2 |
| Inspection, expansion, or directory failure | one diagnostic on stderr | exit 1 |
| Executable exists but cannot be invoked | one diagnostic on stderr | exit 126 |
| Required launch target missing after presence/resolution | one diagnostic on stderr | exit 127 |
| Process replacement succeeds | wrapper emits nothing | replacement process decides |

Wrapper diagnostics are neutral English, one line, colorless, and shaped as:

```text
run-if-present: <operation>: <escaped operand>: <OS error>
```

Operands are quoted and escape control characters and unrepresentable bytes. The stable contract
is the prefix and operation; OS wording may differ by platform.

Success is observable when a newline in a filename cannot forge a log line. A counter-example is
printing operands raw or announcing successful skips.

## 11. Runtime boundaries

The wrapper has no configuration file, persistent state, telemetry, runtime network access, shell
evaluation, or privilege-management behavior.

Success is observable when behavior depends only on arguments, process context, and local
filesystem state. A counter-example is a cache that changes later skip behavior.

## 12. Implementation dependencies

- Rust is the implementation language.
- The public Cargo package/crate name and installed command name are both `run-if-present`.
- `clap` 4.6.6 provides OS-string-aware parsing, help, and version output; color is disabled.
- Tilde handling, executable discovery and classification, and Unix process replacement use the
  standard library and minimal local code.
- `Cargo.lock` is committed.
- `Cargo.toml` declares `rust-version = "1.85"`, the highest minimum of the selected dependencies.
- `Cargo.toml` declares the exact plain-text package description: `Run a command only when an
  optional command or path is present, without hiding execution failures.`
- The project and crate declare the SPDX license expression `MIT OR Apache-2.0` and carry
  `LICENSE-MIT` and `LICENSE-APACHE`.

Success is observable when Rust 1.85 builds and tests the locked project. A counter-example is an
undeclared higher compiler requirement or a shell/runtime dependency.

## 13. Verification contract

Every merge and release candidate passes:

- `cargo fmt --check`;
- `cargo clippy --all-targets --all-features -- -D warnings`;
- behavior tests on Linux and macOS;
- build and behavior tests on Rust 1.85;
- `cargo package`;
- a `--version` smoke test for every release artifact.

Behavior tests cover present and absent conditions, files, directories, dangling links,
inaccessible inspection, unusable and disappearing executables, missing script interpreters,
mixed `PATH` candidates, `--chdir`, tilde rules, empty values, argument and stream preservation,
child exits and signals, and non-UTF-8 Unix input where supported. Tilde tests cover non-empty,
unset, empty, and non-UTF-8 `HOME` values, including user-database fallback and an empty home field
returned by that database.

Success is observable from all checks exiting 0 without skipped failures. A counter-example is
testing only a classifier without executing the built CLI.

## 14. Public documentation

The first README is English-only and covers purpose, installation from crates.io and GitHub,
examples for both subcommands and `--chdir`, exit behavior, supported systems, lack of shell
evaluation, checksum verification, unsigned macOS guidance with `cargo install` as an alternative,
and the `MIT OR Apache-2.0` license choice.

Success is observable when a new user can predict whether a missing command, broken command, or
failed child is hidden. A counter-example is describing the tool only as "try to run a command."

## 15. Version and release contract

### 15.1 Versioning and changelog

The first version is `0.1.0`. `Cargo.toml` is the sole canonical version source. `CHANGELOG.md`
records standalone user-visible changes under `Unreleased`; a release promotes them to the exact
version heading with a comparison link. The tag is `v<version>` and is never moved or reused after
publication.

Success is observable when release commit, changelog heading, and tag all identify `0.1.0`. A
counter-example is maintaining independent version declarations.

### 15.2 Artifacts

```text
run-if-present-v<version>-x86_64-unknown-linux-musl.tar.gz
run-if-present-v<version>-aarch64-unknown-linux-musl.tar.gz
run-if-present-v<version>-x86_64-apple-darwin.tar.gz
run-if-present-v<version>-aarch64-apple-darwin.tar.gz
SHA256SUMS
```

Each archive contains the executable, `README.md`, `LICENSE-MIT`, and `LICENSE-APACHE`.
`SHA256SUMS` covers every archive. Initial macOS binaries are unsigned and unnotarized.

Success is observable when all archives share a layout and verify. A counter-example is publishing
one target before another target has failed to build.

### 15.3 Automation and credentials

Only a version tag on a commit that already passed all project checks may trigger release. The
workflow reruns the complete Section 13 contract on the exact tagged commit, verifies tag, Cargo
version, and changelog agreement, then builds and smoke-tests every artifact before a protected
GitHub environment approval gate.

Publication is restartable for one immutable tag:

1. Use the exact Rust 1.85.0 toolchain, including its bundled Cargo version, for package creation,
   publication, and every retry. Set `SOURCE_DATE_EPOCH` to the tagged commit's Unix timestamp.
2. Generate the `.crate` once, retain it as a GitHub Actions workflow artifact, and verify that a
   same-input regeneration has the same checksum before publication.
3. Create or update a draft GitHub Release and upload all verified archives plus `SHA256SUMS`.
4. If the crate version is absent from crates.io, publish under the same fixed inputs. If it
   already exists, verify that its registry checksum matches the retained or deterministically
   regenerated `.crate`; stop without overwriting on mismatch.
5. Publish the GitHub Release. If it is already public, verify its tag and asset checksums; stop
   without overwriting on mismatch.
6. On retry, retain every matching completed phase and perform only the missing phase.

No new version is required for a transient failure when all already-published bytes match the
tagged source. Any mismatch or uncertainty stops for human judgment; published bytes and tags are
never moved or replaced.

The workflow reads the dedicated crates.io token only from the protected environment secret
`CARGO_REGISTRY_TOKEN`. Its value never appears in repository content, examples, or logs. GitHub
Actions are pinned to immutable commit SHAs. Weekly Dependabot updates Cargo dependencies and
Actions references through the full verification contract.

Success is observable when an unapproved workflow cannot publish, every required check runs on the
tagged SHA, and a retry after one destination fails completes only the missing destination. A
counter-example is storing a token in a repository variable, publishing before all artifacts
build, or retrying an already-published crate without first comparing its checksum.

## 16. Approval boundaries

Implementation produces a locally release-ready Git repository. Creating the public GitHub
repository, pushing a tag, and publishing to crates.io or GitHub Releases are separate actions
requiring explicit human approval at the time. Adding a local remote configuration does not itself
publish anything.

Success is observable when local work finishes without consuming a public crate version or name.
A counter-example is treating specification approval as approval to publish.

## 17. Not built in 0.1

- Windows support.
- Shell aliases, functions, builtins, pipelines, redirections, or implicit `sh -c`.
- Separate file-only or directory-only guards.
- Verbose, quiet, dry-run, JSON, or skip-reason output.
- Configuration files, persistent state, telemetry, or runtime network access.
- Homebrew or other package-manager integration beyond crates.io.
- Installers, shell completions, or a man page.
- macOS code signing or notarization.

Success is observable when none appears in help, runtime behavior, or artifacts. A counter-example
is adding a speculative option for possible later use.

## 18. Rejected alternatives

- Command-only or path-only scope: both conditions are required.
- Default command mode plus `--path`: explicit subcommands keep extension points visible.
- `try-run`, `if-present`, `run-optional`, or `soft-run`: the chosen name states the condition
  without implying that runtime failures are swallowed.
- Skipping all lookup, inspection, or launch errors: only confirmed absence is optional.
- Zero dependencies: an established crate owns parsing, with local code only where its contract
  is insufficient.
- The `which` crate for discovery: it expands a leading `~` in `PATH` entries and normalises `.`
  components, so its result could only be accepted after comparing it with the literal candidate
  that local code had already built; the guard was larger than the discovery it protected.
- glibc binaries: musl reduces Linux host-library coupling.
- Latest-stable-only support: the compiler floor is declared and tested.
- Independent manual publication: a guarded workflow reduces mismatched public states.
- Signed initial macOS binaries: Apple account, certificate, and secret costs are deferred.

## 19. Undecided and delegated decisions

None. An implementer returns to specification work instead of silently choosing a new public
behavior, dependency, release surface, or external action.
