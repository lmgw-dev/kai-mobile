#!/usr/bin/env bash
# The lmgw source Kai mobile is built and tested against (LMGW_PIN; Kai desktop's script, cut down).
# Usage: scripts/lmgw-pin.sh [--at REF] <command>     LMGW_PIN as committed at REF, else as in the
#                                                     working tree
#   check [lmgw checkout]   warn, never fail, when the checkout (default ../lmgw, or $LMGW_DIR) is
#                           neither at the pinned commit nor at the pinned tree, has changes, or
#                           when the pinned commit's tree is not the pinned tree
#   verify-tree <checkout>  fail unless the checkout's HEAD has the pinned tree and no changes (CI,
#                           on its clone of lmgw at the pinned commit)
#   verify-published        fail unless lmgw_tag is a release of the public lmgw (LMGW_PUBLIC_URL,
#                           default https://github.com/lmgw-dev/lmgw) with the pinned tree; fetches
#                           the tag alone into target/lmgw-public.git (scripts/release.sh and
#                           scripts/publish-github.sh, before Kai mobile goes to GitHub)
#   clone-published <dest>  GitHub's CI: clone the public lmgw at lmgw_tag into <dest> (the
#                           checkout's `../lmgw`) and run verify-tree on it; fails with a clear
#                           message while lmgw_tag is `unpublished`
#   stage <dest>            extract lmgw-client and lmgw-api-types at the pinned TREE (`git archive`,
#                           never the checkout's working tree) into <dest>/lmgw, with a minimal
#                           workspace root carrying the pin's [workspace.package]: the layout
#                           `../../lmgw` from core/ resolves in the container build
#   bump [lmgw checkout]    write the checkout's HEAD commit and tree into LMGW_PIN (the tag stays);
#                           refuses unless HEAD is on its origin/main, which CI clones from, and the
#                           checkout is clean
#   commit | tag | tree     print the pinned commit, tag or tree (`tag` may say `unpublished`)
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
at=""
if [[ "${1:-}" == --at ]]; then
  at="${2:?--at needs a commit}"
  shift 2
  text=$(git -C "$root" show "$at:LMGW_PIN" 2>/dev/null) || { echo "$at has no LMGW_PIN" >&2; exit 1; }
else
  text=$(<"$root/LMGW_PIN")
