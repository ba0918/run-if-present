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

prefix="## [$version] - "
dated_heading=$(awk -v prefix="$prefix" '
  function leap(year) {
    return year % 400 == 0 || (year % 4 == 0 && year % 100 != 0)
  }
  function valid_date(date, year, month, day, limit) {
    if (length(date) != 10 || substr(date, 5, 1) != "-" || substr(date, 8, 1) != "-") return 0
    year = substr(date, 1, 4)
    month = substr(date, 6, 2)
    day = substr(date, 9, 2)
    if (year !~ /^[0-9][0-9][0-9][0-9]$/ || month !~ /^[0-9][0-9]$/ || day !~ /^[0-9][0-9]$/) return 0
    year += 0
    month += 0
    day += 0
    if (year < 1 || month < 1 || month > 12 || day < 1) return 0
    split("31 28 31 30 31 30 31 31 30 31 30 31", limit, " ")
    if (month == 2 && leap(year)) limit[2] = 29
    return day <= limit[month]
  }
  index($0, prefix) == 1 && valid_date(substr($0, length(prefix) + 1)) { print; exit }
' "$changelog")
if [[ -z "$dated_heading" ]]; then
  echo "release metadata: changelog has no dated $version heading" >&2
  exit 1
fi

if ! awk -v target="$dated_heading" '
  function content(line) {
    if (line ~ /^[[:space:]]*$/) return 0
    if (line ~ /^###[#]*([[:space:]]|$)/) return 0
    if (line ~ /^\[[^]]+\]:[[:space:]]/) return 0
    return 1
  }
  /^## / {
    if ($0 == "## Unreleased") section = "unreleased"
    else if ($0 == target) section = "target"
    else section = "other"
    next
  }
  section == "unreleased" && content($0) { unreleased_content = 1 }
  section == "target" && content($0) { target_content = 1 }
  END { exit unreleased_content || !target_content }
' "$changelog"; then
  echo "release metadata: changelog content was not promoted to $version" >&2
  exit 1
fi

link_prefix="[$version]: https://"
link_suffix="v$version"
if ! awk -v prefix="$link_prefix" -v suffix="$link_suffix" '
  index($0, prefix) == 1 &&
    length($0) > length(prefix) + length(suffix) &&
    substr($0, length($0) - length(suffix) + 1) == suffix { found = 1 }
  END { exit !found }
' "$changelog"; then
  echo "release metadata: changelog has no $version comparison link" >&2
  exit 1
fi

printf '%s\n' "$version"
