#!/usr/bin/env bash
# Check APKs: every native library is 16 KB aligned, the zip is aligned, and report the sizes.
# Usage: scripts/verify-apk.sh <apk>…        (just verify-apk; needs the build tools: run in the image)
#   Per APK: `zipalign -c -P 16 -v 4` (stored .so files on 16 KB boundaries, everything else on 4),
#   and each library's ELF LOAD segments' p_align (>= 0x4000), read with the NDK's llvm-readelf.
#   Optional, for a release (.github/workflows/release.yml):
#     KAI_EXPECT_VERSION_NAME / KAI_EXPECT_VERSION_CODE   the manifest's versionName / versionCode (aapt2)
#     KAI_EXPECT_SIGNED=1                                 apksigner verifies the signature (min SDK 31)
#                                                         and the signer's certificate is printed
#     KAI_EXPECT_CERT_SHA256=<64 hex digits>              the signer's certificate SHA-256 (one only) (with
#                                                         KAI_EXPECT_SIGNED=1; release.yml: the
#                                                         repository variable KAI_SIGNING_CERT_SHA256)
set -euo pipefail
(($# > 0)) || { sed -n '2,6p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2; }
bt=$(ls -d "${ANDROID_HOME:?ANDROID_HOME is not set}"/build-tools/* | sort -V | tail -1)
readelf=$(ls "${ANDROID_NDK_HOME:?ANDROID_NDK_HOME is not set}"/toolchains/llvm/prebuilt/*/bin/llvm-readelf)
status=0
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
for apk in "$@"; do
  if [[ ! -f "$apk" ]]; then
    echo "missing: $apk" >&2
    status=1
    continue
  fi
  echo "== $apk ($(stat -c %s "$apk") bytes)"
  if "$bt/zipalign" -c -P 16 -v 4 "$apk" >"$work/zipalign.log" 2>&1; then
    echo "zipalign -c -P 16: OK ($(grep -c '\.so' "$work/zipalign.log") libraries)"
  else
    grep -E 'BAD|FAIL|\.so' "$work/zipalign.log" | tail -20 >&2 || tail -20 "$work/zipalign.log" >&2
    echo "zipalign -c -P 16: FAILED" >&2
    status=1
  fi
  if [[ -n "${KAI_EXPECT_VERSION_NAME:-}${KAI_EXPECT_VERSION_CODE:-}" ]]; then
    # Captured whole, then cut: `| head -1` closes the pipe after one line and aapt2 dies of SIGPIPE
    # (141 under pipefail) when the badging is longer than the pipe buffer, as a release APK's is.
    badging=$("$bt/aapt2" dump badging "$apk")
    badging=${badging%%$'\n'*}
    got_name=$(sed -nE "s/.* versionName='([^']*)'.*/\1/p" <<<"$badging")
    got_code=$(sed -nE "s/.* versionCode='([^']*)'.*/\1/p" <<<"$badging")
    if [[ "$got_name" == "${KAI_EXPECT_VERSION_NAME:-$got_name}" && "$got_code" == "${KAI_EXPECT_VERSION_CODE:-$got_code}" ]]; then
      echo "version: $got_name ($got_code)"
    else
      echo "version: $got_name ($got_code), expected ${KAI_EXPECT_VERSION_NAME:-any} (${KAI_EXPECT_VERSION_CODE:-any})" >&2
      status=1
    fi
  fi
  if [[ "${KAI_EXPECT_SIGNED:-}" == 1 ]]; then
    if "$bt/apksigner" verify --min-sdk-version 31 --print-certs "$apk" >"$work/apksigner.log" 2>&1; then
      echo "apksigner verify: OK"
      grep -E 'certificate SHA-256 digest' "$work/apksigner.log" || true
      if [[ -n "${KAI_EXPECT_CERT_SHA256:-}" ]]; then
        want=$(tr -d ':[:space:]' <<<"$KAI_EXPECT_CERT_SHA256" | tr 'A-F' 'a-f')
        mapfile -t got < <(sed -nE 's/^(Signer #[0-9]+|V[0-9.]+ Signer): certificate SHA-256 digest: ([0-9A-Fa-f]+)$/\2/p' "$work/apksigner.log" | tr 'A-F' 'a-f' | sort -u)
        if [[ ! "$want" =~ ^[0-9a-f]{64}$ ]]; then
          echo "signer certificate: KAI_EXPECT_CERT_SHA256 is no SHA-256 (64 hex digits)" >&2
          status=1
        elif ((${#got[@]} == 1)) && [[ "${got[0]}" == "$want" ]]; then
          echo "signer certificate: matches the expected SHA-256"
        else
          echo "signer certificate: ${got[*]:-none} (${#got[@]} distinct certificates), expected $want" >&2
          status=1
        fi
      fi
    else
      cat "$work/apksigner.log" >&2
      echo "apksigner verify: FAILED" >&2
      status=1
    fi
  fi
  rm -rf "$work/x" && mkdir "$work/x"
  unzip -q "$apk" 'lib/*' -d "$work/x"
  while IFS= read -r so; do
    # The smallest p_align of the LOAD segments (sed -n 1p reads to the end, unlike head: no SIGPIPE).
    align=$("$readelf" -lW "$so" | awk '$1 == "LOAD" {print strtonum($NF)}' | sort -n | sed -n 1p)
    printf '%-60s LOAD p_align %#x  %s bytes\n' "${so#"$work/x/"}" "$align" "$(stat -c %s "$so")"
    ((align >= 16384)) || { echo "  not 16 KB aligned: $so" >&2; status=1; }
  done < <(find "$work/x" -name '*.so' | sort)
done
exit $status
