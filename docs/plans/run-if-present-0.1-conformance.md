# Align `run-if-present` 0.1 with the revised specification

## Goal

Bring the implementation on branch `run-if-present-0.1` into conformance with the specification
as revised in commit `e1057b6`, so the branch can be merged and, after hosted evidence, released.

## Specification

The governing specification is [`docs/spec/run-if-present.md`](../spec/run-if-present.md). The
implementer reads it before starting and uses the headings named below as the behavioral
authority. The previous plan, `docs/plans/run-if-present-0.1.md`, is complete except for its
Step 11; this plan supersedes it and carries that step forward as Step 9.

## Approach and why

The revision fixed public behaviors that the implementation had chosen on its own, and added a
few it did not have. Each step below changes one behavior boundary, test-first, against the built
binary, and becomes one commit, so that a reviewer can trace every specification decision to one
diff. Work stays on the existing branch because the person decided so: the branch already holds
the conforming baseline, and a second branch would have to carry the same commits.

The order follows the runtime's own sequence: command-line parsing (Step 1), the absence
boundary that every condition shares (Step 2), candidate classification (Step 3), process
transparency (Step 4), the one remaining test gap (Step 5), release automation (Step 6),
documentation of the changed behavior (Step 7), the local acceptance gate (Step 8), and the
hosted matrix (Step 9). Steps 1–4 are independent of each other in code but ordered this way so
that the diagnostic vocabulary of `## 10. Output and exit contract` is asserted once per step
rather than re-touched.

Reuse decisions, by layer:

- Recording whether descriptors 0–2 were open before the Rust runtime ran — extend the existing
  constructor in `src/runtime.rs` that already records the `SIGPIPE` disposition; the platform
  call `fcntl(fd, F_GETFD)` answers "open or not" without a dependency.
- Distinguishing a missing `--chdir` path component from an existing non-directory — the standard
  library's metadata call before the change-directory call; no new abstraction.
- Overriding the user-database lookup in a behavior test — the existing dynamic-loader interposer
  fixture in `tests/runtime.rs` (LD_PRELOAD on Linux, `__interpose` on macOS), extended to
  `getpwuid`; no new test dependency.
- Comparing the published crate with the retained one — the existing `curl` and `jq` calls in
  `.github/workflows/release.yml` and `.github/scripts/verify-same-checksum.sh`.
- Accepting any `https` changelog link — the existing `awk` check in
  `.github/scripts/verify-release-metadata.sh`, narrowed.

No new runtime or development dependency is adopted. `clap` stays pinned at `4.6.6`.

## Scope of change

The plan may change only:

- `src/**` and `tests/**`;
- `README.md` and `CHANGELOG.md`;
- `.github/workflows/release.yml`, `.github/scripts/verify-release-metadata.sh`, and
  `tests/release_helpers.rs`.

Do not change the specification, `CONTEXT.md`, `PROJECT.md`, `Cargo.toml`, `Cargo.lock`, agent
instruction files, git remotes, or anything under `.agents/`. Do not create a public repository,
push a tag, publish a crate, or publish a GitHub Release.

## Step order and prerequisites

Steps 1–8 are sequential and local. Every Rust or Cargo command runs through
`mise exec -E local --` from the worktree root
`/home/mizumi/develop/run-if-present/.agents/worktrees/run-if-present-0.1`; Rust 1.85.0 is
already pinned there in an untracked `mise.local.toml`, which is never staged. Step 9 is the
approval-gated hosted continuation and is reported as pending when its approval or remote is
absent. The local host is Linux x86_64; shell blocks in this plan are `bash` syntax and are run
through `bash`, not the interactive shell.

## Verification map

- Step 1 proves `### 4.1 Grammar`, `### 4.2 Public options and empty values`, and the `-h` line
  of `## 17. Not built in 0.1`.
- Step 2 proves the `### Confirmed absence` term of `## 2. Terms`, `## 5. Evaluation order and
  working directory`, and the `ENOTDIR` sentences of `### 7.2 Candidate classification` and
  the heading “## 8. `path` condition”.
- Step 3 proves the preflight definition of `### 7.2 Candidate classification`, the launch-target
  paragraph of the heading “## 8. `path` condition”, and the empty-entry sentence of
  `### 7.1 Resolution`.
