#!/usr/bin/env bash
set -euo pipefail

expected=${1:?expected asset directory is required}
existing=${2:?existing asset directory is required}
script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

shopt -s nullglob
assets=("$expected"/*)
if [[ ${#assets[@]} -eq 0 ]]; then
  echo "release assets: expected asset directory is empty" >&2
  exit 1
fi

for asset in "${assets[@]}"; do
  retained="$existing/$(basename "$asset")"
  if [[ -e "$retained" ]]; then
    "$script_dir/verify-same-checksum.sh" "$asset" "$retained" >/dev/null
  else
    printf '%s\n' "$asset"
  fi
done
