# run-if-present

`run-if-present` runs a command only when an explicitly optional command or filesystem path is
present. Confirmed absence is silent success. Inspection errors, unusable programs, launch
failures, and failures from a command that did run remain visible.

The command is useful in hooks where a dependency is genuinely optional, but a broken installed
dependency must still fail the hook.

## Install

To install the checked-out source:

```sh
cargo install --path .
```

After version 0.1.0 is published to crates.io, it can be installed with:

```sh
cargo install run-if-present --version 0.1.0
```

GitHub release archives, once published, contain the executable, this README, and both license
files. Choose the target that matches the output of `uname -m` and `uname -s`:

| System | Target archive suffix |
| --- | --- |
| x86_64 Linux | `x86_64-unknown-linux-musl` |
| arm64 or aarch64 Linux | `aarch64-unknown-linux-musl` |
| x86_64 macOS | `x86_64-apple-darwin` |
| arm64 macOS | `aarch64-apple-darwin` |

Download that one archive and `SHA256SUMS` from the same GitHub Release. For example, after
downloading the x86_64 Linux files for version 0.1.0, verify only the selected archive, extract it,
and install the executable in a directory on `PATH`:

```sh
archive=run-if-present-v0.1.0-x86_64-unknown-linux-musl.tar.gz
awk -v archive="$archive" '$2 == archive { print }' SHA256SUMS | sha256sum -c -
tar -xzf "$archive"
mkdir -p "$HOME/.local/bin"
install -m 0755 "${archive%.tar.gz}/run-if-present" "$HOME/.local/bin/run-if-present"
run-if-present --version
```

On macOS, use the same selection command with `shasum -a 256 -c -` in place of
`sha256sum -c -`. Initial macOS archives are unsigned and unnotarized; review the checksum and your
local security policy before running one. `cargo install` is the source-built alternative.

## Usage

Run an external command only when it can be resolved through `PATH`:

```sh
run-if-present command printf '%s\n' available
```

Run a command when any followed filesystem entry exists:

```sh
run-if-present path ./Cargo.toml -- printf '%s\n' present
```

Change directory before evaluating relative guards, executables, and relative `PATH` entries:

```sh
run-if-present --chdir . path ./Cargo.toml -- printf '%s\n' present
```

The `--` separator is required in `path` mode. In `command` mode, every token after the command
name belongs to that command, including options and empty arguments.

Only an exact `~` or leading `~/` in `--chdir` and path guards is expanded. Executable names and
child arguments are never rewritten.

## Results and diagnostics

- A confirmed-absent optional command, path, or `--chdir` directory exits 0 without output.
- Invalid syntax exits 2 and writes to standard error.
- Inspection, home expansion, or directory changes that fail exit 1 with one diagnostic.
- A program found but not invokable exits 126 with one diagnostic.
- A required launch target missing after the condition is present exits 127 with one diagnostic.
- A launched command replaces the wrapper, so its output, exit status, terminating signal,
  environment, streams, and arguments pass through unchanged.

The wrapper resolves external executables only. It does not evaluate shell aliases, functions,
builtins, pipelines, redirections, or other shell syntax. Invoke a shell explicitly if that is
really the command you intend to run.

## Supported systems

Version 0.1 supports Linux and macOS on x86_64 and aarch64. Windows is not supported.

## License

Licensed under either the MIT License or the Apache License 2.0, at your option. See
`LICENSE-MIT` and `LICENSE-APACHE`.
