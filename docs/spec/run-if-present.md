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

### Optional condition

An optional condition is one of the three things whose confirmed absence the wrapper turns into
a silent exit 0: the `path` guard, the executable named in `command` mode, and the target
directory of `--chdir`. Nothing else is optional. A `--chdir` target is optional because the
wrapper evaluates it before the guard (Section 5), so "change into the project directory if it
exists, otherwise skip" has no other spelling.

Success is observable when a missing `--chdir` directory skips the command silently while a
missing launch target after a present guard is an error. A counter-example is treating a missing
`--chdir` directory as a failure, or treating a missing script interpreter as optional.

### Confirmed absence

Confirmed absence means the wrapper completed the relevant filesystem or executable search and
found no candidate. It never means that inspection failed. The operating system reporting that a
component of the path is not a directory (`ENOTDIR`) is confirmed absence, because the operating
system has determined that nothing can exist at that path; an entry that exists at the path but is
of the wrong kind is not absence.

Success is observable when a missing guard exits 0 silently while an inaccessible search location
reports an error, and when `path /etc/passwd/hooks` (a component is a regular file) exits 0 while
`--chdir /etc/passwd` (the entry exists and is a file) exits 1. A counter-example is mapping every
unsuccessful executable search to absence.

### Wrapper failure and executed-command result

A wrapper failure occurs before process replacement or when replacement itself fails. After
replacement, the result belongs to the executed command and is not translated by the wrapper.

Success is observable when the command retains its exit status and terminating signal. A
counter-example is spawning a child and mapping its result to a wrapper-defined code.

## 3. Supported systems

The first release supports Linux and macOS on x86_64 and aarch64. Windows is unsupported.

Success is observable through builds for all four release targets, behavior tests on Linux and
macOS, and a `--version` smoke test for each artifact run natively on that artifact's own
architecture (Section 13). A counter-example is claiming Windows support because the crate
happens to compile there.

## 4. Command-line interface

### 4.1 Grammar

```text
run-if-present [--chdir DIR] command COMMAND [ARG...]
run-if-present [--chdir DIR] path PATH -- COMMAND [ARG...]
```

`--chdir` is accepted only before the subcommand. In `command` mode, every token after `COMMAND`
is child input, including tokens beginning with `-`. In `path` mode, `--` is required between the
guard and command.

The token in the `COMMAND` position of `command` mode is the executable, with one exception: an
exact `--help` with nothing after it requests the subcommand help. `--help` followed by any token
is invalid syntax and exits 2, so a mistyped hook is not turned into a search for a program named
`--help`. The same rule applies to the `PATH` position of `path` mode: an exact `--help` with
nothing after it requests the subcommand help, and `--help` followed by any token is invalid
syntax. `-h` is never recognised by the wrapper: at the top level it is invalid syntax, and in an
operand position it names a program or a guard. `--help` is the only wrapper option name treated
specially in an operand position; `--version` or `--chdir` there names a program or a guard like
any other token. Every other token beginning with `-` in the `PATH` position is a guard value. A
program literally named `--help` remains reachable through an explicit path such as `./--help`.

Success is observable when `run-if-present command printf --help` passes `--help` to `printf`,
`run-if-present command --help` and `run-if-present path --help` print help,
`run-if-present command --help extra` and `run-if-present path --help extra -- cmd` exit 2,
`run-if-present path -h -- cmd` evaluates a guard named `-h`, and omitting the `path` separator
exits 2. A counter-example is consuming child options as wrapper options after `COMMAND` begins,
searching `PATH` for `--help`, or printing help for `path --help extra -- cmd`.

### 4.2 Public options and empty values

The first release exposes only `--chdir DIR`, top-level and subcommand `--help`, and `--version`.
The help and version paths are exactly four: top-level `--help`, top-level `--version`,
`command --help`, and `path --help`. Invalid syntax is written to stderr and exits 2. An
explicitly empty command, guard, or `--chdir` value is invalid syntax; empty child arguments
remain valid and unchanged.

