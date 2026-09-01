#!/usr/bin/env bash
set -euo pipefail

binary=${1:?binary path is required}
version=${2:?version is required}
target=${3:?target is required}
output=${4:?output directory is required}

archive="run-if-present-v${version}-${target}.tar.gz"
staging=$(mktemp -d)
trap 'rm -rf "$staging"' EXIT
root="$staging/run-if-present-v${version}-${target}"
mkdir -p "$root" "$output"
cp "$binary" "$root/run-if-present"
cp README.md LICENSE-MIT LICENSE-APACHE "$root/"
tar -czf "$output/$archive" -C "$staging" "$(basename "$root")"
printf '%s\n' "$output/$archive"
