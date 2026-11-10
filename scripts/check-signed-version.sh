#!/usr/bin/env bash
# Checks that an updater signature names the version being built (phase 4b final review I3).
# Usage: check-signed-version.sh <installer>.sig <version>
#
# The Tauri CLI writes the installer's .sig as base64 of minisign's signature text, whose
# "trusted comment: " line holds tab-separated key:value fields (timestamp, file, version). The
# app's updater runs with `requireSignedVersion: true` (tauri.conf.json): it refuses a signature
# whose trusted comment has no `version:` field, or names another version than latest.json
# offers, and that refusal is silent by design (one log line, nothing shown). So CI proves the
# field is there and exact before an installer is uploaded: ci.yml runs this on a signed run
# (`inputs.sign`) and in the updater spike, for both of its builds. The trusted comment is public;
# nothing secret is read.
#
# Exit 0: the field `version:<version>` is in the trusted comment. Exit 1: anything else (no
# file, not base64, no trusted comment, no such field).
set -euo pipefail

if [ "$#" -ne 2 ] || [ -z "$2" ]; then
  echo "::error::usage: check-signed-version.sh <installer>.sig <version>"
  exit 1
fi
sig="$1"
version="$2"
if [ ! -f "$sig" ]; then
  echo "::error::$sig is not there"
  exit 1
fi

text="$(tr -d '\r\n' < "$sig" | base64 -d | tr -d '\r')" || {
  echo "::error::$sig is not base64"
  exit 1
}
comment="$(printf '%s\n' "$text" | sed -n 's/^trusted comment: //p')"
if [ -z "$comment" ]; then
  echo "::error::$sig has no trusted comment"
  exit 1
fi

IFS=$'\t' read -r -a fields <<< "$comment"
for field in "${fields[@]}"; do
  if [ "$field" = "version:$version" ]; then
    echo "$sig: the trusted comment names version:$version"
    exit 0
  fi
done
echo "::error::$sig: the trusted comment ($comment) does not name version:$version, so the updater would refuse it"
exit 1
