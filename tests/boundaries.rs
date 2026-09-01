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
