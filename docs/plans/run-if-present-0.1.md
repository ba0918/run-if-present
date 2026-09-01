# Implement `run-if-present` 0.1

## Goal

Produce a locally release-ready `run-if-present` 0.1 repository whose Unix CLI, tests,
documentation, packaging, and guarded release automation satisfy the approved specification.

## Specification

The governing specification is [`docs/spec/run-if-present.md`](../spec/run-if-present.md).
The implementer must read it before starting and use its headings named below as the behavioral
authority.

## Approach and why

Build one Rust binary with a narrow sequence of boundaries: parse operating-system strings,
establish the effective working directory, evaluate exactly one optional condition, and replace
the process. Keep classification and formatting in small functions that integration tests can
exercise through the built binary. Keep process replacement at the outer boundary so the normal
Unix process contract remains observable rather than being reconstructed from a spawned child.

The work remains one plan because the runtime, cross-platform behavior tests, package contents,
and release workflow prove one 0.1 contract. Separate branches would leave intermediate results
that cannot satisfy the specification's merge and release-candidate checks. The steps below are
independently verifiable and should become one-concern commits, so a failed later step does not
make the earlier runtime work opaque.

Reuse decisions, in ladder order:

- Command-line parsing — adopt `clap` 4.6.6, as fixed by `## 12. Implementation dependencies`;
  its OS-string-aware parser owns the public grammar while child tokens remain opaque.
- Bare-command discovery — adopt `which` 8.0.6, as fixed by `## 12. Implementation dependencies`;
  supplement it locally only where the specified absence, inspection-failure, and preflight
  distinctions require more information than discovery returns.
- Tilde expansion, filesystem classification, diagnostic escaping, and process replacement —
  use the Rust and Unix platform facilities already available, then add minimal local code; the
  specification explicitly assigns these boundaries to the standard library and local code.
- Behavioral test process control and temporary filesystem setup — use the Rust standard library
  first; add a development dependency only if a named test cannot be made deterministic without
  it, and stop if that dependency would impose a new public or persisted contract.
- CI matrices, release artifacts, checksums, retained workflow artifacts, and protected approval
  — adopt GitHub Actions and standard platform tools because `### 15.3 Automation and credentials`
  fixes that platform. Pin every third-party action to a full commit SHA; do not build a custom
  automation service.
- GitHub Actions syntax validation — adopt `actionlint` 1.7.12 from its official release because
  it is the ecosystem checker for workflow structure and expressions. Verify that exact version
  with `actionlint -version`, then run `actionlint`; obtain it from the official v1.7.12 GitHub
  release and verify the release attestation plus published checksum before use. It is a temporary
  development tool, not a Cargo or runtime dependency.

## Scope of change

The plan may add or change only:

- `Cargo.toml` and `Cargo.lock`;
- `src/**` and `tests/**`;
- `README.md`, `CHANGELOG.md`, `LICENSE-MIT`, and `LICENSE-APACHE`;
- `.github/workflows/**` and `.github/dependabot.yml`;
- narrowly scoped test fixtures or helper scripts under `tests/**` or `.github/**` when a workflow
  or behavior test cannot express the same check directly.

Do not change the approved specification, `PROJECT.md`, agent instruction files, git remotes, or
anything under `.agents/`. Do not create a public repository, push a tag, publish a crate, or
publish a GitHub Release.

## Step order and prerequisites

Steps 1–10 are sequential. Step 1 establishes the locked compiler and dependency floor. Steps 2–6
use test-first development against that project. Step 7 documents only behavior already proven by
those tests. Steps 8 and 9 implement CI and release automation while keeping their decision logic
locally testable. Step 10 is the local repository-wide acceptance check and cannot compensate for
missing evidence from an earlier step.

The local host can prove Unix behavior for its own operating system. The GitHub Actions matrix is
the required evidence for macOS behavior and all four release targets. Workflow configuration may
be validated locally, but it is not recorded as passing until an actual run on the relevant hosts
has succeeded. Step 11 is therefore a separate approval-gated continuation after Step 10: if no
approved remote exists, hand back the locally complete implementation with Step 11 explicitly
pending. Pending external evidence does not block local implementation of Steps 9 or 10, and it
must never be reported as a passing merge or release-candidate check.

## Verification map

