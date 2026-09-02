use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

mod common;

use common::TempDir;

const PROMOTED_CHANGELOG: &str = "# Changelog\n\n## Unreleased\n\n### Added\n\n## [0.1.0] - 2024-01-02\n\n### Added\n\n- A promoted item.\n\n[0.1.0]: https://example.invalid/compare/v0.0.0...v0.1.0\n";

fn fixture() -> TempDir {
    let root = TempDir::new();
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"run-if-present\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::write(root.path().join("CHANGELOG.md"), PROMOTED_CHANGELOG).unwrap();
    root
}

fn metadata(root: &Path, tag: &str) -> Output {
    Command::new("bash")
        .arg(".github/scripts/verify-release-metadata.sh")
        .arg(tag)
        .arg(root.join("Cargo.toml"))
        .arg(root.join("CHANGELOG.md"))
        .output()
        .unwrap()
}

fn write_changelog_link(root: &Path, link: &str) {
    fs::write(
        root.join("CHANGELOG.md"),
        format!(
            "# Changelog\n\n## Unreleased\n\n### Added\n\n## [0.1.0] - 2024-01-02\n\n### Added\n\n- A promoted item.\n\n{link}\n"
        ),
    )
    .unwrap();
}

fn package_version(root: &Path, version: &str) -> Output {
    Command::new("bash")
        .arg(".github/scripts/verify-package-version.sh")
        .arg(version)
        .arg(root.join("Cargo.toml"))
        .output()
        .unwrap()
}

