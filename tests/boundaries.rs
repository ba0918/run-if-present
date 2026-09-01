use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;

fn normal_dependency_graph_is_approved(tree: &str) -> bool {
    const APPROVED: [&str; 19] = [
        "anstream v1.0.0",
        "anstyle v1.0.14",
        "anstyle-parse v1.0.0",
        "anstyle-query v1.1.5",
        "clap v4.6.6",
        "clap_builder v4.6.6",
        "clap_derive v4.6.4",
        "clap_lex v1.1.0",
        "colorchoice v1.0.5",
        "heck v0.5.0",
        "is_terminal_polyfill v1.70.2",
        "libc v0.2.189",
        "proc-macro2 v1.0.107",
        "quote v1.0.47",
        "strsim v0.11.1",
        "syn v3.0.4",
        "unicode-ident v1.0.24",
        "utf8parse v0.2.2",
        "which v8.0.6",
    ];
    let approved = APPROVED
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let actual = tree
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let name = fields.next()?;
            let version = fields.next()?;
            (name != "run-if-present").then(|| format!("{name} {version}"))
        })
        .collect::<BTreeSet<_>>();
    actual == approved
}

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
    assert!(normal_dependency_graph_is_approved(&tree));

    let tree_with_unknown_crate = format!("{tree}unknown-runtime-crate v1.0.0\n");
    assert!(!normal_dependency_graph_is_approved(
        &tree_with_unknown_crate
    ));

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