fi
field() { awk -v k="$1" '$1 == k {print $2}' <<<"$text"; }
pin=$(field lmgw_commit)
tag=$(field lmgw_tag)
tree=$(field lmgw_tree)
where="LMGW_PIN${at:+ at $at}"
[[ "$pin" =~ ^[0-9a-f]{40}$ ]] || { echo "$where holds no 40-hex lmgw_commit" >&2; exit 1; }
[[ "$tree" =~ ^[0-9a-f]{40}$ ]] || { echo "$where holds no 40-hex lmgw_tree" >&2; exit 1; }
[[ "$tag" == unpublished || "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] \
  || { echo "$where holds no lmgw_tag vX.Y.Z or unpublished" >&2; exit 1; }
crates=(lmgw-client lmgw-api-types)

case "${1:-check}" in
  commit) echo "$pin" ;;
  tag) echo "$tag" ;;
  tree) echo "$tree" ;;
  verify-tree)
    lmgw=$(realpath -m "${2:?usage: $0 verify-tree <lmgw checkout>}")
    head=$(git -C "$lmgw" rev-parse --verify -q 'HEAD^{tree}' 2>/dev/null) \
      || { echo "no lmgw checkout at $lmgw" >&2; exit 1; }
    [[ "$head" == "$tree" ]] \
      || { echo "lmgw at $lmgw has the tree $head, not the pinned $tree: LMGW_PIN's lines disagree or the clone is not at the pin" >&2; exit 1; }
    changed=$(git -C "$lmgw" status --porcelain --untracked-files=no | wc -l)
    ((changed == 0)) || { echo "lmgw at $lmgw has $changed changed files" >&2; exit 1; }
    echo "lmgw at $lmgw has the pinned tree ${tree:0:12}"
    ;;
  verify-published)
    fail() { echo "$where: $*" >&2; exit 1; }
    [[ "$tag" != unpublished ]] \
      || fail "lmgw_tag is unpublished: lmgw ${pin:0:7} (tree ${tree:0:12}) is in no published lmgw" \
        "release yet, and GitHub's CI and release build clone one. Publish lmgw first, then name its" \
        "release in LMGW_PIN"
    url=${LMGW_PUBLIC_URL:-https://github.com/lmgw-dev/lmgw}
    # --git-dir, never -C: a cache directory that is no repository must not send git up into this one.
    cache=$root/target/lmgw-public.git
    mkdir -p "$root/target"
    git init -q --bare "$cache"
    git --git-dir="$cache" fetch -q --depth 1 --no-tags "$url" "+refs/tags/$tag:refs/tags/$tag" \
      || fail "cannot fetch lmgw_tag $tag from $url: is it published?"
    public=$(git --git-dir="$cache" rev-parse "refs/tags/$tag^{tree}")
    [[ "$public" == "$tree" ]] \
      || fail "$url's $tag has the tree ${public:0:12}, not lmgw_tree ${tree:0:12}: the tag is" \
        "another release than the pinned source"
    echo "$url's $tag has the pinned tree ${tree:0:12}"
    ;;
  clone-published)
    dest=$(realpath -m "${2:?usage: $0 clone-published <dest>}")
    [[ "$tag" != unpublished ]] || {
      echo "$where: lmgw_tag is unpublished: no published lmgw release has the pinned tree ${tree:0:12}" \
        "(lmgw ${pin:0:7}), so GitHub's build has no lmgw to clone. Publish lmgw, then name its release" \
        "as lmgw_tag in LMGW_PIN." >&2
      exit 1
    }
    url=${LMGW_PUBLIC_URL:-https://github.com/lmgw-dev/lmgw}
    git clone --quiet --depth 1 --branch "$tag" "$url" "$dest" \
      || { echo "$where: cannot clone lmgw $tag from $url" >&2; exit 1; }
    exec "${BASH_SOURCE[0]}" ${at:+--at "$at"} verify-tree "$dest"
    ;;
  stage)
    dest=${2:?usage: $0 stage <dest>}
    lmgw=$(realpath -m "${LMGW_DIR:-$root/../lmgw}")
    [[ "$(git -C "$lmgw" cat-file -t "$tree" 2>/dev/null)" == tree ]] || {
      echo "the pinned lmgw tree $tree (LMGW_PIN) is not in $lmgw: fetch it there, or set LMGW_DIR" >&2
      exit 1
    }
    # The marker keeps an unchanged stage untouched, so cargo's fingerprints of the staged path
    # dependencies stay valid between runs.
    if [[ "$(cat "$dest/lmgw/.tree" 2>/dev/null || true)" == "$tree" ]]; then
      exit 0
    fi
    rm -rf "$dest/lmgw"
    mkdir -p "$dest/lmgw"
    paths=()
    for d in "${crates[@]}"; do
      git -C "$lmgw" cat-file -e "$tree:crates/$d/Cargo.toml" 2>/dev/null \
        || { echo "lmgw's tree ${tree:0:12} has no crates/$d" >&2; exit 1; }
      paths+=("crates/$d")
    done
    git -C "$lmgw" archive --format=tar "$tree" -- "${paths[@]}" | tar -xf - -C "$dest/lmgw"
    {
      printf '[workspace]\nresolver = "2"\nmembers = ["crates/lmgw-api-types", "crates/lmgw-client"]\n\n'
      git -C "$lmgw" show "$tree:Cargo.toml" | awk '/^\[workspace\.package\]/{p=1;print;next} /^\[/{p=0} p&&NF'
    } >"$dest/lmgw/Cargo.toml"
    echo "$tree" >"$dest/lmgw/.tree"
    echo "staged ${crates[*]} from lmgw tree ${tree:0:12} (lmgw ${pin:0:7}, $tag) into $dest/lmgw" >&2
    ;;
  bump)
    lmgw=$(realpath -m "${2:-${LMGW_DIR:-$root/../lmgw}}")
    new=$(git -C "$lmgw" rev-parse --verify -q HEAD 2>/dev/null) \
      || { echo "no lmgw checkout at $lmgw" >&2; exit 1; }
    git -C "$lmgw" merge-base --is-ancestor HEAD origin/main 2>/dev/null || {
      echo "lmgw's HEAD ${new:0:7} is not on its origin/main: CI clones the pin from GitLab, so push it first (or git fetch, if origin/main is stale)" >&2
      exit 1
    }
    changed=$(git -C "$lmgw" status --porcelain --untracked-files=no | wc -l)
    ((changed == 0)) || { echo "lmgw at $lmgw has $changed changed files: the pin names a commit, not a working tree" >&2; exit 1; }
    newtree=$(git -C "$lmgw" rev-parse 'HEAD^{tree}')
    sed -i -E -e "s/^lmgw_commit .*/lmgw_commit $new/" -e "s/^lmgw_tree .*/lmgw_tree $newtree/" "$root/LMGW_PIN"
    echo "LMGW_PIN now names lmgw ${new:0:7} (tree ${newtree:0:7}); the tag stays $tag" >&2
    ;;
  check)
    lmgw=$(realpath -m "${2:-${LMGW_DIR:-$root/../lmgw}}")
    warn() { echo "warning (LMGW_PIN): $*" >&2; }
    if ! head=$(git -C "$lmgw" rev-parse HEAD 2>/dev/null); then
      warn "no lmgw checkout at $lmgw; Kai mobile is pinned to lmgw ${pin:0:7} ($tag, tree ${tree:0:7})"
      exit 0
    fi
    if git -C "$lmgw" cat-file -e "$pin^{commit}" 2>/dev/null; then
      pinned_tree=$(git -C "$lmgw" rev-parse "$pin^{tree}")
      if [[ "$pinned_tree" != "$tree" ]]; then
        warn "lmgw_commit ${pin:0:7}'s tree is ${pinned_tree:0:7}, not lmgw_tree ${tree:0:7}: the" \
          "lines of LMGW_PIN must name the same source"
      fi
    fi
    if [[ "$head" != "$pin" && "$(git -C "$lmgw" rev-parse 'HEAD^{tree}')" != "$tree" ]]; then
      branch=$(git -C "$lmgw" branch --show-current 2>/dev/null || true)
      if ! git -C "$lmgw" cat-file -e "$pin^{commit}" 2>/dev/null; then
        same="the pin is not in that checkout (git -C $lmgw fetch)"
      elif git -C "$lmgw" diff --quiet "$pin" HEAD -- "${crates[@]/#/crates/}"; then
        same="lmgw-client and lmgw-api-types are as at the pin; other parts of lmgw differ"
      else
        same="lmgw-client or lmgw-api-types differ from the pin"
      fi
      warn "lmgw at $lmgw is at ${head:0:7}${branch:+ ($branch)}, not the pinned ${pin:0:7}: the host" \
        "tests build against that checkout ($same); the container build uses the pinned tree. Check out" \
        "the pin, or run 'just pin-bump' to move the pin (and core/Cargo.lock) to that checkout."
    fi
    changed=$(git -C "$lmgw" status --porcelain --untracked-files=no | wc -l)
    if ((changed > 0)); then
      warn "lmgw at $lmgw has $changed changed files: the host tests build its working tree, not a commit."
    fi
    ;;
  *)
    echo "usage: $0 [--at REF] check [lmgw checkout] | verify-tree <lmgw checkout> | verify-published |" \
      "clone-published <dest> | bump [lmgw checkout] | stage <dest> | commit | tag | tree" >&2
    exit 2
    ;;
esac
