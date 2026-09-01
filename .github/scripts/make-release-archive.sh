#!/usr/bin/env bash
set -euo pipefail

: "${SOURCE_DATE_EPOCH:?SOURCE_DATE_EPOCH is required}"
script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
python3 "$script_dir/make-release-archive.py" "$@"
