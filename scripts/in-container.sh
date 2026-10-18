# Run a command in the Android build image under Podman, on a STAGED COPY of the work tree.
# Usage: scripts/in-container.sh <command> [args…]       (just apk, just check, just verify-apk …)
#   Inside the image already (CI, KAI_BUILD_IMAGE=1) the command just runs.
# The image is localhost/kai-mobile-build:<short hash of ci/Containerfile>, built when missing.
#
# NEVER bind-mount the checkout, a worktree or a parent of one with :Z or :z. The relabel is recursive
# and outlives the container: the checkout ends up container_file_t:<random MCS>, which makes other
# confined tools there (e.g. pasta) fail with SELinux denials. So the container gets a scratch copy
# under $KAI_CACHE/stage/ (the tracked and not-ignored files, rsync'd with their mtimes so Gradle's
# up-to-date checks hold; the path is stable per checkout) and only that copy and the caches are
# relabelled. Afterwards the outputs the host uses (APKs, reports, test results under
# android/app/build and the spike's outputs) are copied back.
# lmgw-client and lmgw-api-types are staged from LMGW_PIN's tree (git archive) into the copy's
# .stage/lmgw and mounted again at /src/lmgw, so `../../lmgw` from core/ resolves as on the host.
# Caches live under ~/.cache/kai-mobile/ (Gradle home, cargo registry, the cargo target directory,
# the Android user home with the debug key), mounted with :z so that a second run can share them.
# 6 cargo jobs, Gradle's 6 workers (gradle.properties).
set -euo pipefail
(($# > 0)) || { sed -n '2,3p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2; }
if [[ "${KAI_BUILD_IMAGE:-}" == 1 ]]; then
  exec "$@"
fi
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
tag=$(sha256sum ci/Containerfile | cut -c1-12)
image=localhost/kai-mobile-build:$tag
if ! podman image exists "$image"; then
  echo "building $image (ci/Containerfile)" >&2
  podman build -t "$image" -f ci/Containerfile ci >&2
fi
scripts/lmgw-pin.sh check || true
cache=${KAI_CACHE:-$HOME/.cache/kai-mobile}
mkdir -p "$cache"/{gradle,android,cargo,cargo-target,home}
# The staged copy: one directory per checkout path.
stage=$cache/stage/$(basename "$root")-$(printf %s "$root" | sha256sum | cut -c1-8)
work=$stage/kai-mobile
mkdir -p "$work"
list=$(mktemp)
trap 'rm -f "$list" "$list.old"' EXIT
# Tracked plus untracked-not-ignored files that exist now (a tracked file deleted in the work tree is skipped).
git ls-files -z --cached --others --exclude-standard | while IFS= read -r -d '' f; do
  [[ -e $f || -L $f ]] && printf '%s\0' "$f"
done >"$list"
# Files that left the list since the last run go from the copy; its build output is not on the list and stays.
if [[ -f $stage/files.list ]]; then
  tr '\0' '\n' <"$stage/files.list" | sort >"$list.old"
  tr '\0' '\n' <"$list" | sort | comm -23 "$list.old" - | while IFS= read -r f; do rm -f -- "$work/$f"; done
fi
rsync -a --files-from="$list" --from0 ./ "$work/"
cp "$list" "$stage/files.list"
# Earlier outputs (verify-apk on an APK an earlier run copied back) are there for a fresh copy too.
for d in android/app/build/outputs spikes/wp0/app/build/outputs; do
  if [[ -d $d ]]; then mkdir -p "$work/$d"; rsync -a --update "$d/" "$work/$d/"; fi
done
scripts/lmgw-pin.sh stage "$work/.stage"
tty=()
[[ -t 0 && -t 1 ]] && tty=(-t)
status=0
podman run --rm "${tty[@]}" --userns=keep-id \
  -v "$work:/src/kai-mobile:Z" \
  -v "$work/.stage/lmgw:/src/lmgw:Z" \
  -v "$cache/gradle:/cache/gradle:z" \
  -v "$cache/android:/cache/android:z" \
  -v "$cache/cargo:/cache/cargo:z" \
  -v "$cache/cargo-target:/cache/cargo-target:z" \
  -v "$cache/home:/cache/home:z" \
  -e HOME=/cache/home \
  -e JAVA_TOOL_OPTIONS=-Duser.home=/cache/home \
  -e GRADLE_USER_HOME=/cache/gradle \
  -e ANDROID_USER_HOME=/cache/android \
  -e CARGO_HOME=/cache/cargo \
  -e CARGO_TARGET_DIR=/cache/cargo-target \
  -e CARGO_BUILD_JOBS=6 \
  -e KAI_VERSION_CODE \
  -w /src/kai-mobile \
  "$image" "$@" || status=$?
# Copy the outputs back even when the command failed (the reports say why).
for d in android/app/build/outputs android/app/build/reports android/app/build/test-results \
         spikes/wp0/app/build/outputs; do
  if [[ -d $work/$d ]]; then mkdir -p "$d"; rsync -a --delete "$work/$d/" "$d/"; fi
done
exit "$status"
