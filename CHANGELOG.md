# Changelog

## Unreleased

### Added

- Added `command` and `path` conditions that treat only confirmed absence as silent success while
  preserving inspection, launch, and executed-command failures.
- Added `--chdir` and wrapper-owned tilde expansion so relative conditions and relative `PATH`
  entries are evaluated from an explicitly selected directory.
- Added process-transparent Unix execution with operating-system-string argument handling and
  one-line escaped diagnostics.
- Added support for Linux and macOS on x86_64 and aarch64, distributed under
  `MIT OR Apache-2.0`.
