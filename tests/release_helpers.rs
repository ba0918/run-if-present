use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn fixture() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("run-if-present-release-{nonce}"));
    fs::create_dir(&root).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"run-if-present\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\n## Unreleased\n\n### Added\n\n## [0.1.0] - 2024-01-02\n\n### Added\n\n- A promoted item.\n\n[0.1.0]: https://example.invalid/compare/v0.0.0...v0.1.0\n",
    )
    .unwrap();
    root
}

fn metadata(root: &Path, tag: &str) -> std::process::Output {
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

fn package_version(root: &Path, version: &str) -> std::process::Output {
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

fn reconcile_release(root: &Path) -> std::process::Output {
    let expected = root.join("expected");
    let mut path = root.join("bin").into_os_string();
    path.push(":");
    path.push(std::env::var_os("PATH").unwrap());
    Command::new("bash")
        .arg(".github/scripts/reconcile-github-release.sh")
        .args(["v0.1.0", expected.to_str().unwrap()])
        .env("PATH", path)
        .env("GH_STATE", root.join("github-state"))
        .env("GH_LOG", root.join("github.log"))
        .output()
        .unwrap()
}

fn publish_release(root: &Path) -> std::process::Output {
    let expected = root.join("expected");
    let mut path = root.join("bin").into_os_string();
    path.push(":");
    path.push(std::env::var_os("PATH").unwrap());
    Command::new("bash")
        .arg(".github/scripts/reconcile-github-release.sh")
        .args(["v0.1.0", expected.to_str().unwrap(), "--publish"])
        .env("PATH", path)
        .env("GH_STATE", root.join("github-state"))
        .env("GH_LOG", root.join("github.log"))
        .output()
        .unwrap()
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

fn yaml_step_blocks(job: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current = None;

    for line in job.lines() {
        if line.starts_with("      - ") {
            if let Some(block) = current.replace(vec![line.to_owned()]) {
                blocks.push(block.join("\n"));
            }
        } else if let Some(block) = current.as_mut() {
            block.push(line.to_owned());
        }
    }
    if let Some(block) = current {
        blocks.push(block.join("\n"));
    }
    blocks
}

fn yaml_child_block(parent: &str, indentation: usize, key: &str) -> Option<String> {
    let header = format!("{}{key}:", " ".repeat(indentation));
    let scalar_header = format!("{header} ");
    let mut lines = parent.lines();
    let first_line = lines.find(|line| *line == header || line.starts_with(&scalar_header))?;

    let mut block = vec![first_line.to_owned()];
    for line in lines {
        let leading_spaces = line.bytes().take_while(|byte| *byte == b' ').count();
        if !line.is_empty() && leading_spaces <= indentation {
            break;
        }
        block.push(line.to_owned());
    }
    Some(block.join("\n"))
}

fn registry_token_scope_is_valid(workflow: &str) -> bool {
    const TOKEN_REFERENCE: &str = "${{ secrets.CARGO_REGISTRY_TOKEN }}";
    const PUBLISH_STEP_NAME: &str = "- name: Publish only a missing matching crate";

    if workflow.matches(TOKEN_REFERENCE).count() != 1 {
        return false;
    }
    let Some(publish_job) = yaml_job_block(workflow, "publish-crate") else {
        return false;
    };
    if publish_job.contains(&format!(
        "    env:\n      CARGO_REGISTRY_TOKEN: {TOKEN_REFERENCE}"
    )) {
        return false;
    }

    let steps = yaml_step_blocks(&publish_job);
    let Some(publish_step) = steps
        .iter()
        .find(|step| step.lines().next() == Some(&format!("      {PUBLISH_STEP_NAME}")))
    else {
        return false;
    };

    let Some(environment) = yaml_child_block(publish_step, 8, "env") else {
        return false;
    };
    let Some(run) = yaml_child_block(publish_step, 8, "run") else {
        return false;
    };
    let token_entry = format!("          CARGO_REGISTRY_TOKEN: {TOKEN_REFERENCE}");

    publish_step.matches(TOKEN_REFERENCE).count() == 1
        && environment
            .lines()
            .filter(|line| *line == token_entry)
            .count()
            == 1
        && environment.matches(TOKEN_REFERENCE).count() == 1
        && !run.contains(TOKEN_REFERENCE)
        && run.contains("cargo publish --locked")
        && steps
            .iter()
            .filter(|step| *step != publish_step)
            .all(|step| !step.contains(TOKEN_REFERENCE))
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

    let root = fixture();
    let marker = root.join("must-not-exist");
    let malicious = format!("$(touch {})", marker.display());
    let status = Command::new("bash")
        .args(["-c", "[[ \"$REF\" =~ ^[0-9a-f]{40}$ ]]"])
        .env("REF", malicious)
        .status()
        .unwrap();
    assert!(!status.success());
    assert!(!marker.exists());
    fs::remove_dir_all(root).unwrap();
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
    let publish_job = workflow.split("  publish-release:").nth(1).unwrap();

    assert!(publish_job.contains("name: release-assets"));
    assert!(publish_job.contains("path: release-assets"));
    assert!(publish_job.contains("reconcile-github-release.sh"));
    assert!(publish_job.contains("--publish"));
    assert!(!publish_job.contains("gh release edit"));
}

#[test]
fn registry_token_is_scoped_only_to_the_cargo_publish_step() {
    let workflow = fs::read_to_string(".github/workflows/release.yml").unwrap();

    assert!(registry_token_scope_is_valid(&workflow));
}

#[test]
fn registry_token_scope_rejects_a_token_moved_to_a_later_step() {
    let workflow = r#"jobs:
  publish-crate:
    steps:
      - name: Publish only a missing matching crate
        run: cargo publish --locked
      - name: Unrelated later step
        env:
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
        run: "true"
  publish-release:
    steps:
      - run: echo must-not-be-part-of-publish-crate
"#;

    assert!(!registry_token_scope_is_valid(workflow));
    let publish_job = yaml_job_block(workflow, "publish-crate").unwrap();
    assert!(!publish_job.contains("must-not-be-part-of-publish-crate"));
}

#[test]
fn registry_token_scope_rejects_a_token_moved_to_the_publish_run_block() {
    let workflow = r#"jobs:
  publish-crate:
    steps:
      - name: Publish only a missing matching crate
        env:
          VERSION: 0.1.0
        run: |
          echo "${{ secrets.CARGO_REGISTRY_TOKEN }}"
          cargo publish --locked
  publish-release:
    steps:
      - run: "true"
"#;

    assert!(!registry_token_scope_is_valid(workflow));
}

#[test]
fn final_publish_rejects_mismatched_assets_without_editing() {
    let root = fixture();
    let expected = root.join("expected");
    fs::create_dir(&expected).unwrap();
    fs::write(expected.join("archive.tar.gz"), b"expected").unwrap();
    github_release_fixture(
        &root,
        r#"{"tagName":"v0.1.0","isDraft":true,"assets":[{"name":"archive.tar.gz"}]}"#,
        b"different",
    );

    let output = publish_release(&root);

    assert!(!output.status.success());
    let operations = fs::read_to_string(root.join("github.log")).unwrap();
    assert!(!operations.contains("release edit"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn final_publish_edits_a_matching_draft_once() {
    let root = fixture();
    let expected = root.join("expected");
    fs::create_dir(&expected).unwrap();
    fs::write(expected.join("archive.tar.gz"), b"same").unwrap();
    github_release_fixture(
        &root,
        r#"{"tagName":"v0.1.0","isDraft":true,"assets":[{"name":"archive.tar.gz"}]}"#,
        b"same",
    );

    let output = publish_release(&root);

    assert!(output.status.success(), "{:?}", output.stderr);
    let operations = fs::read_to_string(root.join("github.log")).unwrap();
    assert_eq!(operations.matches("release edit").count(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn final_publish_keeps_a_matching_public_release_unchanged() {
    let root = fixture();
    let expected = root.join("expected");
    fs::create_dir(&expected).unwrap();
    fs::write(expected.join("archive.tar.gz"), b"same").unwrap();
    github_release_fixture(
        &root,
        r#"{"tagName":"v0.1.0","isDraft":false,"assets":[{"name":"archive.tar.gz"}]}"#,
        b"same",
    );

    let output = publish_release(&root);

    assert!(output.status.success(), "{:?}", output.stderr);
    let operations = fs::read_to_string(root.join("github.log")).unwrap();
    assert!(!operations.contains("release edit"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn package_version_validation_rejects_shell_syntax_without_executing_it() {
    let root = fixture();
    let marker = root.join("must-not-exist");
    let version = format!("0.1.0$(touch {})", marker.display());

    assert!(package_version(&root, "0.1.0").status.success());
    let output = package_version(&root, &version);

    assert!(!output.status.success());
    assert!(!marker.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn release_metadata_does_not_count_items_from_another_version() {
    let root = fixture();
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\n## Unreleased\n\n### Added\n\n## [0.1.0] - 2024-01-02\n\n## [0.0.9] - 2023-12-01\n\n- An older item.\n\n[0.1.0]: https://example.invalid/compare/v0.0.0...v0.1.0\n",
    )
    .unwrap();

    assert!(!metadata(&root, "v0.1.0").status.success());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn release_metadata_accepts_a_promoted_changelog() {
    let root = fixture();

    let output = metadata(&root, "v0.1.0");

    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(output.stdout, b"0.1.0\n");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn release_metadata_rejects_items_left_in_unreleased() {
    let root = fixture();
    for changelog in [
        "# Changelog\n\n## Unreleased\n\n### Added\n\n- Still unreleased.\n\n## [0.1.0] - 2024-01-02\n\n[0.1.0]: https://example.invalid/compare/v0.0.0...v0.1.0\n",
        "# Changelog\n\n## Unreleased\n\n### Added\n\n- Still unreleased.\n\n## [0.1.0] - 2024-01-02\n\n### Added\n\n- A promoted item.\n\n[0.1.0]: https://example.invalid/compare/v0.0.0...v0.1.0\n",
    ] {
        fs::write(root.join("CHANGELOG.md"), changelog).unwrap();
        assert!(!metadata(&root, "v0.1.0").status.success(), "{changelog}");
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn release_metadata_requires_a_dated_heading() {
    let root = fixture();
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\n## Unreleased\n\n## [0.1.0]\n\n- A promoted item.\n\n[0.1.0]: https://example.invalid/compare/v0.0.0...v0.1.0\n",
    )
    .unwrap();

    assert!(!metadata(&root, "v0.1.0").status.success());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn release_metadata_requires_a_comparison_link_ending_in_the_tag() {
    let root = fixture();

    for link in [
        "[0.1.0]: https://example.invalid/releases/tag/v0.1.0",
        "[0.1.0]: https://example.invalid/compare/v0.0.0...v0.1.1",
        "[0.1.1]: https://example.invalid/compare/v0.0.0...v0.1.0",
        "[0x1y0]: https://example.invalid/compare/v0.0.0...v0x1y0",
    ] {
        write_changelog_link(&root, link);
        assert!(!metadata(&root, "v0.1.0").status.success(), "{link}");
    }

    write_changelog_link(
        &root,
        "[0.1.0]: https://code.example/owner/project/compare/release-0.0...v0.1.0",
    );
    assert!(metadata(&root, "v0.1.0").status.success());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn release_metadata_rejects_a_tag_mismatch() {
    let root = fixture();
    assert!(!metadata(&root, "v0.1.1").status.success());
    assert!(!metadata(&root, "0.1.0").status.success());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn release_metadata_rejects_a_changelog_mismatch() {
    let root = fixture();
    fs::write(root.join("CHANGELOG.md"), "# Changelog\n\n## Unreleased\n").unwrap();
    assert!(!metadata(&root, "v0.1.0").status.success());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn checksum_guard_accepts_equal_files_and_rejects_a_mismatch() {
    let root = fixture();
    let expected = root.join("expected");
    let actual = root.join("actual");
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
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_partial_draft_keeps_matching_assets_and_selects_only_missing_assets() {
    let root = fixture();
    let expected = root.join("expected");
    let existing = root.join("existing");
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
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_public_release_with_matching_assets_is_a_noop() {
    let root = fixture();
    let expected = root.join("expected");
    fs::create_dir(&expected).unwrap();
    fs::write(expected.join("archive.tar.gz"), b"same").unwrap();
    github_release_fixture(
        &root,
        r#"{"tagName":"v0.1.0","isDraft":false,"assets":[{"name":"archive.tar.gz"}]}"#,
        b"same",
    );

    let output = reconcile_release(&root);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let operations = fs::read_to_string(root.join("github.log")).unwrap();
    assert!(!operations.contains("release upload"));
    assert!(!operations.contains("release create"));
    assert!(!operations.contains("release edit"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_public_release_rejects_tag_and_asset_mismatches() {
    for (tag, remote_asset) in [("v0.1.1", b"same".as_slice()), ("v0.1.0", b"different")] {
        let root = fixture();
        let expected = root.join("expected");
        fs::create_dir(&expected).unwrap();
        fs::write(expected.join("archive.tar.gz"), b"same").unwrap();
        github_release_fixture(
            &root,
            &format!(
                r#"{{"tagName":"{tag}","isDraft":false,"assets":[{{"name":"archive.tar.gz"}}]}}"#
            ),
            remote_asset,
        );

        let output = reconcile_release(&root);

        assert!(!output.status.success(), "{tag}");
        let operations = fs::read_to_string(root.join("github.log")).unwrap();
        assert!(!operations.contains("release upload"));
        assert!(!operations.contains("release create"));
        assert!(!operations.contains("release edit"));
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn a_draft_release_uploads_only_missing_assets() {
    let root = fixture();
    let expected = root.join("expected");
    fs::create_dir(&expected).unwrap();
    fs::write(expected.join("archive.tar.gz"), b"same").unwrap();
    fs::write(expected.join("missing.tar.gz"), b"missing").unwrap();
    github_release_fixture(
        &root,
        r#"{"tagName":"v0.1.0","isDraft":true,"assets":[{"name":"archive.tar.gz"}]}"#,
        b"same",
    );

    let output = reconcile_release(&root);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let operations = fs::read_to_string(root.join("github.log")).unwrap();
    assert!(operations.contains("release upload v0.1.0"));
    assert!(root.join("github-state/assets/missing.tar.gz").exists());
    assert_eq!(
        fs::read(root.join("github-state/assets/archive.tar.gz")).unwrap(),
        b"same"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn only_an_absent_release_is_created_after_lookup_failure() {
    for (mode, should_create) in [("not-found", true), ("failure", false)] {
        let root = fixture();
        let expected = root.join("expected");
        fs::create_dir(&expected).unwrap();
        fs::write(expected.join("archive.tar.gz"), b"same").unwrap();
        github_release_fixture(&root, "{}", b"same");
        fs::write(root.join("github-state/view-mode"), mode).unwrap();

        let output = reconcile_release(&root);
        let operations = fs::read_to_string(root.join("github.log")).unwrap();

        assert_eq!(output.status.success(), should_create, "{mode}");
        assert_eq!(
            operations.contains("release create"),
            should_create,
            "{mode}"
        );
        assert!(!operations.contains("release upload"), "{mode}");
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn an_unexpected_remote_asset_stops_without_mutation() {
    let root = fixture();
    let expected = root.join("expected");
    fs::create_dir(&expected).unwrap();
    fs::write(expected.join("archive.tar.gz"), b"same").unwrap();
    github_release_fixture(
        &root,
        r#"{"tagName":"v0.1.0","isDraft":false,"assets":[{"name":"archive.tar.gz"},{"name":"unexpected"}]}"#,
        b"same",
    );
    fs::write(root.join("github-state/assets/unexpected"), b"unexpected").unwrap();

    let output = reconcile_release(&root);

    assert!(!output.status.success());
    let operations = fs::read_to_string(root.join("github.log")).unwrap();
    assert!(!operations.contains("release upload"));
    assert!(!operations.contains("release create"));
    assert!(!operations.contains("release edit"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn release_archive_has_the_uniform_layout_and_a_fixed_timestamp() {
    let root = fixture();
    let first = root.join("first");
    let second = root.join("second");
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
        .args(["-tzf"])
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

    let extracted = root.join("extracted");
    fs::create_dir(&extracted).unwrap();
    let extraction = Command::new("tar")
        .args(["-xzf"])
        .arg(&archive)
        .args(["-C"])
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
    fs::remove_dir_all(root).unwrap();
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
    let archive_path = archives
        .find("ARCHIVE=\"release-archive/run-if-present-v$VERSION-$TARGET.tar.gz\"")
        .unwrap();
    for expected_entry in [
        "\"$ROOT/\"",
        "\"$ROOT/run-if-present\"",
        "\"$ROOT/README.md\"",
        "\"$ROOT/LICENSE-MIT\"",
        "\"$ROOT/LICENSE-APACHE\"",
    ] {
        assert!(
            archives.contains(expected_entry),
            "missing {expected_entry}"
        );
    }
    assert!(archives.contains("EXTRACTED=$(mktemp -d)"));
    let extraction = archives.find("tar -xzf \"$ARCHIVE\"").unwrap();
    let contained_smoke = archives
        .find("\"$EXTRACTED/run-if-present-v$VERSION-$TARGET/run-if-present\" --version")
        .unwrap();
    let upload = archives
        .find("uses: actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02")
        .unwrap();

    assert!(creation < archive_path);
    assert!(archive_path < extraction);
    assert!(extraction < contained_smoke);
    assert!(contained_smoke < upload);
}