- Steps 2 and 3 prove `## 4. Command-line interface`, `## 5. Evaluation order and working
  directory`, and `## 6. Tilde expansion`.
- Steps 4 and 5 prove `## 7. command condition`, `## 8. path condition`, `## 9. Process
  transparency`, and `## 10. Output and exit contract`.
- Step 6 proves `## 11. Runtime boundaries` and guards `## 17. Not built in 0.1`.
- Steps 1 and 10 prove `## 12. Implementation dependencies` and the local portions of
  `## 13. Verification contract`.
- Step 7 proves `## 14. Public documentation` and the local documentation portions of
  `## 15. Version and release contract`.
- Steps 8 and 9 implement the automation required by `## 3. Supported systems`, the
  cross-platform portions of `## 13. Verification contract`, and `### 15.2 Artifacts` through
  `### 15.3 Automation and credentials`; Step 11 supplies their external execution evidence.
- Every step remains within `## 16. Approval boundaries`; Step 10 checks that the exclusions in
  `## 17. Not built in 0.1` did not enter the public surface.

## Left to the implementer

Internal module names, private function names, test-helper structure, and the division of code
among files are open when every choice preserves the specified behavior and dependency direction.
The implementer may choose maintained third-party GitHub Actions and their immutable commit SHAs
when the chosen actions provide exactly the specified checkout, toolchain, cache, artifact, and
release capabilities. No public option, error category, dependency version, artifact name,
release destination, or publication behavior is delegated.

## Stop conditions

Stop and hand back if the specification lacks meaning needed to choose an input boundary, error
result, public interface, dependency, artifact, or release action; if implementation would depart
from approved behavior; before any irreversible, privileged, dangerous, or externally visible
operation; if an accidental change spreads outside this plan's scope; or if a changed approach
still makes no progress. Also stop if a required crate or Rust 1.85 cannot support the specified
OS-string or Unix behavior, rather than raising the compiler floor or changing dependencies.

## Test command

Use `mise exec -E local -- cargo test --all-targets --all-features` for each RED, GREEN, and
REFACTOR transition unless a step names a narrower single-test command for RED. A step is not
complete until the full command passes. The final local check order is fixed in Step 10.

## Out of scope

Everything in `## 17. Not built in 0.1` is out of scope. Publication, tag creation or push, remote
repository creation, protected-environment configuration, signing, and notarization require later
human action and are not performed by this plan.

## Step 1 — Establish the locked Rust package and legal files

Purpose: Create the minimal buildable package boundary and canonical metadata before behavior is
added. Specification: `docs/spec/run-if-present.md`#`## 12. Implementation dependencies`,
`docs/spec/run-if-present.md`#`### 15.1 Versioning and changelog`.
Prerequisites: run `mise use --env local --pin rust@1.85.0` to install and select Rust 1.85.0 with
its bundled Cargo. Keep the generated `mise.local.toml` as an untracked local setup file and never
stage or commit it. Run every Rust or Cargo command through `mise exec -E local --`.
May change: `Cargo.toml`, `Cargo.lock`, `src/main.rs`, `LICENSE-MIT`, `LICENSE-APACHE`.
Done when: the empty binary package resolves exactly the specified runtime dependencies, declares
the required package metadata and sole version, includes both license texts, and builds with Rust
1.85.0.
Shown by: check — run `mise exec -E local -- cargo metadata --locked --no-deps`, then
`mise exec -E local -- cargo build --locked`; inspect the metadata for name, version, description,
license, rust-version, and exact dependency requirements.
Left to the implementer: Cargo manifest key ordering and the minimal placeholder shape of
`src/main.rs`.
Stop and hand back if: resolving the specified exact crates changes the required compiler floor,
or the canonical license texts cannot be sourced from their official license repositories.

## Step 2 — Parse the public CLI without consuming child input

