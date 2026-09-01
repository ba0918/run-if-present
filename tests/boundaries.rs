use std::fs;
use std::path::Path;
use std::process::Command;

#[test]
fn help_exposes_no_excluded_runtime_options() {
    let output = Command::new(env!("CARGO_BIN_EXE_run-if-present"))
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8(output.stdout).unwrap();

    assert!(help.contains("--help"));
    assert!(help.contains("--version"));
    assert!(!help.contains("-h,"));
    assert!(!help.contains("-V,"));

    for excluded in [
        "--verbose",
        "--quiet",
        "--dry-run",
        "--json",
        "--skip-reason",
        "--shell",
        "--file-only",
        "--directory-only",
    ] {
        assert!(
            !help.contains(excluded),
            "unexpected public option {excluded}"
        );
    }
}

#[test]
fn help_exposes_only_the_approved_options() {
    for (arguments, expected) in [
        (&["--help"][..], vec!["--chdir", "--help", "--version"]),
        (&["path", "--help"][..], vec!["--help"]),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_run-if-present"))
            .args(arguments)
            .output()
            .unwrap();
        assert!(output.status.success());
        let help = String::from_utf8(output.stdout).unwrap();
        let options: Vec<_> = help
            .lines()
            .filter_map(|line| {
                line.split_whitespace()
                    .find(|word| word.starts_with("--") && word.len() > 2)
            })
            .collect();
        assert_eq!(options, expected, "{arguments:?}");
    }
}

#[test]
fn source_has_no_hidden_runtime_surface() {
    let source = ["src/main.rs", "src/cli.rs", "src/runtime.rs"]
        .into_iter()
        .map(|path| fs::read_to_string(path).unwrap())
        .collect::<String>();

    for excluded in [
        "TcpStream",
        "UdpSocket",
        "reqwest",
        "telemetry",
        "setuid",
        "setgid",
        "sh -c",
        "config file",
    ] {
        assert!(
            !source.contains(excluded),
            "unexpected runtime surface {excluded}"
        );
    }

    let manifest = fs::read_to_string(Path::new("Cargo.toml")).unwrap();
    assert!(manifest.contains("clap ="));
    assert!(manifest.contains("which ="));
}

#[test]
fn locked_normal_dependency_tree_has_no_runtime_io_surface() {
    let output = Command::new("cargo")
        .args(["tree", "--locked", "--edges", "normal", "--prefix", "none"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let tree = String::from_utf8(output.stdout).unwrap();
    for forbidden in [
        "reqwest",
        "ureq",
        "tokio",
        "serde",
        "config",
        "tracing",
        "telemetry",
    ] {
        assert!(
            !tree.contains(forbidden),
            "unexpected dependency {forbidden}"
        );
    }

    let source = ["src/main.rs", "src/cli.rs", "src/runtime.rs"]
        .into_iter()
        .map(|path| fs::read_to_string(path).unwrap())
        .collect::<String>();
    for forbidden in [
        "File::create",
        "fs::write",
        "OpenOptions",
        "Command::new",
        "/bin/sh",
        "std::net",
        "config_dir",
    ] {
        assert!(
            !source.contains(forbidden),
            "unexpected runtime API {forbidden}"
        );
    }
}
