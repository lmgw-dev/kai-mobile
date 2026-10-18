#!/usr/bin/env bash
# Cut a release: set the version, commit it, and make the annotated tag v<X.Y.Z>. Pushes nothing.
#
# Usage: scripts/release.sh <X.Y.Z> [notes-file]
#
# Sets the version in android/gradle.properties (kai.version, the app's versionName) and in
# core/Cargo.toml's [workspace.package] (the core's and the bindgen's version, which the About page
# shows) with core/Cargo.lock, commits "release X.Y.Z" on main and tags that commit. The tag's
# message becomes the GitHub release notes, Markdown with wrapped lines joined
# (.github/workflows/release.yml): the notes file when given, else git opens your editor. When the
# tree already carries the version (the first release, 0.1.0), it only tags. Then publish:
#   git push origin main vX.Y.Z    the private CI builds X.Y.Z+<pipeline> of main
#   scripts/publish-github.sh --dry-run --ref vX.Y.Z -F <message> --tag vX.Y.Z
#                                  GitHub: one squashed commit with the tag's tree, and the tag
#                                  on it (.github/workflows/release.yml builds the release)
# It refuses unless LMGW_PIN's lmgw_tag is a published lmgw release with the pinned tree
# (scripts/lmgw-pin.sh verify-published): GitHub's build clones it. Between lmgw releases the pin
# says `unpublished`, which the private CI takes, a release never (README.md, "Maintainers").
# The cargo lock is updated offline, with lmgw's checkout next to this one (../lmgw), as the core's
# other cargo commands need it.
set -euo pipefail
cd "$(dirname "$0")/.."

version="${1:?usage: scripts/release.sh <X.Y.Z> [notes-file]}"
notes="${2:-}"
die() { echo "$*" >&2; exit 1; }

num='(0|[1-9][0-9]*)'
[[ "$version" =~ ^$num\.$num\.$num$ ]] || die "not an X.Y.Z version (no leading zeros): $version"
[[ "$(git branch --show-current)" == main ]] || die "releases are cut from main"
[[ -z "$(git status --porcelain --untracked-files=no)" ]] || die "commit or stash your changes first"
if git rev-parse -q --verify "refs/tags/v$version" >/dev/null; then die "tag v$version already exists"; fi
scripts/lmgw-pin.sh verify-published \
    || die "a release needs lmgw's published release in LMGW_PIN first (README.md, \"Maintainers\")"
[[ -z "$notes" || -f "$notes" ]] || die "no notes file $notes"

props=android/gradle.properties
manifest=core/Cargo.toml
current="$(sed -nE 's/^kai\.version=([0-9]+\.[0-9]+\.[0-9]+)$/\1/p' "$props")"
[[ -n "$current" ]] || die "$props has no kai.version=X.Y.Z"
# [workspace.package]'s version: the first `version = ` line of the core's workspace manifest.
core="$(grep -m1 -oE '^version = "[0-9]+\.[0-9]+\.[0-9]+"' "$manifest" | grep -oE '[0-9]+\.[0-9]+\.[0-9]+')"
[[ "$core" == "$current" ]] || die "$manifest says $core, $props says $current: make them agree first"
# The release workflow derives versionCode from the version (android/app/build.gradle.kts), whose
# fields hold up to 999.
IFS=. read -r major minor patch <<<"$version"
((major <= 999 && minor <= 999 && patch <= 999)) || die "$version has a field above 999: versionCode (android/app/build.gradle.kts) cannot hold it"
if [[ "$current" != "$version" ]]; then
    [[ "$(printf '%s\n' "$current" "$version" | sort -V | tail -n1)" == "$version" ]] \
        || die "$version is older than the current $current"
    cur="${current//./\\.}"
    sed -i -E "s/^kai\.version=$cur\$/kai.version=$version/" "$props"
    sed -i -E "0,/^version = \"$cur\"/s//version = \"$version\"/" "$manifest"
    grep -qx "kai.version=$version" "$props" || die "$props was not set"
    grep -qxF "version = \"$version\"" "$manifest" || die "$manifest was not set"
    (cd core && cargo update --workspace --offline --quiet)
    git commit -q -m "release $version" -- "$props" "$manifest" core/Cargo.lock
fi

if [[ -n "$notes" ]]; then
    git tag -a "v$version" -F "$notes"
else
    git tag -a "v$version"
fi

echo "tagged v$version at $(git rev-parse --short HEAD). Publish with:"
echo "  git push origin main v$version"
echo "  scripts/publish-github.sh --dry-run --ref v$version -F <message> --tag v$version"
echo "  (audit, --approve, then the same without --dry-run; see the script's header)"
