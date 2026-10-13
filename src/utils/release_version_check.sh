#!/bin/bash
# Picks the newest playwright-rs release whose bundled Playwright version has a
# matching patchright-core on npm, and decides whether patchright-rs needs a
# release for it. patchright-rs reuses the playwright-rs version number and is
# released as a `v<version>` tag of this repository.

set -euo pipefail

RELEASE_REPO="${RELEASE_REPO:-https://github.com/${GITHUB_REPOSITORY:-InfernalAzazel/patchright-rs}}"
CRATES_API="${CRATES_API:-https://crates.io/api/v1/crates}"
USER_AGENT="patchright-rs release check (https://github.com/InfernalAzazel/patchright-rs)"
UPSTREAM_RAW="${UPSTREAM_RAW:-https://raw.githubusercontent.com/padamson/playwright-rust}"
NPM_PACKAGE="${NPM_PACKAGE:-patchright-core}"
CANDIDATES="${CANDIDATES:-5}"

set_output() {
  if [ -n "${GITHUB_OUTPUT:-}" ]; then
    echo "$1=$2" >> "$GITHUB_OUTPUT"
  else
    echo "$1=$2"
  fi
}

crates_io() {
  curl -fsSL -H "User-Agent: $USER_AGENT" "$CRATES_API/$1"
}

playwright_rs_versions=$(crates_io playwright-rs | jq -r --argjson n "$CANDIDATES" '
  [.versions[] | select(.yanked | not) | .num
    | select(test("^[0-9]+\\.[0-9]+\\.[0-9]+$"))]
  | sort_by(split(".") | map(tonumber)) | reverse | .[:$n][]
')
patchright_core_versions=$(npm view "$NPM_PACKAGE" versions --json)

for version in $playwright_rs_versions; do
  tag="v$version"
  playwright_version=$(curl -fsSL "$UPSTREAM_RAW/$tag/crates/playwright/build.rs" \
    | sed -n 's/^const PLAYWRIGHT_VERSION: &str = "\([0-9.]*\)";/\1/p')
  if [ -z "$playwright_version" ]; then
    echo "playwright-rs $tag: no PLAYWRIGHT_VERSION in build.rs, skipping."
    continue
  fi

  IFS='.' read -r major minor patch <<< "$playwright_version"
  patchright_core_version=$(jq -r --arg line "$major.$minor" --argjson min "$patch" '
    [.[] | select(test("^[0-9]+\\.[0-9]+\\.[0-9]+$"))
      | select(startswith($line + "."))
      | select((split(".")[2] | tonumber) >= $min)]
    | sort_by(split(".") | map(tonumber)) | last // empty
  ' <<< "$patchright_core_versions")

  if [ -z "$patchright_core_version" ]; then
    echo "playwright-rs $tag bundles Playwright $playwright_version; no $NPM_PACKAGE $major.$minor.x >= it yet."
    continue
  fi

  echo "playwright-rs $tag bundles Playwright $playwright_version; using $NPM_PACKAGE $patchright_core_version."
  if [ -n "$(git ls-remote --tags "$RELEASE_REPO" "refs/tags/$tag")" ]; then
    echo "patchright-rs $tag is already released."
    set_output proceed false
    exit 0
  fi

  set_output proceed true
  set_output playwright_rs_version "$tag"
  set_output patchright_core_version "$patchright_core_version"
  set_output patchright_version "$version"
  exit 0
done

echo "No playwright-rs release among the newest $CANDIDATES has a matching $NPM_PACKAGE."
set_output proceed false
