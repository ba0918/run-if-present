use std::collections::BTreeSet;
use std::fs;
use std::process::Command;

const APPROVED_RUNTIME_CRATES: [&str; 19] = [
    "anstream",
    "anstyle",
    "anstyle-parse",
    "anstyle-query",
    "clap",
    "clap_builder",
    "clap_derive",
    "clap_lex",
    "colorchoice",
    "heck",
    "is_terminal_polyfill",
    "libc",
    "proc-macro2",
    "quote",
    "strsim",
    "syn",
    "unicode-ident",
    "utf8parse",
    "which",
];

fn runtime_source() -> String {
    ["src/main.rs", "src/cli.rs", "src/runtime.rs"]
        .into_iter()
        .map(|path| fs::read_to_string(path).unwrap())
        .collect()
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
        assert!(!help.contains("-h,"), "{arguments:?}");
        assert!(!help.contains("-V,"), "{arguments:?}");
    }
}

#[test]
fn source_has_no_hidden_runtime_surface() {
    let source = runtime_source();

    for excluded in [
        "TcpStream",
        "UdpSocket",
        "std::net",
        "reqwest",
        "telemetry",
        "setuid",
        "setgid",
        "sh -c",
        "/bin/sh",
        "Command::new",
        "config file",
        "config_dir",
        "File::create",
        "fs::write",
        "OpenOptions",
    ] {
        assert!(
            !source.contains(excluded),
            "unexpected runtime surface {excluded}"
        );
    }
}

#[test]
fn locked_normal_dependency_tree_contains_only_approved_crates() {
    let output = Command::new("cargo")
        .args(["tree", "--locked", "--edges", "normal", "--prefix", "none"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let tree = String::from_utf8(output.stdout).unwrap();

    let actual: BTreeSet<_> = tree
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| *name != "run-if-present")
        .collect();
    let approved: BTreeSet<_> = APPROVED_RUNTIME_CRATES.into_iter().collect();
    assert_eq!(actual, approved);
}