Help and version requests are honoured before wrapper-owned values are validated, so
`run-if-present --chdir "" command --help` prints help and exits 0. Help and version output goes
to stdout; a failure to write it (for example a closed pipe) does not change the exit status of 0
and produces no diagnostic, on every help and version path alike.

Success is observable when an empty guard is rejected but an empty child argument is preserved,
and when `run-if-present --version | head -0` exits 0 without output on stderr. A counter-example
is treating an empty wrapper-owned path as optional absence, adding verbose, quiet, dry-run, JSON,
skip-reason, shell, file-only, or directory-only options, or a help request that exits 101 because
stdout was closed.

## 5. Evaluation order and working directory

With `--chdir`, the wrapper expands its leading tilde when applicable, changes directory, evaluates
relative guards, relative executables, and relative `PATH` entries there, then replaces itself.
Absolute paths retain their meaning.

The `--chdir` target is an optional condition (Section 2). A target that does not exist, or whose
path passes through a non-directory (`ENOTDIR`, including a trailing slash on a regular file), is
confirmed absence and exits 0 silently. An entry that exists at the path and is not a directory, a
permission failure, or another inspection or change-directory failure reports a diagnostic and
exits 1. Distinguishing the two requires inspecting the target before changing into it; the
`chdir` system call alone reports both as `ENOTDIR`. The diagnostic for an existing non-directory
carries the operating system's error from the change-directory call itself. The wrapper does not
update the `PWD` environment variable (Section 9).

Success is observable when relative guards and relative `PATH` entries resolve from the new
directory, `--chdir /etc/passwd/sub` exits 0 silently, and `--chdir /etc/passwd` exits 1 with a
diagnostic. A counter-example is evaluating the condition in the caller's directory first, or
rewriting `PWD` the way a shell's `cd` does.

## 6. Tilde expansion

Only an exact `~` or leading `~/` in wrapper-owned paths—the `path` guard and `--chdir`—is
expanded. Executables and child arguments are never rewritten. `~user` remains a literal relative
path. On Unix, a non-empty `HOME` value is used first, and an unset or empty `HOME` falls back to
the home field of the user-database record for the real user ID (`getpwuid`). Non-UTF-8 values
are retained. If neither source yields a non-empty home path, the wrapper exits 1 with a diagnostic;
an empty path returned by the user database is not expanded relative to the current directory.

Success is observable when a quoted `~/bin` guard expands but a child argument `~/input` does not.
A counter-example is invoking a shell for expansion or treating an unknown home as absence.

## 7. `command` condition

### 7.1 Resolution

An executable containing a path separator is an explicit absolute or relative path. A bare name is
searched in `PATH`. Aliases, shell functions, builtins, pipelines, redirections, and shell syntax
are never resolved. An unset or empty `PATH` supplies no candidates. Relative `PATH` entries,
including an empty entry, which names the working directory, use the effective working directory
from Section 5.

Success is observable when an external executable is found and a shell-only function is skipped.
A counter-example is invoking `sh -c` to make a command appear present.

### 7.2 Candidate classification

Search continues past candidates that fail the wrapper's preflight checks and selects the first
candidate that passes them. Preflight follows symbolic links and requires that the followed target
is a regular file with at least one Unix execute bit set, for any of owner, group, or others;
whether the calling user may actually execute it is left to the operating system. A dangling
symbolic link is no candidate, as for the `path` guard. Passing preflight is not a guarantee that
the operating system will permit process replacement: a file executable only by another user
passes preflight and fails at replacement (Section 7.3, exit 126, operation `execute`). If none
is found, outcomes have this priority:

1. An inspection failure that prevents a complete search exits 1 with a diagnostic.
2. At least one existing candidate that fails preflight exits 126 with a diagnostic.
3. No candidate is confirmed absence and exits 0 silently.