Purpose: Pin the entire public grammar and syntax-error boundary before runtime evaluation begins.
Specification: `docs/spec/run-if-present.md`#`### 4.1 Grammar`,
`docs/spec/run-if-present.md`#`### 4.2 Public options and empty values`,
`docs/spec/run-if-present.md`#`## 10. Output and exit contract`.
Prerequisites: Step 1 is complete.
May change: `src/**`, `tests/**`.
Done when: help, version, both subcommands, separator handling, wrapper-owned empty values, child
options, and empty child arguments match the public grammar using Unix strings without lossy
conversion.
Shown by: test — RED then GREEN then REFACTOR behavior tests named for rejecting an empty wrapper
value, requiring the path separator, preserving a child option after `command`, preserving an
empty child argument, and writing help/version and syntax errors to the specified streams with the
specified exit results; finish with
`mise exec -E local -- cargo test --all-targets --all-features`.
Left to the implementer: private parsed-value types and module layout.
Stop and hand back if: `clap` cannot preserve the grammar or operating-system strings without a
new public option, lossy conversion, or a dependency change.

## Step 3 — Establish the effective directory and expand wrapper-owned tildes

Purpose: Make working-directory and home resolution a deterministic boundary shared by both
conditions. Specification: `docs/spec/run-if-present.md` headings
“## 5. Evaluation order and working directory”, “## 6. Tilde expansion”, and
“## 10. Output and exit contract”.
Prerequisites: Step 2 is complete; tests can invoke the built binary with controlled environment
and filesystem state.
May change: `src/**`, `tests/**`.
Done when: leading-tilde expansion, user-database fallback, effective-directory ordering, and all
absence-versus-failure distinctions in these sections are observable without rewriting executable
or child arguments.
Shown by: test — RED then GREEN then REFACTOR behavior tests named for exact and leading-slash
tilde expansion, literal `~user`, non-empty/unset/empty/non-UTF-8 `HOME`, empty user-database home,
relative guards and `PATH` after `--chdir`, missing directories, non-directories, and inspection or
change-directory failures; finish with
`mise exec -E local -- cargo test --all-targets --all-features`.
Left to the implementer: the private representation of expansion and directory-evaluation results.
Stop and hand back if: user-database fallback cannot be isolated behind a private dependency and
tested with non-empty and empty operating-system-string results without privileged mutation; do
not mutate the host account database or add a test-only public runtime input.

## Step 4 — Resolve and classify optional commands

Purpose: Distinguish confirmed absence from incomplete inspection and unusable candidates before
process replacement. Specification: `docs/spec/run-if-present.md`#`### 7.1 Resolution`,
`docs/spec/run-if-present.md`#`### 7.2 Candidate classification`,
`docs/spec/run-if-present.md`#`## 10. Output and exit contract`.
Prerequisites: Step 3 is complete and supplies the effective directory.
May change: `src/**`, `tests/**`.
Done when: explicit paths and bare names follow their distinct resolution rules, mixed `PATH`
candidates are searched in order, and the three no-selection outcomes retain their specified
priority.
Shown by: test — RED then GREEN then REFACTOR behavior tests named for unset and empty `PATH`,
relative `PATH` entries, explicit absolute and relative executable paths containing separators,
proof that explicit paths are not searched through `PATH`, shell-only names, earlier unusable
candidates, later usable candidates, inspection failure followed by a usable candidate,
inspection failure with no usable candidate, preflight-only failure, and confirmed absence; finish with
`mise exec -E local -- cargo test --all-targets --all-features`.
Left to the implementer: private classifier types and the smallest local supplement around
`which` needed to retain the three outcomes.
Stop and hand back if: `which` behavior makes the specified search ordering or error distinctions
unobservable without replacing the approved dependency or inventing a public behavior.

## Step 5 — Evaluate path guards and replace the process transparently

Purpose: Complete both execution paths while preserving the selected command's native process
result and making replacement failures visible. Specification: `docs/spec/run-if-present.md`
headings “### 7.3 Resolution-to-execution race”, “## 8. `path` condition”,
“## 9. Process transparency”, and “## 10. Output and exit contract”.
Prerequisites: Step 4 is complete; tests can create executable fixtures and invoke the built binary
as a separate process.
May change: `src/**`, `tests/**`.
Done when: guard classification follows symbolic links as specified, presence commits execution,
successful launch replaces the wrapper, and replacement errors map to 1, 126, or 127 without
resuming optional lookup.
Shown by: test — RED then GREEN then REFACTOR behavior tests named for files, directories, missing
guards, dangling links, link loops, inaccessible guards, missing and uninvokable launch targets,
disappearing selected executables, missing script interpreters, executable-format failure,
environment/stream/argument preservation, unchanged real and effective credentials, unchanged
resource limits, close-on-exec descriptor closure, child exit status and signals, and non-UTF-8
Unix input; finish with `mise exec -E local -- cargo test --all-targets --all-features`.
Left to the implementer: deterministic synchronization used to trigger the resolution-to-execution
race and the private boundary around Unix process replacement.
Stop and hand back if: a required failure cannot be triggered deterministically without privileged
filesystem changes, or a platform maps an OS error incompatibly with the specification.