- Step 4 proves `## 9. Process transparency` and the stderr paragraph of `## 10. Output and exit
  contract`.
- Steps 1–4 together prove the operation table, the fixed phrases, and the exit table of
  `## 10. Output and exit contract`. Two fixed phrases, `not an executable regular file` and
  `could not capture the inherited SIGPIPE disposition`, are existing behavior that Steps 3 and 4
  keep and re-assert; they are not new work.
- Step 5 proves the user-database sentence of `## 13. Verification contract`.
- Step 6 proves `### 15.1 Versioning and changelog` and items 2 and 4 of `### 15.3 Automation
  and credentials`; the minimum-Rust rule of `## 12. Implementation dependencies` is already
  enforced by the Rust 1.85.0 job in `.github/workflows/verify.yml` and needs no change.
- Step 7 proves `## 14. Public documentation` for the changed behavior.
- Step 8 proves the locally runnable subset of the merge gate of `## 13. Verification contract`
  (Linux, Rust 1.85.0). The macOS and stable-toolchain legs of the merge gate come from the
  existing hosted `.github/workflows/ci.yml`, which runs `verify.yml` on a pull request; Step 9
  supplies the release-candidate gate and the hosted evidence of `## 3. Supported systems`,
  `### 15.2 Artifacts`, and `### 15.3 Automation and credentials`. Merging is not claimed on
  Step 8 alone.
- Step 6's changes to the changelog check and the publication step have no hosted exercise
  before a real release: the rehearsal workflow runs neither the metadata check (a promoted
  changelog does not exist on a non-release ref) nor publication (approval-gated by
  `## 16. Approval boundaries`). Their evidence is the local helper tests and `actionlint`.

## Left to the implementer

Private function and type names, test-helper structure, and the order of assertions inside a
test are open when every choice preserves the specified behavior. Where a step says "the
existing X", extending X in place or extracting a helper from it are both acceptable. No public
option, exit result, operation name, fixed phrase, dependency, artifact, or release action is
delegated.

## Stop conditions

Stop and hand back if the specification lacks meaning needed to choose an input boundary, error
result, public interface, dependency, artifact, or release action; if implementation would depart
from approved behavior; before any irreversible, privileged, dangerous, or externally visible
operation; if an accidental change spreads outside this plan's scope; or if a changed approach
still makes no progress. Also stop if a step's test cannot be made deterministic without a new
dependency or a privileged host change.

## Test command

`mise exec -E local -- cargo test --all-targets --all-features --locked` for each RED, GREEN,
and REFACTOR transition, unless a step names a narrower single-test filter for RED. A step is not
complete until the full command passes. The final local order is fixed in Step 8.

Not every "Done when" clause is new behavior. Where a clause already holds on the current tree
(each step says which), the test is added as a regression guard and observed passing once; RED
evidence is required only for the clauses the step marks as changing. A step records, per test,
whether it failed before the change.

## Out of scope

Everything in `## 17. Not built in 0.1`. Publication, tag creation or push, remote repository
creation, and protected-environment configuration require later human action.

## Step 1 — Make `--help` the only special token in an operand position

Purpose: Pin help handling in both subcommands so a mistyped hook exits 2 instead of silently 0,
and remove the one help path that can panic. Specification:
`docs/spec/run-if-present.md`#`### 4.1 Grammar`,
`docs/spec/run-if-present.md`#`### 4.2 Public options and empty values`,
`docs/spec/run-if-present.md`#`## 10. Output and exit contract`,
`docs/spec/run-if-present.md`#`## 17. Not built in 0.1`.
Prerequisites: none beyond the worktree and toolchain.
May change: `src/main.rs`, `src/cli.rs`, `tests/cli.rs`, `tests/boundaries.rs`.
Done when: `command --help` and `path --help` with nothing after them print the subcommand help
and exit 0; either followed by any token exits 2 with the fixed syntax message
`invalid command line`; `-h` at the top level exits 2 and in an operand position names a program
or guard; `--version` and `--chdir` in an operand position name a program or guard; a help
request combined with an empty `--chdir` value prints help and exits 0; every one of the four
help and version paths exits 0 with nothing on stderr when stdout is a closed pipe; both
subcommand helps are printed from the same parser definition and list no wrapper option, while
the top-level help lists exactly `--chdir`, `--help`, and `--version`.
Changing behavior (RED expected): `command --help extra` and `path --help extra -- cmd` exit 2;
`command --help` with stdout closed exits 0; `path --help` no longer lists `--help`. Already
holding (regression guards): the rest.
Shown by: test — RED then GREEN then REFACTOR behavior tests named for each condition in "Done
when". The `path` subcommand's help stops being a `clap` help action and uses the same manual
check as `command`, because `clap` prints help before it can see trailing tokens; since the
`path` launch command is declared required in `src/cli.rs`, that requiredness moves into the
wrapper's own validation so that `path --help` alone reaches the help check, while a missing
launch command or separator still exits 2 with `invalid command line`. The closed-stdout fixture
is a pipe whose read end is closed before the wrapper is spawned, not a shell pipeline. The
existing boundary test that lists the options each help exposes is updated to the listing above.
Finish with the test command.
Left to the implementer: where the manual help check lives and how the four paths share it.
Stop and hand back if: `clap` 4.6.6 cannot be configured so that a leading `--help` in the `PATH`
position reaches the wrapper's own check without also swallowing other hyphen-leading guard
values.