A candidate path that does not exist, or that passes through a non-directory (`ENOTDIR`), is no
candidate: a `PATH` entry that is a regular file contributes nothing and the search continues, and
an explicit path with such a component is confirmed absence. A permission error or a symbolic-link
loop while inspecting a candidate is an inspection failure.

Discovery is local: each `PATH` entry is taken literally, joined with the bare name, and inspected
in order with the standard library, so that these distinctions remain observable.

Success is observable when an earlier candidate that fails preflight does not hide a later
candidate that passes preflight, but a preflight-failure-only result exits 126; when
`PATH=/etc/passwd:/bin` finds `/bin/true` without a diagnostic; when a symbolic link to an
executable is selected and a dangling link in `PATH` is skipped. A launch failure after preflight
never resumes search. A counter-example is mapping every unsuccessful search result directly to
silent success, exiting 1 because a `PATH` entry is a file, or exiting 126 for a dangling link.

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
directory. A dangling symbolic link is confirmed absence, and so is a guard whose path passes
through a non-directory (`ENOTDIR`, including a trailing slash on a regular file). A
symbolic-link loop, permission error, or other inability to determine existence exits 1 with a
diagnostic.

An absent guard exits 0 silently. A present guard proceeds to execution. The launch target after
`--` is resolved as in Section 7.1 and classified as in Section 7.2, in both its bare-name and
explicit-path forms, and replacement follows Section 7.3. Because presence has already been
established, the launch target is not an optional condition, and the third outcome of Section 7.2
changes: no candidate exits 127 with a diagnostic instead of 0. The other outcomes are unchanged:
an inspection failure exits 1, an uninvokable target exits 126, and another replacement failure
exits 1.

Success is observable when a present directory runs the command, a dangling link skips it,
`path /tmp -- no-such-command` exits 127, and `path /tmp -- ./not-executable` exits 126 with the
same diagnostic that `command ./not-executable` produces. A counter-example is silently skipping
a missing executable after the guard was present, or an explicit-path launch target that bypasses
preflight and reports a different operation than the bare-name form.

## 9. Process transparency

Successful launch uses Unix process replacement. The wrapper does not intentionally change the
environment, standard input, standard output, standard error, credentials, resource limits, or
signal dispositions, except for an explicitly changed working directory. Normal Unix `exec`
semantics still apply, including closure of file descriptors marked close-on-exec. The wrapper
adds or removes no environment variables, and does not update `PWD` after `--chdir`.

Where the language runtime alters inherited state before the wrapper runs, the wrapper records
the inherited state first and restores it before replacement: the `SIGPIPE` disposition, which
the runtime sets to ignored, is restored to the inherited value; file descriptors 0, 1, or 2 that
were closed at startup, which the runtime opens on `/dev/null`, are closed again. A failure to
record or restore is a wrapper failure (Section 10, operation `prepare execution`).

Paths and arguments remain operating-system strings. Valid non-UTF-8 Unix bytes are neither
converted nor rejected. The child's `argv[0]` is the command name exactly as given on the wrapper's
command line; the resolved path is used only as the file to execute.

Success is observable when streams, environment, exit status, signal, and non-UTF-8 arguments pass
through; when a child started with `SIGPIPE` ignored inherits it ignored and one started with the
default disposition dies of `SIGPIPE`; when a child started with stdin closed sees stdin closed
rather than `/dev/null`; when `env` run through `--chdir` prints the caller's `PWD`; and when a
script printing `$0` shows `tool` for `command tool` found on `PATH` and `./tool` for
`command ./tool`. A counter-example is lossy UTF-8 conversion before execution, a child that
always sees `SIGPIPE` ignored, or `argv[0]` rewritten to the resolved path.

## 10. Output and exit contract

