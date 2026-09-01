#!/usr/bin/env bash
set -euo pipefail

expected=${1:?expected version is required}
manifest=${2:?Cargo.toml path is required}

manifest_version=$(awk '
  /^\[package\]$/ { package = 1; next }
  /^\[/ { package = 0 }
  package && /^version = "/ { gsub(/^version = "|"$/, ""); print; exit }
' "$manifest")

if [[ -z "$manifest_version" || "$expected" != "$manifest_version" ]]; then
  echo "release metadata: requested version and Cargo version disagree" >&2
  exit 1
fi

printf '%s\n' "$manifest_version"