## Step 6 — Make diagnostics safe and prove the runtime has no hidden surface

Purpose: Finish the wrapper-owned output contract and mechanically exclude accidental runtime
features. Specification: `docs/spec/run-if-present.md`#`## 10. Output and exit contract`,
`docs/spec/run-if-present.md`#`## 11. Runtime boundaries`,
`docs/spec/run-if-present.md`#`## 17. Not built in 0.1`.
Prerequisites: Step 5 is complete and all wrapper error paths are reachable by tests.
May change: `src/**`, `tests/**`.
Done when: every wrapper failure emits one neutral colorless line with an escaped operand and
stable prefix/operation, confirmed absence stays silent, and no excluded runtime surface is
present.
Shown by: test — RED then GREEN then REFACTOR behavior tests named for newline/control-byte and
non-UTF-8 operand escaping, the OS-error component, one-line diagnostics, silent absence, and
colorless output; add a source-boundary test that scans `src/**` and the locked normal-dependency
tree for configuration readers, persistent writes, telemetry or network APIs, shell invocation,
and credential or privilege-changing APIs, reviewing each match against the only allowed
`HOME`/`PATH` reads and direct target execution. Inspect `--help` in the same test for excluded
options, then finish with `mise exec -E local -- cargo test --all-targets --all-features`.
Left to the implementer: the exact escaping notation, provided it is unambiguous, one-line, and
lossless enough to distinguish escaped operating-system bytes.
Stop and hand back if: an escaping choice would add a stable public format beyond the prefix and
operation guaranteed by the specification.

## Step 7 — Document the proven 0.1 behavior and visible changes

Purpose: Give users an accurate installation, prediction, verification, and licensing guide for
the behavior already implemented. Specification: `docs/spec/run-if-present.md` headings
“## 14. Public documentation”, “### 15.1 Versioning and changelog”, “### 15.2 Artifacts”, and
“## 17. Not built in 0.1”.
Prerequisites: Steps 1–6 are complete; examples must run against the built binary.
May change: `README.md`, `CHANGELOG.md`, `tests/**`.
Done when: the English README covers every required topic without claiming unpublished artifacts,
the changelog contains standalone 0.1 user-visible changes under `Unreleased`, and executable
examples agree with the CLI.
Shown by: check — run every README command example that does not require publication against the
local binary, run `mise exec -E local -- cargo test --all-targets --all-features`, and search
README/help/changelog for excluded 0.1 features or unsupported-system claims.
Left to the implementer: prose organization and example operands that preserve the specified
meaning.
Stop and hand back if: documenting installation would require claiming that crates.io, a GitHub
repository, a release, or checksums already exist.

## Step 8 — Encode supported-host and compiler-floor continuous integration

Purpose: Encode the merge checks and platform behavior matrix for later execution on Linux,
macOS, and Rust 1.85. Specification: `docs/spec/run-if-present.md` headings
“## 3. Supported systems”, “## 12. Implementation dependencies”,
“## 13. Verification contract”, and “### 15.3 Automation and credentials”.
Prerequisites: Steps 1–7 are complete; required tests identify platform-specific skips explicitly.
On this Linux x86_64 implementation host, bootstrap `actionlint` 1.7.12 with the following exact
commands even if another version is installed. Keep `ACTIONLINT_TMP` for the remaining plan checks;
do not install an unverified or `latest` binary.

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
May change: `.github/workflows/**`, `.github/dependabot.yml`, `tests/**`.
Done when: CI pins third-party actions to immutable SHAs, declares explicit least-privilege token
permissions, runs formatting, linting, packaging, and the full behavior suite on Linux and macOS
including Rust 1.85, and Dependabot covers Cargo and Actions weekly with the same checks required on
updates. Non-publication jobs have read-only contents access and no write permission.
Shown by: check — run `"$ACTIONLINT_TMP/actionlint" -version`, require 1.7.12, then run
`"$ACTIONLINT_TMP/actionlint"`; run tests for any local workflow helper, inspect every `uses:`
entry for a full commit SHA, and trace each Section 13 check and supported host/compiler combination
to a required job. Resolve every workflow and job `permissions:` block and verify that no
non-publication job inherits or requests write access. Record the actual hosted-run result as
pending until Step 11.
Left to the implementer: job names, matrix factoring, caching, and the choice of maintained actions
whose full commit SHAs are recorded.
Stop and hand back if: the actionlint release asset fails attestation or checksum verification,
or implementing the workflow requires a new credential or a change to the supported host/compiler
contract. Do not create a remote or run the workflow in this step.