| Situation | Output | Result |
|---|---|---|
| Optional condition confirmed absent: the `path` guard, the `command` executable, or the `--chdir` directory (Section 2) | none | exit 0 |
| Help or version | stdout | exit 0 |
| Invalid syntax or empty wrapper value | stderr | exit 2 |
| Inspection, expansion, or directory failure | one diagnostic on stderr | exit 1 |
| Executable exists but cannot be invoked | one diagnostic on stderr | exit 126 |
| Required launch target missing after presence/resolution | one diagnostic on stderr | exit 127 |
| Preparation for replacement fails, or replacement fails in a way not classified above | one diagnostic on stderr | exit 1 |
| Process replacement succeeds | wrapper emits nothing | replacement process decides |

Wrapper diagnostics are neutral English, one line, colorless, and take one of two shapes. A
diagnostic about an operand is:

```text
run-if-present: <operation>: <escaped operand>: <reason>
```

A syntax diagnostic has no operand and never echoes input bytes:

```text
run-if-present: syntax: <fixed message>
```

The operations are these eight; adding, removing, or renaming one is a breaking change:

| Operation | Meaning | Operand |
|---|---|---|
| `syntax` | the command line is not valid | none |
| `expand` | tilde expansion of a wrapper-owned path failed | the path as given |
| `chdir` | the `--chdir` target could not be inspected or entered | the expanded target |
| `inspect` | the `path` guard could not be inspected | the expanded guard |
| `resolve executable` | every existing candidate fails preflight | the candidate path |
| `inspect executable` | a candidate could not be inspected | the candidate path |
| `prepare execution` | restoring inherited state before replacement failed | the selected path |
| `execute` | replacement failed, or the `path`-mode launch target has no candidate | the selected path, or the name as given |

The reason is either the operating system's error text or one of these fixed phrases:
`home directory is unavailable` (`expand`), `command not found` (`execute`),
`not an executable regular file` (`resolve executable`), and
`could not capture the inherited SIGPIPE disposition` (`prepare execution`). The fixed syntax
messages are `invalid command line` and `<chdir|command|path> must not be empty`.

Operands are quoted and escape control characters and unrepresentable bytes. The reason is not
escaped and never contains input bytes. The stable contract is the prefix, the operation, and the
fixed phrases; operating-system wording may differ by platform.

The exit status is the contract; delivery of a diagnostic depends on stderr being open. When
stderr was closed at startup the diagnostic is lost and the exit status stands. A diagnostic
written after the `SIGPIPE` disposition has been restored (Section 9) is preceded by the wrapper
ignoring `SIGPIPE` again, so that a closed pipe on stderr cannot replace the exit status with a
signal; the wrapper exits immediately afterwards, so the child never observes this.

Success is observable when a newline in a filename cannot forge a log line, and when a replacement
failure with stderr connected to a closed pipe still exits 126 or 127. A counter-example is
printing operands raw, announcing successful skips, or a wrapper that dies of `SIGPIPE` while
reporting a failure.

## 11. Runtime boundaries

The wrapper has no configuration file, persistent state, telemetry, runtime network access, shell
evaluation, or privilege-management behavior.

Success is observable when behavior depends only on arguments, process context, and local
filesystem state. A counter-example is a cache that changes later skip behavior.

## 12. Implementation dependencies

- Rust is the implementation language.
- The public Cargo package/crate name and installed command name are both `run-if-present`.
- `clap` 4 provides OS-string-aware parsing, help, and version output; color is disabled. The
  exact version is pinned in `Cargo.toml` (`4.6.6` at `0.1.0`). A dependency update that would
  raise the minimum supported Rust version above 1.85 is not merged in the 0.1 series.
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

Two gates share one set of checks.

The merge gate, which every merge passes:

- `cargo fmt --check`;
- `cargo clippy --all-targets --all-features -- -D warnings`;
- behavior tests on Linux and macOS;
- build and behavior tests on Rust 1.85;
- `cargo package --locked`.

