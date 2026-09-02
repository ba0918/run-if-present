# Changelog

## Unreleased

### Added

- Added `command` and `path` conditions that treat only confirmed absence as silent success while
  preserving inspection, launch, and executed-command failures. Paths that pass through a regular
  file and dangling symbolic links in `PATH` are treated as absent candidates.
- Added `--chdir` and wrapper-owned tilde expansion so relative conditions and relative `PATH`
  entries are evaluated from an explicitly selected directory. A path through a regular file is
  confirmed absent, while a target that is itself a regular file exits 1 with a diagnostic.
- Added subcommand help that recognises `--help` only as the final operand; trailing tokens are
  invalid syntax and exit 2.
- Added process-transparent Unix execution with operating-system-string argument handling and
  one-line escaped diagnostics.
- Added support for Linux and macOS on x86_64 and aarch64, distributed under
  `MIT OR Apache-2.0`.