## Step 9 — Build guarded, restartable release automation

Purpose: Produce and verify all release assets from one immutable tag while keeping publication
behind the specified human-controlled boundary. Specification:
`docs/spec/run-if-present.md`#`### 15.1 Versioning and changelog`,
`docs/spec/run-if-present.md`#`### 15.2 Artifacts`,
`docs/spec/run-if-present.md`#`### 15.3 Automation and credentials`,
`docs/spec/run-if-present.md`#`## 16. Approval boundaries`.
Prerequisites: Step 8 is complete; the workflow must consume only the canonical Cargo version and
tagged commit, while the repository changelog remains under `Unreleased` until a separately
authorized release preparation promotes it. In the shell used for this step, rerun the complete
Step 8 `actionlint` bootstrap block to create a new verified `ACTIONLINT_TMP`; do not rely on the
temporary path or shell variable from the earlier step.
May change: `.github/workflows/**`, `.github/**` narrowly scoped helper scripts, `tests/**`.
Done when: a tag-only workflow requires proof that the tagged commit already passed project
checks; verifies tag, Cargo, and changelog agreement; uses exactly Rust 1.85.0 and its bundled Cargo
for every package creation, publication, and retry; derives `SOURCE_DATE_EPOCH` from the tagged
commit; builds and smoke-tests all four named targets; creates uniform archives and checksums;
retains and rechecks one deterministic crate package; and makes the draft-release, crates.io, and
public-release phases restartable and checksum-guarded behind the protected environment. The sole
crates.io credential source is the protected environment secret `CARGO_REGISTRY_TOKEN`, referenced
as `secrets.CARGO_REGISTRY_TOKEN`, and no step prints its value. Every job declares its token
permissions explicitly: verification and artifact-building jobs remain read-only, crates.io
publication receives no GitHub write permission, and the minimum GitHub Release write permission
exists only on its protected publication jobs after environment approval.
Shown by: check — run local tests for release helpers using an isolated fixture whose Cargo version,
`v0.1.0` tag value, and promoted `0.1.0` changelog heading agree, plus mismatch fixtures for every
agreement and checksum stop; run `"$ACTIONLINT_TMP/actionlint" -version`, require 1.7.12, then run
`"$ACTIONLINT_TMP/actionlint"`;
inspect the workflow for the tag-only trigger, prior-check gate, exact toolchain and bundled Cargo,
tagged-commit timestamp, four exact artifact names and layouts, action SHA pins, protected
environment, sole secret source, log-safe command invocation, retained `.crate`, checksum
comparisons, resume conditions, and every workflow/job `permissions:` block. Verify that no write
permission is inherited by pre-approval or non-publication work. Verify that the tag-only workflow and the non-publishing CI
rehearsal call the same reusable jobs or checked-in helper entry points for verification, package
creation, archive creation, and checksums; only the immutable input ref and disabled publication
may differ. Record cross-platform execution and workflow-log inspection as pending until Step 11.
Left to the implementer: workflow decomposition and maintained action selection, provided retry
state is derived from immutable remote checksums rather than mutable local flags.
Stop and hand back if: validating the workflow requires publishing, pushing a real release tag,
creating public infrastructure, weakening the protected approval, exposing a credential, or
overwriting any mismatch.

## Step 10 — Run the complete local acceptance gate

