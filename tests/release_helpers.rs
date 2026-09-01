use std::fs;
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
        "# Changelog\n\n## [0.1.0] - 2024-01-02\n\n[0.1.0]: https://example.invalid/compare/v0.0.0...v0.1.0\n",
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

fn package_version(root: &Path, version: &str) -> std::process::Output {
    Command::new("bash")
        .arg(".github/scripts/verify-package-version.sh")
        .arg(version)
        .arg(root.join("Cargo.toml"))
        .output()
        .unwrap()
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
fn release_metadata_accepts_a_fixed_release_date_on_retries() {
    let root = fixture();
    assert!(metadata(&root, "v0.1.0").status.success());
    assert!(metadata(&root, "v0.1.0").status.success());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn release_metadata_rejects_a_tag_mismatch() {
    let root = fixture();
    assert!(!metadata(&root, "v0.1.1").status.success());
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
fn release_archive_has_the_uniform_layout() {
    let root = fixture();
    let output_dir = root.join("output");
    let status = Command::new("bash")
        .arg(".github/scripts/make-release-archive.sh")
        .arg(env!("CARGO_BIN_EXE_run-if-present"))
        .args(["0.1.0", "x86_64-unknown-linux-musl"])
        .arg(&output_dir)
        .status()
        .unwrap();
    assert!(status.success());

    let archive = output_dir.join("run-if-present-v0.1.0-x86_64-unknown-linux-musl.tar.gz");
    let listing = Command::new("tar")
        .args(["-tzf"])
        .arg(archive)
        .output()
        .unwrap();
    assert!(listing.status.success());
    let listing = String::from_utf8(listing.stdout).unwrap();
    for name in [
        "run-if-present",
        "README.md",
        "LICENSE-MIT",
        "LICENSE-APACHE",
    ] {
        assert!(
            listing.lines().any(|entry| entry.ends_with(name)),
            "missing {name}"
        );
    }
    fs::remove_dir_all(root).unwrap();
}