## Step 2 — Treat a non-directory path component as confirmed absence

Purpose: Make `ENOTDIR` mean "nothing can exist here" on every path the wrapper inspects, while an
entry that exists with the wrong kind stays a visible failure. Specification:
`docs/spec/run-if-present.md`#`### Confirmed absence`,
`docs/spec/run-if-present.md`#`## 5. Evaluation order and working directory`,
`docs/spec/run-if-present.md`#`### 7.2 Candidate classification`,
`docs/spec/run-if-present.md` heading “## 8. `path` condition”,
`docs/spec/run-if-present.md`#`## 10. Output and exit contract`.
Prerequisites: Step 1 is complete.
May change: `src/runtime.rs`, `tests/runtime.rs`.
Done when: a `path` guard, a `--chdir` target, an explicit-path executable in `command` mode,
and a `PATH` entry whose path passes through a regular file (including a trailing slash on a
regular file) are confirmed absence — the guard, `--chdir`, and the `command`-mode explicit path
exit 0 silently, and the `PATH` entry contributes no candidate while the search continues (the
`path`-mode launch target is Step 3's, where no candidate is 127); `--chdir` naming an existing
regular file still exits 1 with operation `chdir` and the operating system's error text from the
change-directory call; permission errors and link loops still exit 1.
Changing behavior (RED expected): every `ENOTDIR` case (all exit 1 today). Already holding
(regression guards): the existing-regular-file `--chdir` target, permission errors, link loops.
Shown by: test — RED then GREEN then REFACTOR behavior tests named for each of the four paths
with an intermediate regular file, the trailing-slash form for the guard and for `--chdir`, and
the existing-regular-file `--chdir` target. The `--chdir` implementation inspects the target
before changing into it, for the reason `## 5. Evaluation order and working directory` states.
Finish with the test command.
Left to the implementer: whether the "missing or through a non-directory" test is one helper
shared by the four inspection sites or written at each site.
Stop and hand back if: the platform's metadata call reports an intermediate regular file with an
error kind that cannot be distinguished from a permission error.

## Step 3 — Define preflight once and apply it to every launch target

Purpose: Make candidate classification follow links, accept any execute bit, skip dangling links,
and apply identically to `path`-mode launch targets. Specification:
`docs/spec/run-if-present.md`#`### 7.1 Resolution`,
`docs/spec/run-if-present.md`#`### 7.2 Candidate classification`,
`docs/spec/run-if-present.md`#`### 7.3 Resolution-to-execution race`,
`docs/spec/run-if-present.md` heading “## 8. `path` condition”,
`docs/spec/run-if-present.md`#`## 10. Output and exit contract`.
Prerequisites: Step 2 is complete.
May change: `src/runtime.rs`, `tests/runtime.rs`.
Done when: a symbolic link to an executable on `PATH` is selected; a dangling link on `PATH` is
no candidate; a regular file whose only execute bit is not the caller's passes preflight and
fails at replacement with operation `execute` and exit 126; an empty `PATH` entry names the
effective working directory; in `path` mode an explicit-path launch target goes through the same
preflight as in `command` mode, so `path /tmp -- ./not-executable` and `command ./not-executable`
both exit 126 with byte-identical diagnostics (operation `resolve executable`, the same escaped
operand, the fixed phrase `not an executable regular file`); in `path` mode a launch target with
no candidate exits 127 with operation `execute`, the name as given, and the fixed phrase
`command not found`, in both the bare-name form and the explicit-path form (missing, or passing
through a regular file).
Changing behavior (RED expected): the `path`-mode explicit-path cases (preflight parity and the
127 for an explicit path through a regular file). Already holding (regression guards): symbolic
link selection, dangling link skipped, any-execute-bit preflight with `execute` 126 (an existing
test with mode `0o010` covers it), empty `PATH` entry, bare-name 127.
Shown by: test — RED then GREEN then REFACTOR behavior tests named for each condition in "Done
when". The "only execute bit is not the caller's" fixture is a file owned by the test user with
mode `0o070` or `0o007`; on a root host the test drops to the unprivileged user ID the suite
already uses, and fails visibly if it cannot. Finish with the test command.
Left to the implementer: how the `path`-mode explicit path is routed through the existing
classifier.
Stop and hand back if: the running platform lets the owner execute a file with no owner execute
bit, so the fixture cannot produce `EACCES` at replacement.

## Step 4 — Restore inherited process state before replacement

Purpose: Make descriptors, signal dispositions, `argv[0]`, and `PWD` reach the child as the caller
set them, and keep the exit status the contract when stderr is unusable. Specification:
`docs/spec/run-if-present.md`#`## 9. Process transparency`,
`docs/spec/run-if-present.md`#`## 10. Output and exit contract`.
Prerequisites: Step 3 is complete.
May change: `src/runtime.rs`, `tests/runtime.rs`.
Done when: a child started with descriptor 0, 1, or 2 closed sees that descriptor closed rather
than `/dev/null`; a failure to record or re-close is reported with operation `prepare execution`,
exit 1, and the operating system's error text from the failing call as the reason; a script
printing `$0` shows the name as given in both the bare-name and explicit-path forms; `env` run
through `--chdir` prints the caller's `PWD`; a replacement failure whose diagnostic is written to
a closed pipe on stderr still exits 126 or 127 rather than dying of `SIGPIPE`; a replacement
failure with descriptor 2 closed at startup still exits 126 or 127 rather than 101, the
diagnostic being lost; the existing inherited-`SIGPIPE` behavior is unchanged.
Changing behavior (RED expected): the closed-descriptor cases, the closed-pipe diagnostic, and
the closed-stderr replacement failure. Already holding (regression guards): `argv[0]`, `PWD`,
inherited `SIGPIPE`.
Shown by: test — RED then GREEN then REFACTOR behavior tests named for each condition in "Done
when". The descriptor state is recorded in the existing constructor that records `SIGPIPE`; the
wrapper sets `SIGPIPE` back to ignored immediately before writing any diagnostic that follows the
restoration; and every diagnostic is written with a call whose error is ignored, never through a
macro that panics on a failed write, because re-closing descriptor 2 makes a later diagnostic
write fail by design. Finish with the test command.
Left to the implementer: the representation of the recorded descriptor state and the exact point
in the replacement sequence where re-closing happens, provided it is after every wrapper write to
stderr that precedes a successful replacement and before the replacement call.
Stop and hand back if: the constructor cannot observe the descriptor state before the Rust
runtime fills it on either supported operating system.

## Step 5 — Exercise the empty user-database home through the built CLI

Purpose: Close the one test gap the specification names, so that the user-database fallback is
proven on the binary and not only on the expansion logic. Specification:
`docs/spec/run-if-present.md`#`## 6. Tilde expansion`,
`docs/spec/run-if-present.md`#`## 13. Verification contract`.
Prerequisites: Step 4 is complete.
May change: `tests/runtime.rs`.
Done when: a behavior test runs the built binary with `HOME` unset and the dynamic-loader
interposer replacing `getpwuid` so that the record's home field is empty, and observes exit 1 with
operation `expand` and the fixed phrase `home directory is unavailable`; the existing unit test
of the expansion logic stays.
Changing behavior: none in `src/`; the test is new (RED by absence, GREEN once written).
Shown by: test — one behavior test named for the empty home field reaching the binary, compiled
and run on both supported operating systems; macOS support of the `getpwuid` interposer is
established by Step 9's hosted run, and a macOS failure there is a hand-back to this step, not a
skip. Finish with the test command.
Left to the implementer: whether the `getpwuid` interposer is a second fixture or an extension of
the existing one.
Stop and hand back if: the interposer cannot replace `getpwuid` for a statically linked or
hardened binary on the local host.

## Step 6 — Align release automation with the crate and changelog rules

Purpose: Make the first publication compare regenerated and published bytes against the retained
crate, and accept the first release's tag-page link. Specification:
`docs/spec/run-if-present.md`#`### 15.1 Versioning and changelog`,
`docs/spec/run-if-present.md`#`### 15.3 Automation and credentials`,
`docs/spec/run-if-present.md`#`## 16. Approval boundaries`.
Prerequisites: Step 5 is complete. In a `bash` shell on the Linux x86_64 host, bootstrap
`actionlint` 1.7.12 with the following exact commands even if another version is installed.

```sh
ACTIONLINT_TMP=$(mktemp -d)
gh release download v1.7.12 --repo rhysd/actionlint \
  --pattern actionlint_1.7.12_linux_amd64.tar.gz \
  --pattern actionlint_1.7.12_checksums.txt --dir "$ACTIONLINT_TMP"
gh attestation verify "$ACTIONLINT_TMP/actionlint_1.7.12_linux_amd64.tar.gz" \
  --repo rhysd/actionlint
rg ' actionlint_1\.7\.12_linux_amd64\.tar\.gz$' \
  "$ACTIONLINT_TMP/actionlint_1.7.12_checksums.txt" \
  | (cd "$ACTIONLINT_TMP" && sha256sum -c -)
tar -xzf "$ACTIONLINT_TMP/actionlint_1.7.12_linux_amd64.tar.gz" \
  -C "$ACTIONLINT_TMP" actionlint
"$ACTIONLINT_TMP/actionlint" -version
```

May change: `.github/workflows/release.yml`, `.github/scripts/verify-release-metadata.sh`,
`tests/release_helpers.rs`.
Done when: the changelog check accepts a `[<version>]: https://…` link of any `https` form and
rejects a non-`https` or missing link, with the `## [<version>] - <YYYY-MM-DD>` heading rule
unchanged (the existing helper test that requires a comparison link ending in the tag is
rewritten, not kept); in the publication job, a step that does not carry the registry token runs
`cargo package --locked` under the same fixed inputs and verifies the regenerated checksum
against the retained crate with the existing checksum helper; the token-carrying step then, on a
404, publishes and immediately fetches the registry checksum and verifies it against the retained
crate; the existing 200 branch is unchanged; a non-200 answer after publishing, or any mismatch,
exits non-zero with no polling and no retry inside the workflow, because a rerun of the workflow
lands in the 200 branch by design.
Shown by: check — run the test command (the release-helper tests cover a tag-page link, a
comparison link, an `http` link, and a missing link, and the existing test that pins the token to
the publish step's single environment line still passes); run
`"$ACTIONLINT_TMP/actionlint" -version`, require 1.7.12, then `"$ACTIONLINT_TMP/actionlint"`;
inspect the publication job for the three comparisons in order and confirm that no command
prints the token. These changes have no hosted exercise before a real release (see the
verification map).
Left to the implementer: shell structure of the publication steps, provided each comparison is a
separate observable failure.
Stop and hand back if: the actionlint asset fails attestation or checksum verification, or the
crates.io API no longer exposes the version checksum used by the existing 200 branch.

## Step 7 — Document the changed behavior

Purpose: Keep the README and changelog truthful for the behaviors Steps 1–4 changed.
Specification: `docs/spec/run-if-present.md`#`## 14. Public documentation`,
`docs/spec/run-if-present.md`#`### 15.1 Versioning and changelog`.
Prerequisites: Steps 1–6 are complete.
May change: `README.md`, `CHANGELOG.md`.
Done when: the README's exit-behavior section lets a reader predict `--help` followed by tokens
(exit 2), a path through a regular file (skipped), a dangling link on `PATH` (skipped), and a
`--chdir` target that is a file (exit 1); the `Unreleased` section of the changelog describes
these as part of the 0.1 behavior rather than as changes to a released version.
Shown by: check — run every README command example that needs no publication against
`target/debug/run-if-present`, then search README and changelog for claims that contradict
`## 17. Not built in 0.1` or name an unpublished artifact as available.
Left to the implementer: prose organization and example operands.
Stop and hand back if: documenting a behavior would require describing an exit result the
specification does not state.

## Step 8 — Run the complete local acceptance gate

Purpose: Demonstrate that the branch is internally consistent and locally release-ready.
Specification: `docs/spec/run-if-present.md`#`## 13. Verification contract`,
`docs/spec/run-if-present.md`#`## 16. Approval boundaries`,
`docs/spec/run-if-present.md`#`## 17. Not built in 0.1`.
Prerequisites: Steps 1–7 are complete. Rerun the complete Step 6 `actionlint` bootstrap block
in a fresh `bash` shell to create a new verified `ACTIONLINT_TMP`; do not reuse the path or
variable from Step 6.
May change: only files already in scope for Steps 1–7, and only to correct a failed check through
a new test-first concern; after any correction, return to the earliest step that owns a changed
file and rerun every later step.
Done when: all local checks pass on the locked project, the public surface contains no excluded
feature, the working diff contains no credential material or build artifact, and every
unavailable external result is named explicitly.
Shown by: check — in order run
`mise exec -E local -- cargo test --all-targets --all-features --locked`,
`mise exec -E local -- cargo fmt --check`,
`mise exec -E local -- cargo clippy --all-targets --all-features --locked -- -D warnings`,
`mise exec -E local -- cargo build --release --locked`,
`target/release/run-if-present --version`, `mise exec -E local -- cargo package --locked`,
`mise exec -E local -- cargo package --list`, and `"$ACTIONLINT_TMP/actionlint"`; then inspect
the scoped diff from the commit that added this plan to the branch head and scan it for
credential-shaped assignments, excluded public features, and files outside this plan's scope.
Left to the implementer: none.
Stop and hand back if: any check fails after one changed corrective approach, or completion would
require an action reserved by the approval boundary.

## Step 9 — Run the approval-gated hosted matrix

Purpose: Obtain the host and artifact evidence that only hosted runners can produce, with every
publication action disabled. Specification:
`docs/spec/run-if-present.md`#`## 3. Supported systems`,
`docs/spec/run-if-present.md`#`## 13. Verification contract`,
`docs/spec/run-if-present.md`#`### 15.2 Artifacts`,
`docs/spec/run-if-present.md`#`### 15.3 Automation and credentials`,
`docs/spec/run-if-present.md`#`## 16. Approval boundaries`.
Prerequisites: Step 8 is complete; the person has explicitly approved the exact remote operation;
a remote with the required runners exists; a human has configured the `release` GitHub
environment with required-reviewer approval and placed `CARGO_REGISTRY_TOKEN` only in that
environment's secrets, and provided read-only evidence of those settings. Without that approval,
remote, or evidence, stop after Step 8 and hand back with this step pending.
May change: only files already in scope for Steps 1–6, and only through a new locally verified
fix for an observed hosted failure; a fix returns to the step that owns the file and reruns every
later step through Step 8 before Step 9 is rerun. A hosted failure in a workflow file this plan
does not own (`ci.yml`, `verify.yml`, `release-rehearsal.yml`, `release-artifacts.yml`, other
scripts) is a hand-back. No tag, release, or registry state.
Done when: hosted jobs pass the behavior suite on x86_64 Linux and aarch64 macOS with Rust
1.85.0 and stable, build all four release targets on their native runners, smoke-test every
archive's `--version`, verify `SHA256SUMS`, and run the non-publishing release rehearsal through
the same helper entry points as the tag workflow; repository settings show a human reviewer gates
publication; logs show the token value was neither requested by the rehearsal nor printed.
Shown by: external — a human triggers the approved run; inspect the run and downloaded artifacts;
require every matrix job to pass with no skipped required check, every archive layout and checksum
to agree, action references to remain pinned, and each job's effective permissions to match its
minimum need.
Left to the implementer: none.
Stop and hand back if: the remote operation lacks current approval, a required runner is
unavailable, any job or smoke test fails, a credential appears in logs, or obtaining evidence
would create or publish a tag, crate, or release. If a hosted failure requires a file change,
return to Step 8 and rerun its complete gate before rerunning Step 9.
