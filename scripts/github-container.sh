#!/usr/bin/env bash
# Run a command in the Android build image on GitHub's runners (.github/workflows), where the
# image is built by the job itself and the checkout is mounted as it is.
# Usage: scripts/github-container.sh <command> [args…]       (just check, just apk-release, …)
#   The image is kai-mobile-build:<first 12 hex digits of ci/Containerfile's sha256>, built here
#   when missing (the same file as the private CI's and `just image`). The container runs as the
#   caller's user with the checkout at /src/kai-mobile and lmgw's clone (../lmgw) at /src/lmgw, so
#   `../../lmgw` from core/ resolves as everywhere else; caches and the tool homes live in the
#   checkout's .cache/ (ignored). KAI_BUILD_IMAGE=1 inside, so scripts/in-container.sh and the
#   recipes just run. KAI_CONTAINER_ENGINE picks docker (the default) or podman.
#   Passed through when set: KAI_PUBLIC_RELEASE, KAI_EXPECT_VERSION_NAME, KAI_EXPECT_VERSION_CODE,
#   KAI_EXPECT_SIGNED, KAI_EXPECT_CERT_SHA256. Never the signing secrets and never a keystore: this
#   container runs the build, so third-party code (Gradle plugins, build scripts, proc macros) with
#   it; the release is signed afterwards by scripts/github-sign.sh, in a container of its own.
# No SELinux relabelling is asked for (`--security-opt label=disable`, a no-op on runners).
set -euo pipefail
(($# > 0)) || { sed -n '2,3p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2; }
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
engine=${KAI_CONTAINER_ENGINE:-docker}
cd "$root"
tag=$(sha256sum ci/Containerfile | cut -c1-12)
image=kai-mobile-build:$tag
if ! "$engine" image inspect "$image" >/dev/null 2>&1; then
  echo "building $image (ci/Containerfile)" >&2
  "$engine" build -t "$image" -f ci/Containerfile ci >&2
fi
lmgw=$(realpath -e "${LMGW_DIR:-$root/../lmgw}") || { echo "no lmgw clone at ../lmgw (scripts/lmgw-pin.sh clone-published ../lmgw)" >&2; exit 1; }
home=/src/kai-mobile/.cache
mkdir -p .cache/{gradle,android,cargo,home}
args=(
  --rm --security-opt label=disable
  --user "$(id -u):$(id -g)"
  -v "$root:/src/kai-mobile"
  -v "$lmgw:/src/lmgw"
  -e "HOME=$home/home"
  -e "JAVA_TOOL_OPTIONS=-Duser.home=$home/home"
  -e "GRADLE_USER_HOME=$home/gradle"
  -e "ANDROID_USER_HOME=$home/android"
  -e "CARGO_HOME=$home/cargo"
  -e CARGO_BUILD_JOBS=6
  -w /src/kai-mobile
)
if [[ "$engine" == podman ]]; then args+=(--userns=keep-id); fi
for v in KAI_PUBLIC_RELEASE KAI_EXPECT_VERSION_NAME KAI_EXPECT_VERSION_CODE KAI_EXPECT_SIGNED \
         KAI_EXPECT_CERT_SHA256; do
  if [[ -n "${!v:-}" ]]; then args+=(-e "$v"); fi
done
exec "$engine" run "${args[@]}" "$image" "$@"
