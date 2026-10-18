#!/usr/bin/env bash
# Sign an unsigned release APK in a container of its own, on GitHub's runners (release.yml).
# Usage: scripts/github-sign.sh <unsigned.apk> <signed.apk>
#   Environment: KAI_KEYSTORE_FILE (host path of the PKCS12 keystore), KAI_KEYSTORE_PASSWORD,
#   KAI_KEY_ALIAS, and KAI_KEY_PASSWORD when the key's password differs from the store's.
#   The signing secrets exist only here: the build (scripts/github-container.sh) runs third-party
#   code and never sees them. This container has --network none, all capabilities dropped and a
#   read-only root, and mounts only the keystore (read-only), the unsigned APK (read-only) and the
#   directory of the signed APK; not the checkout. The image's own apksigner signs (v2 and v3,
#   minSdk 31); the unsigned APK is zipaligned by AGP already, which verify-apk checks on the
#   result. The password goes in by name (`env:`), never on a command line. The image must exist
#   (scripts/github-container.sh builds it); KAI_CONTAINER_ENGINE picks docker (default) or podman.
set -euo pipefail
(($# == 2)) || { sed -n '2,3p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2; }
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
engine=${KAI_CONTAINER_ENGINE:-docker}
for v in KAI_KEYSTORE_FILE KAI_KEYSTORE_PASSWORD KAI_KEY_ALIAS; do
  [[ -n "${!v:-}" ]] || { echo "$v is not set" >&2; exit 1; }
done
unsigned=$(realpath -e "$1") || { echo "no such APK: $1" >&2; exit 1; }
keystore=$(realpath -e "$KAI_KEYSTORE_FILE")
mkdir -p "$(dirname "$2")"
outdir=$(realpath -e "$(dirname "$2")")
out=$(basename "$2")
rm -f "$outdir/$out"
image=kai-mobile-build:$(sha256sum "$root/ci/Containerfile" | cut -c1-12)
"$engine" image inspect "$image" >/dev/null 2>&1 || { echo "no image $image (scripts/github-container.sh builds it)" >&2; exit 1; }
sign=(sign --v4-signing-enabled false --ks /run/kai-keystore --ks-pass env:KAI_KEYSTORE_PASSWORD --ks-key-alias "$KAI_KEY_ALIAS")
if [[ -n "${KAI_KEY_PASSWORD:-}" ]]; then sign+=(--key-pass env:KAI_KEY_PASSWORD); fi
sign+=(--out "/out/$out" /in/unsigned.apk)
args=(
  --rm --security-opt label=disable --network none --cap-drop ALL --read-only --tmpfs /tmp
  --user "$(id -u):$(id -g)"
  -v "$keystore:/run/kai-keystore:ro"
  -v "$unsigned:/in/unsigned.apk:ro"
  -v "$outdir:/out"
  -e HOME=/tmp -e JAVA_TOOL_OPTIONS=-Duser.home=/tmp
  -e KAI_KEYSTORE_PASSWORD
)
if [[ -n "${KAI_KEY_PASSWORD:-}" ]]; then args+=(-e KAI_KEY_PASSWORD); fi
if [[ "$engine" == podman ]]; then args+=(--userns=keep-id); fi
"$engine" run "${args[@]}" "$image" sh -c \
  'bt=$(ls -d "$ANDROID_HOME"/build-tools/* | sort -V | tail -1) && exec "$bt/apksigner" "$@"' sh "${sign[@]}"
[[ -s "$outdir/$out" ]] || { echo "apksigner wrote no $out" >&2; exit 1; }
