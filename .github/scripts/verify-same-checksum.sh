#!/usr/bin/env bash
set -euo pipefail

first=${1:?first file is required}
second=${2:?second file is required}

checksum() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

first_checksum=$(checksum "$first")
second_checksum=$(checksum "$second")

if [[ "$first_checksum" != "$second_checksum" ]]; then
  echo "checksum mismatch" >&2
  exit 1
fi

printf '%s\n' "$first_checksum"
