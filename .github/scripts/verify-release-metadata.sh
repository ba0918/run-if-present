#!/usr/bin/env bash
set -euo pipefail

tag=${1:?tag is required}
manifest=${2:?Cargo.toml path is required}
changelog=${3:?CHANGELOG.md path is required}

case "$tag" in
  v[0-9]*.[0-9]*.[0-9]*) version=${tag#v} ;;
  *) echo "release metadata: tag must be v<version>" >&2; exit 1 ;;
esac

manifest_version=$(awk '
  /^\[package\]$/ { package = 1; next }
  /^\[/ { package = 0 }
  package && /^version = "/ { gsub(/^version = "|"$/, ""); print; exit }
' "$manifest")

if [[ "$manifest_version" != "$version" ]]; then
  echo "release metadata: tag and Cargo version disagree" >&2
  exit 1
fi

dated_heading=false
while IFS= read -r line; do
  prefix="## [$version] - "
  if [[ "$line" == "$prefix"* ]] && [[ "${line#"$prefix"}" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]; then
    dated_heading=true
    break
  fi
done < "$changelog"

if [[ "$dated_heading" != true ]]; then
  echo "release metadata: changelog has no dated $version heading" >&2
  exit 1
fi

if ! grep -Eq "^\[$version\]: https://.+v$version$" "$changelog"; then
  echo "release metadata: changelog has no $version comparison link" >&2
  exit 1
fi

printf '%s\n' "$version"