Purpose: Demonstrate that the finished repository is internally consistent and locally
release-ready without crossing a publication boundary. Specification:
`docs/spec/run-if-present.md`#`## 13. Verification contract`,
`docs/spec/run-if-present.md`#`## 15. Version and release contract`,
`docs/spec/run-if-present.md`#`## 16. Approval boundaries`,
`docs/spec/run-if-present.md`#`## 17. Not built in 0.1`.
Prerequisites: Steps 1–9 are locally complete; external CI evidence is recorded as pending rather
than reported as passing.
May change: only files already in scope for Steps 1–9, and only to correct a failed acceptance
check through a new test-first concern; do not hide or waive failures in this step. After any
correction, return to the earliest step that owns a changed file and rerun every subsequent local
step through Step 10, including the Step 8 and Step 9 actionlint and workflow checks whenever
`.github/**`, workflow helpers, or their tests changed.
Done when: all local checks pass on the locked project, package contents are correct, the public
surface contains no excluded features, the working diff contains no credential material or build
artifacts, and every unavailable external result is named explicitly.
Shown by: check — in order run
`mise exec -E local -- cargo test --all-targets --all-features`,
`mise exec -E local -- cargo fmt --check`,
`mise exec -E local -- cargo clippy --all-targets --all-features -- -D warnings`,
`mise exec -E local -- cargo build --release --locked`,
`target/release/run-if-present --version`, `mise exec -E local -- cargo package`, and
`mise exec -E local -- cargo package --list`; then inspect the full scoped diff and scan it for
credential-shaped assignments and excluded public features. Release-archive smoke tests remain
pending for Step 11.
Left to the implementer: none.
Stop and hand back if: any check fails after one changed corrective approach, package contents or
versions disagree, external evidence is being mistaken for local evidence, or completion would
require an action reserved by the approval boundary.

## Step 11 — Run the approval-gated external acceptance matrix

Purpose: Obtain the host and artifact evidence that the local repository cannot produce without an
approved remote, while keeping every publication action disabled. Specification:
`docs/spec/run-if-present.md`#`## 3. Supported systems`,
`docs/spec/run-if-present.md`#`## 13. Verification contract`,
`docs/spec/run-if-present.md`#`### 15.2 Artifacts`,
`docs/spec/run-if-present.md`#`### 15.3 Automation and credentials`,
`docs/spec/run-if-present.md`#`## 16. Approval boundaries`.
Prerequisites: Step 10 is complete, the person has explicitly approved the exact remote operation,
and a suitable remote with required runners exists. Before any publication-capable job is accepted
as protected, a human has configured the named GitHub environment with required-reviewer approval,
placed `CARGO_REGISTRY_TOKEN` only in that environment's secrets, and provided read-only evidence
of those repository-side settings. Without that approval, remote, or protection evidence, stop
after Step 10 and hand back with this step pending.
May change: only files already in scope for Steps 8 and 9, and only through a new locally verified
fix for an observed external failure; no tag, release, or registry state.
Done when: hosted jobs pass the full behavior suite on Linux and macOS including Rust 1.85, build
all four release targets, inspect and smoke-test every archive, verify `SHA256SUMS`, and exercise
the non-publishing release rehearsal through the same reusable jobs or helper entry points used by
the tag-only workflow, without skipped required checks. The rehearsal differs only by using its
approved immutable ref and disabling publication. Repository environment settings show a required
human reviewer blocks publication-capable jobs before secrets or write permissions become
available. Workflow logs show the credential value was neither requested for the rehearsal nor
printed, and the real publication jobs remain protected and unrun.
Shown by: external — inspect the approved GitHub Actions run and downloaded artifacts; require all
matrix jobs to pass, all archive layouts and checksums to agree, every artifact's `--version` smoke
test to pass, action references to remain pinned, the rehearsal and tag workflow to identify the
same reusable implementation, each job's effective token permissions to match its minimum need,
and logs to contain no credential disclosure. Inspect the named
GitHub environment's repository settings and require its human-approval rule and environment-only
secret placement to be visible before accepting the publication boundary.
Left to the implementer: none.
Stop and hand back if: the remote operation lacks current approval, any required runner or target
is unavailable, any job or smoke test fails, any required test is skipped, a credential appears in
logs, or obtaining evidence would create or publish a tag, crate, or release. If an external
failure requires any file change, return to Step 10 and rerun its complete gate on the changed
bytes before rerunning Step 11; previous local and external evidence is invalid for the new bytes.