The release-candidate gate, which every release candidate passes, is the merge gate plus a build
of every artifact in Section 15.2 and a `--version` smoke test of each artifact, run natively on
that artifact's own operating system and architecture.

Behavior tests run on at least one architecture per supported operating system (x86_64 Linux and
aarch64 macOS at `0.1.0`); all four combinations are not required. Tests that expect a permission
failure run as an unprivileged user: on a runner that is root, the test drops to an unprivileged
user ID itself, and if it cannot, it fails visibly rather than skipping.

Behavior tests cover present and absent conditions, files, directories, dangling links,
inaccessible inspection, paths passing through a regular file (guard, `--chdir`, explicit
executable, and `PATH` entry, including a trailing slash), an existing non-directory `--chdir`
target, unusable and disappearing executables, missing script interpreters, mixed `PATH`
candidates including an empty entry, a symbolic link to an executable and a dangling link in
`PATH`, an executable whose only execute bit belongs to another user, `--chdir`, tilde rules,
empty values, `command --help` and `path --help` each with and without trailing tokens, a help
request combined with an empty `--chdir` value, top-level `-h`, `-h` and `--version` in operand
positions, `argv[0]` preservation, argument and stream preservation, closed standard descriptors
reaching the child closed,
`PWD` unchanged after `--chdir`, `path`-mode launch targets in bare-name and explicit-path forms,
a missing `path` separator, help and version output written to a closed stdout, `SIGPIPE`
inherited ignored and inherited default, a replacement-failure diagnostic written to a closed pipe,
child exits and signals, and non-UTF-8 Unix input where supported. Tilde tests cover non-empty,
unset, empty, and non-UTF-8 `HOME` values, including user-database fallback and an empty home
field returned by that database; the empty home field is exercised both by a unit test of the
expansion logic and by running the built CLI with the user-database lookup replaced through the
dynamic loader (Linux; on macOS where supported).

Success is observable from all checks exiting 0 without skipped failures. A counter-example is
testing only a classifier without executing the built CLI, or a merge blocked on an artifact that
only a release builds.

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
records standalone user-visible changes under `Unreleased`; a release promotes them to a heading
of the form `## [<version>] - <YYYY-MM-DD>` with a link reference line `[<version>]: <URL>`. For
the first release the URL is the tag page (`releases/tag/v0.1.0`); for later releases it is the
comparison `compare/v<previous>...v<version>`. The release check requires an `https` URL and does
not enforce either form. The tag is `v<version>` and is never moved or reused after publication.

Success is observable when release commit, changelog heading, and tag all identify `0.1.0`. A
counter-example is maintaining independent version declarations, or a first release blocked
because no previous tag exists to compare against.

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
one target while another target's build has failed.

### 15.3 Automation and credentials

Only a version tag on a commit that already passed all project checks may trigger release. The
workflow verifies tag, Cargo version, and changelog agreement, then reruns the release-candidate
gate of Section 13 on the exact tagged commit; the artifacts that gate builds and smoke-tests are
the ones published, behind a protected GitHub environment approval gate.

Publication is restartable for one immutable tag:

1. Use the exact Rust 1.85.0 toolchain, including its bundled Cargo version, for package creation,
   publication, and every retry. Set `SOURCE_DATE_EPOCH` to the tagged commit's Unix timestamp.
2. Run `cargo package --locked` once under those inputs and retain the `.crate` as a GitHub
   Actions workflow artifact. `cargo publish` always repackages, so the retained file is the
   reference for comparison, never the bytes uploaded.
3. Create or update a draft GitHub Release and upload all verified archives plus `SHA256SUMS`.
4. If the crate version is absent from crates.io: regenerate the `.crate` under the same inputs,
   verify that its checksum equals the retained one, publish, then fetch the registry checksum and
   verify that it equals the retained one. If the version already exists: verify that its registry
   checksum matches the retained `.crate`. Any mismatch stops for human judgment without
   overwriting.