fn github_release_fixture(root: &Path, release_json: &str, remote_asset: &[u8]) -> PathBuf {
    let bin = root.join("bin");
    let state = root.join("github-state");
    fs::create_dir(&bin).unwrap();
    fs::create_dir(&state).unwrap();
    fs::create_dir(state.join("assets")).unwrap();
    fs::write(state.join("release.json"), release_json).unwrap();
    fs::write(state.join("assets/archive.tar.gz"), remote_asset).unwrap();
    let gh = bin.join("gh");
    fs::write(
        &gh,
        r#"#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "$GH_LOG"
case "$1 $2" in
  "release view")
    mode=success
    [[ ! -f "$GH_STATE/view-mode" ]] || mode=$(cat "$GH_STATE/view-mode")
    if [[ "$mode" == not-found ]]; then echo "release not found" >&2; exit 1; fi
    if [[ "$mode" == failure ]]; then echo "authentication failed" >&2; exit 1; fi
    cat "$GH_STATE/release.json"
    ;;
  "release download")
    shift 2
    while [[ $# -gt 0 ]]; do
      if [[ "$1" == --dir ]]; then destination=$2; break; fi
      shift
    done
    cp "$GH_STATE"/assets/* "$destination/"
    ;;
  "release upload") cp "$4" "$GH_STATE/assets/$(basename "$4")" ;;
  "release create") : ;;
  "release edit") : ;;
  *) exit 64 ;;
esac
"#,
    )
    .unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

fn reconcile_release(root: &Path, mode: &[&str]) -> Output {
    let expected = root.join("expected");
    let mut path = root.join("bin").into_os_string();
    path.push(":");
    path.push(std::env::var_os("PATH").unwrap());
    Command::new("bash")
        .arg(".github/scripts/reconcile-github-release.sh")
        .arg("v0.1.0")
        .arg(expected)
        .args(mode)
        .env("PATH", path)
        .env("GH_STATE", root.join("github-state"))
        .env("GH_LOG", root.join("github.log"))
        .output()
        .unwrap()
}

fn github_operations(root: &Path) -> String {
    fs::read_to_string(root.join("github.log")).unwrap()
}

fn yaml_job_block(workflow: &str, job_name: &str) -> Option<String> {
    let header = format!("  {job_name}:");
    let mut lines = workflow.lines();
    lines.find(|line| *line == header)?;

    let mut block = vec![header];
    for line in lines {
        let starts_another_job = line
            .strip_prefix("  ")
            .is_some_and(|remainder| !remainder.starts_with(' ') && remainder.ends_with(':'));
        if starts_another_job {
            break;
        }
        block.push(line.to_owned());
    }
    Some(block.join("\n"))
}

#[test]
fn release_artifact_shell_steps_do_not_interpolate_the_version_input() {
    let workflow = fs::read_to_string(".github/workflows/release-artifacts.yml").unwrap();
    let direct_uses: Vec<_> = workflow
        .lines()
        .filter(|line| line.contains("${{ inputs.version }}"))
        .collect();

    assert_eq!(direct_uses, ["          VERSION: ${{ inputs.version }}"]);
}

#[test]
fn release_artifact_workflow_validates_ref_before_using_it() {
    let workflow = fs::read_to_string(".github/workflows/release-artifacts.yml").unwrap();
    let direct_uses: Vec<_> = workflow
        .lines()
        .filter(|line| line.contains("${{ inputs.ref }}"))
        .collect();

    assert_eq!(direct_uses, ["          REF: ${{ inputs.ref }}"]);
    assert!(workflow.contains("[[ \"$REF\" =~ ^[0-9a-f]{40}$ ]]"));
}

#[test]
fn reusable_verification_keeps_all_dependency_consumers_locked() {
    let workflow = fs::read_to_string(".github/workflows/verify.yml").unwrap();

    for command in [
        "cargo test --all-targets --all-features --locked",
        "cargo clippy --all-targets --all-features --locked -- -D warnings",
        "cargo package --locked",
    ] {
        assert!(
            workflow.contains(command),
            "missing locked command: {command}"
        );
    }
}

#[test]
fn final_release_job_reconciles_downloaded_assets_before_publishing() {
    let workflow = fs::read_to_string(".github/workflows/release.yml").unwrap();
    let publish_job = yaml_job_block(&workflow, "publish-release").unwrap();

    assert!(publish_job.contains("name: release-assets"));
    assert!(publish_job.contains("path: release-assets"));
    assert!(publish_job.contains("reconcile-github-release.sh"));
    assert!(publish_job.contains("--publish"));
    assert!(!publish_job.contains("gh release edit"));
}

#[test]
fn registry_token_is_read_only_by_the_cargo_publish_step() {
    const TOKEN_REFERENCE: &str = "${{ secrets.CARGO_REGISTRY_TOKEN }}";
    let workflow = fs::read_to_string(".github/workflows/release.yml").unwrap();
    assert_eq!(workflow.matches(TOKEN_REFERENCE).count(), 1);

    let publish_job = yaml_job_block(&workflow, "publish-crate").unwrap();
    let publish_step = publish_job
        .split("\n      - ")
        .find(|step| step.starts_with("name: Publish only a missing matching crate"))
        .unwrap();
    let token_line = publish_step
        .lines()
        .find(|line| line.contains(TOKEN_REFERENCE))
        .unwrap();

    assert_eq!(
        token_line,
        format!("          CARGO_REGISTRY_TOKEN: {TOKEN_REFERENCE}")
    );
    let run = publish_step.split("        run:").nth(1).unwrap();
    assert!(!run.contains(TOKEN_REFERENCE));
    assert!(run.contains("cargo publish --locked"));
}

#[test]
fn publication_regenerates_and_compares_the_crate_without_the_registry_token() {
    const TOKEN_REFERENCE: &str = "${{ secrets.CARGO_REGISTRY_TOKEN }}";
    let workflow = fs::read_to_string(".github/workflows/release.yml").unwrap();
    let publish_job = yaml_job_block(&workflow, "publish-crate").unwrap();
    let regeneration_step = publish_job
        .split("\n      - ")
        .find(|step| step.starts_with("name: Verify regenerated crate matches retained package"))
        .unwrap();
    let publish_step = publish_job
        .split("\n      - ")
        .find(|step| step.starts_with("name: Publish only a missing matching crate"))
        .unwrap();

    assert!(!regeneration_step.contains(TOKEN_REFERENCE));
    assert!(
        regeneration_step.contains("SOURCE_DATE_EPOCH=$(git show -s --format=%ct \"$GITHUB_SHA\")")
    );
    assert!(regeneration_step.contains("cargo package --locked"));
    assert!(regeneration_step.contains(".github/scripts/verify-same-checksum.sh"));
    assert!(regeneration_step.contains("crate-package/run-if-present-$VERSION.crate"));
    assert!(regeneration_step.contains("target/package/run-if-present-$VERSION.crate"));
    assert!(publish_job.find(regeneration_step).unwrap() < publish_job.find(publish_step).unwrap());
}

#[test]
fn publication_fetches_and_compares_the_registry_checksum_after_publishing() {
    let workflow = fs::read_to_string(".github/workflows/release.yml").unwrap();
    let publish_job = yaml_job_block(&workflow, "publish-crate").unwrap();
    let publish_step = publish_job
        .split("\n      - ")
        .find(|step| step.starts_with("name: Publish only a missing matching crate"))
        .unwrap();
    let run = publish_step.split("        run:").nth(1).unwrap();

    assert_eq!(
        run.matches("https://crates.io/api/v1/crates/run-if-present/$VERSION")
            .count(),
        2
    );
    let existing_comparison = run
        .find("[[ \"$registry_checksum\" == \"$local_checksum\" ]]")
        .unwrap();
    let publish = run.find("cargo publish --locked").unwrap();
    let after_publish = &run[publish..];
    let fetch_after_publish = after_publish
        .find("https://crates.io/api/v1/crates/run-if-present/$VERSION")
        .unwrap();
    let require_success = after_publish.find("[[ \"$status\" == 200 ]]").unwrap();
    let compare_after_publish = after_publish
        .rfind("[[ \"$registry_checksum\" == \"$local_checksum\" ]]")
        .unwrap();

    assert!(existing_comparison < publish);
    assert!(fetch_after_publish < require_success);
    assert!(require_success < compare_after_publish);
    assert!(!after_publish.contains("sleep"));
}

#[test]
fn final_publish_rejects_mismatched_assets_without_editing() {
    let root = fixture();
    let expected = root.path().join("expected");
    fs::create_dir(&expected).unwrap();
    fs::write(expected.join("archive.tar.gz"), b"expected").unwrap();
    github_release_fixture(
        root.path(),
        r#"{"tagName":"v0.1.0","isDraft":true,"assets":[{"name":"archive.tar.gz"}]}"#,
        b"different",
    );

    let output = reconcile_release(root.path(), &["--publish"]);

    assert!(!output.status.success());
    assert!(!github_operations(root.path()).contains("release edit"));
}

#[test]
fn final_publish_edits_a_matching_draft_once() {
    let root = fixture();
    let expected = root.path().join("expected");
    fs::create_dir(&expected).unwrap();
    fs::write(expected.join("archive.tar.gz"), b"same").unwrap();
    github_release_fixture(
        root.path(),
        r#"{"tagName":"v0.1.0","isDraft":true,"assets":[{"name":"archive.tar.gz"}]}"#,
        b"same",
    );

    let output = reconcile_release(root.path(), &["--publish"]);

    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(
        github_operations(root.path())
            .matches("release edit")
            .count(),
        1
    );
}

#[test]
fn final_publish_keeps_a_matching_public_release_unchanged() {
    let root = fixture();
    let expected = root.path().join("expected");
    fs::create_dir(&expected).unwrap();
    fs::write(expected.join("archive.tar.gz"), b"same").unwrap();
    github_release_fixture(
        root.path(),
        r#"{"tagName":"v0.1.0","isDraft":false,"assets":[{"name":"archive.tar.gz"}]}"#,
        b"same",
    );

    let output = reconcile_release(root.path(), &["--publish"]);

    assert!(output.status.success(), "{:?}", output.stderr);
    assert!(!github_operations(root.path()).contains("release edit"));
}

#[test]
fn package_version_validation_rejects_shell_syntax_without_executing_it() {
    let root = fixture();
    let marker = root.path().join("must-not-exist");
    let version = format!("0.1.0$(touch {})", marker.display());

    assert!(package_version(root.path(), "0.1.0").status.success());
    let output = package_version(root.path(), &version);

    assert!(!output.status.success());
    assert!(!marker.exists());
}

#[test]
fn release_metadata_accepts_a_promoted_changelog() {
    let root = fixture();

    let output = metadata(root.path(), "v0.1.0");

    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(output.stdout, b"0.1.0\n");
}

#[test]
fn release_metadata_rejects_items_left_in_unreleased() {
    let root = fixture();
    for changelog in [
        "# Changelog\n\n## Unreleased\n\n### Added\n\n- Still unreleased.\n\n## [0.1.0] - 2024-01-02\n\n[0.1.0]: https://example.invalid/compare/v0.0.0...v0.1.0\n",
        "# Changelog\n\n## Unreleased\n\n### Added\n\n- Still unreleased.\n\n## [0.1.0] - 2024-01-02\n\n### Added\n\n- A promoted item.\n\n[0.1.0]: https://example.invalid/compare/v0.0.0...v0.1.0\n",
    ] {
        fs::write(root.path().join("CHANGELOG.md"), changelog).unwrap();
        assert!(!metadata(root.path(), "v0.1.0").status.success(), "{changelog}");
    }
}

#[test]
fn release_metadata_does_not_count_items_from_another_version() {
    let root = fixture();
    fs::write(
        root.path().join("CHANGELOG.md"),
        "# Changelog\n\n## Unreleased\n\n### Added\n\n## [0.1.0] - 2024-01-02\n\n## [0.0.9] - 2023-12-01\n\n- An older item.\n\n[0.1.0]: https://example.invalid/compare/v0.0.0...v0.1.0\n",
    )
    .unwrap();

    assert!(!metadata(root.path(), "v0.1.0").status.success());
}

#[test]
fn release_metadata_requires_a_dated_heading() {
    let root = fixture();
    fs::write(
        root.path().join("CHANGELOG.md"),
        "# Changelog\n\n## Unreleased\n\n## [0.1.0]\n\n- A promoted item.\n\n[0.1.0]: https://example.invalid/compare/v0.0.0...v0.1.0\n",
    )
    .unwrap();

    assert!(!metadata(root.path(), "v0.1.0").status.success());
}

#[test]
fn release_metadata_rejects_a_tag_mismatch() {
    let root = fixture();

    assert!(!metadata(root.path(), "v0.1.1").status.success());
    assert!(!metadata(root.path(), "0.1.0").status.success());
}

#[test]
fn release_metadata_accepts_any_https_link_for_the_released_version() {
    let root = fixture();

    for link in [
        "[0.1.0]: https://example.invalid/releases/tag/v0.1.0",
        "[0.1.0]: https://example.invalid/compare/v0.0.0...v0.1.0",
    ] {
        write_changelog_link(root.path(), link);
        assert!(metadata(root.path(), "v0.1.0").status.success(), "{link}");
    }

    for link in [
        "[0.1.0]: http://example.invalid/releases/tag/v0.1.0",
        "[0.1.1]: https://example.invalid/releases/tag/v0.1.0",
        "",
    ] {
        write_changelog_link(root.path(), link);
        assert!(!metadata(root.path(), "v0.1.0").status.success(), "{link}");
    }
}

#[test]
fn checksum_guard_accepts_equal_files_and_rejects_a_mismatch() {
    let root = fixture();
    let expected = root.path().join("expected");
    let actual = root.path().join("actual");
    fs::write(&expected, b"same").unwrap();
    fs::write(&actual, b"same").unwrap();

    let matches = Command::new("bash")
        .arg(".github/scripts/verify-same-checksum.sh")
        .args([&expected, &actual])
        .status()
        .unwrap();
    assert!(matches.success());

    fs::write(&actual, b"different").unwrap();
    let mismatch = Command::new("bash")
        .arg(".github/scripts/verify-same-checksum.sh")
        .args([&expected, &actual])
        .status()
        .unwrap();
    assert!(!mismatch.success());
}

#[test]
fn a_partial_draft_keeps_matching_assets_and_selects_only_missing_assets() {
    let root = fixture();
    let expected = root.path().join("expected");
    let existing = root.path().join("existing");
    fs::create_dir(&expected).unwrap();
    fs::create_dir(&existing).unwrap();
    fs::write(expected.join("kept.tar.gz"), b"kept").unwrap();
    fs::write(expected.join("missing.tar.gz"), b"missing").unwrap();
    fs::write(existing.join("kept.tar.gz"), b"kept").unwrap();

    let selection = Command::new("bash")
        .arg(".github/scripts/select-missing-release-assets.sh")
        .args([&expected, &existing])
        .output()
        .unwrap();

    assert!(selection.status.success());
    assert_eq!(
        String::from_utf8(selection.stdout).unwrap(),
        format!("{}\n", expected.join("missing.tar.gz").display())
    );
    assert_eq!(fs::read(existing.join("kept.tar.gz")).unwrap(), b"kept");

    fs::copy(
        expected.join("missing.tar.gz"),
        existing.join("missing.tar.gz"),
    )
    .unwrap();
    let completed = Command::new("bash")
        .arg(".github/scripts/select-missing-release-assets.sh")
        .args([&expected, &existing])
        .output()
        .unwrap();
    assert!(completed.status.success());
    assert!(completed.stdout.is_empty());
}

#[test]
fn a_public_release_with_matching_assets_is_a_noop() {
    let root = fixture();
    let expected = root.path().join("expected");
    fs::create_dir(&expected).unwrap();
    fs::write(expected.join("archive.tar.gz"), b"same").unwrap();
    github_release_fixture(
        root.path(),
        r#"{"tagName":"v0.1.0","isDraft":false,"assets":[{"name":"archive.tar.gz"}]}"#,
        b"same",
    );

    let output = reconcile_release(root.path(), &[]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let operations = github_operations(root.path());
    assert!(!operations.contains("release upload"));
    assert!(!operations.contains("release create"));
    assert!(!operations.contains("release edit"));
}

#[test]
fn a_public_release_rejects_tag_and_asset_mismatches() {
    for (tag, remote_asset) in [("v0.1.1", b"same".as_slice()), ("v0.1.0", b"different")] {
        let root = fixture();
        let expected = root.path().join("expected");
        fs::create_dir(&expected).unwrap();
        fs::write(expected.join("archive.tar.gz"), b"same").unwrap();
        github_release_fixture(
            root.path(),
            &format!(
                r#"{{"tagName":"{tag}","isDraft":false,"assets":[{{"name":"archive.tar.gz"}}]}}"#
            ),
            remote_asset,
        );

        let output = reconcile_release(root.path(), &[]);

        assert!(!output.status.success(), "{tag}");
        let operations = github_operations(root.path());
        assert!(!operations.contains("release upload"));
        assert!(!operations.contains("release create"));
        assert!(!operations.contains("release edit"));
    }
}

#[test]
fn a_draft_release_uploads_only_missing_assets() {
    let root = fixture();
    let expected = root.path().join("expected");
    fs::create_dir(&expected).unwrap();
    fs::write(expected.join("archive.tar.gz"), b"same").unwrap();
    fs::write(expected.join("missing.tar.gz"), b"missing").unwrap();
    github_release_fixture(
        root.path(),
        r#"{"tagName":"v0.1.0","isDraft":true,"assets":[{"name":"archive.tar.gz"}]}"#,
        b"same",
    );

    let output = reconcile_release(root.path(), &[]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(github_operations(root.path()).contains("release upload v0.1.0"));
    assert!(root
        .path()
        .join("github-state/assets/missing.tar.gz")
        .exists());
    assert_eq!(
        fs::read(root.path().join("github-state/assets/archive.tar.gz")).unwrap(),
        b"same"
    );
}

#[test]
fn only_an_absent_release_is_created_after_lookup_failure() {
    for (mode, should_create) in [("not-found", true), ("failure", false)] {
        let root = fixture();
        let expected = root.path().join("expected");
        fs::create_dir(&expected).unwrap();
        fs::write(expected.join("archive.tar.gz"), b"same").unwrap();
        github_release_fixture(root.path(), "{}", b"same");
        fs::write(root.path().join("github-state/view-mode"), mode).unwrap();

        let output = reconcile_release(root.path(), &[]);
        let operations = github_operations(root.path());

        assert_eq!(output.status.success(), should_create, "{mode}");
        assert_eq!(
            operations.contains("release create"),
            should_create,
            "{mode}"
        );
        assert!(!operations.contains("release upload"), "{mode}");
    }
}

#[test]
fn an_unexpected_remote_asset_stops_without_mutation() {
    let root = fixture();
    let expected = root.path().join("expected");
    fs::create_dir(&expected).unwrap();
    fs::write(expected.join("archive.tar.gz"), b"same").unwrap();
    github_release_fixture(
        root.path(),
        r#"{"tagName":"v0.1.0","isDraft":false,"assets":[{"name":"archive.tar.gz"},{"name":"unexpected"}]}"#,
        b"same",
    );
    fs::write(
        root.path().join("github-state/assets/unexpected"),
        b"unexpected",
    )
    .unwrap();

    let output = reconcile_release(root.path(), &[]);

    assert!(!output.status.success());
    let operations = github_operations(root.path());
    assert!(!operations.contains("release upload"));
    assert!(!operations.contains("release create"));
    assert!(!operations.contains("release edit"));
}

#[test]
fn release_archive_has_the_uniform_layout_and_a_fixed_timestamp() {
    let root = fixture();
    let first = root.path().join("first");
    let second = root.path().join("second");
    for output_dir in [&first, &second] {
        let status = Command::new("python3")
            .arg(".github/scripts/make-release-archive.py")
            .arg(env!("CARGO_BIN_EXE_run-if-present"))
            .args(["0.1.0", "x86_64-unknown-linux-musl"])
            .arg(output_dir)
            .env("SOURCE_DATE_EPOCH", "1700000000")
            .status()
            .unwrap();
        assert!(status.success());
    }

    let archive = first.join("run-if-present-v0.1.0-x86_64-unknown-linux-musl.tar.gz");
    let regenerated = second.join("run-if-present-v0.1.0-x86_64-unknown-linux-musl.tar.gz");
    let bytes = fs::read(&archive).unwrap();
    assert_eq!(bytes, fs::read(regenerated).unwrap());
    // The gzip header stores the modification time in bytes 4..8, little-endian.
    assert_eq!(bytes[4..8], 1_700_000_000_u32.to_le_bytes());

    let listing = Command::new("tar")
        .arg("-tzf")
        .arg(&archive)
        .output()
        .unwrap();
    assert!(listing.status.success());
    let listing = String::from_utf8(listing.stdout).unwrap();
    assert_eq!(
        listing.lines().collect::<Vec<_>>(),
        [
            "run-if-present-v0.1.0-x86_64-unknown-linux-musl/",
            "run-if-present-v0.1.0-x86_64-unknown-linux-musl/run-if-present",
            "run-if-present-v0.1.0-x86_64-unknown-linux-musl/README.md",
            "run-if-present-v0.1.0-x86_64-unknown-linux-musl/LICENSE-MIT",
            "run-if-present-v0.1.0-x86_64-unknown-linux-musl/LICENSE-APACHE",
        ]
    );

    let extracted = root.path().join("extracted");
    fs::create_dir(&extracted).unwrap();
    let extraction = Command::new("tar")
        .arg("-xzf")
        .arg(&archive)
        .arg("-C")
        .arg(&extracted)
        .status()
        .unwrap();
    assert!(extraction.success());
    let smoke = Command::new(
        extracted
            .join("run-if-present-v0.1.0-x86_64-unknown-linux-musl")
            .join("run-if-present"),
    )
    .arg("--version")
    .output()
    .unwrap();
    assert!(smoke.status.success());
    assert_eq!(smoke.stdout, b"run-if-present 0.1.0\n");
}

#[test]
fn release_workflow_extracts_and_smoke_tests_each_native_archive_before_upload() {
    let workflow = fs::read_to_string(".github/workflows/release-artifacts.yml").unwrap();
    let archives = yaml_job_block(&workflow, "archives").unwrap();

    for native_pair in [
        "runner: ubuntu-24.04\n            target: x86_64-unknown-linux-musl",
        "runner: ubuntu-24.04-arm\n            target: aarch64-unknown-linux-musl",
        "runner: macos-15-intel\n            target: x86_64-apple-darwin",
        "runner: macos-15\n            target: aarch64-apple-darwin",
    ] {
        assert!(archives.contains(native_pair), "missing {native_pair}");
    }

    let creation = archives
        .find(".github/scripts/make-release-archive.py")
        .unwrap();
    let extraction = archives.find("tar -xzf \"$ARCHIVE\"").unwrap();
    let contained_smoke = archives
        .find("\"$EXTRACTED/run-if-present-v$VERSION-$TARGET/run-if-present\" --version")
        .unwrap();
    let upload = archives.find("uses: actions/upload-artifact@").unwrap();

    assert!(creation < extraction);
    assert!(extraction < contained_smoke);
    assert!(contained_smoke < upload);
}
