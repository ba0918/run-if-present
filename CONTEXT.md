# Glossary

Terms whose meaning in this project differs from, or is narrower than, their everyday reading.
The specification (`docs/spec/run-if-present.md`, Section 2) is the authority; this file records
the reading and the words not to use in prose. Fixed diagnostic phrases that the specification
freezes (such as `command not found`) are contract text, not prose, and are exempt.

## Optional condition

One of exactly three things whose confirmed absence the wrapper turns into a silent exit 0: the
`path` guard, the executable named in `command` mode, and the target directory of `--chdir`.
The launch target after a present `path` guard is not one.

Do not use: "optional path", "optional argument", "skip condition", "guard" for anything other
than the `path` operand.

## Confirmed absence

The wrapper completed the relevant filesystem or executable search and found no candidate,
including the operating system reporting that a component of the path is not a directory. An
entry that exists at the path but is of the wrong kind (a regular file where a directory is
required) is not absence. It never means that inspection failed.

Do not use: "not found" (ambiguous between absence and an inspection failure), "missing" for an
inspection failure, "skipped" for a wrapper failure.

## Inspection failure

The wrapper could not determine whether something exists or is usable: a permission error, a
symbolic-link loop, or any other error that is neither a missing entry nor a missing path
component. Always reported, never optional.

Do not use: "absent", "not found".

## Wrapper failure and executed-command result

A wrapper failure happens before process replacement, or when replacement itself fails; it has a
wrapper-defined exit status and a diagnostic. After replacement the result belongs to the
executed command and the wrapper translates nothing.

Do not use: "the command failed" for a wrapper failure, "the wrapper failed" for a child's exit
status.