5. Publish the GitHub Release. If it is already public, verify its tag and asset checksums; stop
   without overwriting on mismatch.
6. On retry, retain every matching completed phase and perform only the missing phase.

No new version is required for a transient failure when all already-published bytes match the
tagged source. Any mismatch or uncertainty stops for human judgment; published bytes and tags are
never moved or replaced.

The workflow reads the dedicated crates.io token only from the protected environment secret
`CARGO_REGISTRY_TOKEN`. Its value never appears in repository content, examples, or logs. GitHub
Actions are pinned to immutable commit SHAs. Weekly Dependabot updates Cargo dependencies and
Actions references through the full verification contract, subject to the minimum-Rust rule of
Section 12.

Success is observable when an unapproved workflow cannot publish, every required check runs on the
tagged SHA, and a retry after one destination fails completes only the missing destination. A
counter-example is storing a token in a repository variable, publishing before all artifacts
build, retrying an already-published crate without first comparing its checksum, or publishing
without comparing the regenerated `.crate` to the retained one.

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
- `-h` as a help alias.
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
- `std::env::home_dir()` for the home path: on Rust 1.85 it returns an empty `HOME` as-is instead
  of falling back to the user database, and it is deprecated there, so the fallback that Section 6
  requires is read from the user database directly.
- The `which` crate for discovery: it expands a leading `~` in `PATH` entries and normalises `.`
  components, so its result could only be accepted after comparing it with the literal candidate
  that local code had already built; the check was larger than the discovery it protected.
- `command --help extra` printing help or searching for a program named `--help`: both exit 0
  and hide a mistake inside a hook.
- Refusing bare names as the `path`-mode launch target: it breaks `path ~/proj -- npm test`.
- Treating `--version` or `--chdir` in an operand position as invalid syntax like `--help`: a
  hook is unlikely to be mistyped that way, and each special case narrows what a program may be
  called.
- Letting `path --help` with trailing tokens print help: the same mistake in `command` mode exits
  2, and both modes should surface it.
- `argv[0]` set to the resolved path: programs that select behavior by their invoked name would
  see a different name than the shell gives them.
- Preflight by `access(X_OK)`: its answer depends on root and ACL semantics, while the operating
  system already gives the final answer at replacement; any execute bit is the observable test.
- Treating a dangling symbolic link in `PATH` as an uninvokable candidate: the guard and the shell
  both treat it as nothing there.
- `ENOTDIR` as an inspection failure: the operating system has determined that the path cannot
  exist, the same class as a missing entry, unlike a permission error or a link loop.
- A missing `--chdir` directory as exit 1: Section 5 evaluates `--chdir` before the guard, so
  the "change directory if present, otherwise skip" use would need a different evaluation order.
- Documenting the runtime's `/dev/null` fill of closed standard descriptors as an accepted
  deviation: it would treat the runtime's `SIGPIPE` change and its descriptor change differently.
- Listing a panic exit of 101 in the exit table: a designed outcome is not a panic.
- A diagnostic and exit 1 when help or version output cannot be written: `clap`, which prints the
  top-level help and version, deliberately swallows the write error and exits 0; making the
  wrapper's own help paths stricter would diverge from that and add a ninth operation for no
  practical benefit.
- Letting a post-restoration diagnostic be subject to `SIGPIPE` like any program: the exit table
  promises 126 or 127 for a replacement failure, and a signal death would remain on that path.
- Relaxing Section 13 to accept an injection-only test for the empty home field: it would break
  the section's own counter-example.
- glibc binaries: musl reduces Linux host-library coupling.
- Latest-stable-only support: the compiler floor is declared and tested.
- Independent manual publication: a guarded workflow reduces mismatched public states.
- Signed initial macOS binaries: Apple account, certificate, and secret costs are deferred.

## 19. Undecided and delegated decisions

None. An implementer returns to specification work instead of silently choosing a new public
behavior, dependency, release surface, or external action.
