# Kai mobile: build, test, install.  `just --list` shows the recipes.
# The Android side builds in the CI image under Podman (scripts/in-container.sh); the core's host tests
# use the host toolchain. Inside the image (CI) the same recipes run in place.

set shell := ["bash", "-euo", "pipefail", "-c"]

# Bounds the parallel builds' memory (the machine is shared).
export CARGO_BUILD_JOBS := "6"

apk_debug := "android/app/build/outputs/apk/debug/app-debug.apk"
apk_release := "android/app/build/outputs/apk/release"
spike_apk := "spikes/wp0/app/build/outputs/apk/release/spike-release.apk"

default:
    @just --list

# What CI's check job runs (.gitlab-ci.yml, .github/workflows/ci.yml): the pin warning, the image tag,
# the publish scripts' tests, the core's gates on the host, and Gradle's lint and JVM unit tests in
# the image.
[doc("Pin warning, image tag, publish script tests, core fmt/clippy/tests/doc tests/aws-lc check, Gradle lint and testDebugUnitTest")]
check: pin-check image-tag-check publish-check core-check
    scripts/in-container.sh scripts/gradle.sh lint testDebugUnitTest

[doc("Print the image tag: the first 12 hex digits of ci/Containerfile's sha256")]
image-tag:
    @sha256sum ci/Containerfile | cut -c1-12

# CI's jobs name the image by a tag written in .gitlab-ci.yml (KAI_IMAGE_TAG); it must be the hash.
[doc("Fail when KAI_IMAGE_TAG in .gitlab-ci.yml is not the hash of ci/Containerfile")]
image-tag-check:
    #!/usr/bin/env bash
    set -euo pipefail
    want=$(sha256sum ci/Containerfile | cut -c1-12)
    have=$(sed -nE 's/^ *KAI_IMAGE_TAG: "([0-9a-f]+)".*/\1/p' .gitlab-ci.yml)
    [[ "$want" == "$have" ]] || { echo "KAI_IMAGE_TAG in .gitlab-ci.yml is '$have', ci/Containerfile hashes to $want" >&2; exit 1; }

# scripts/publish-github.sh, its hook, its scan and scripts/release.sh, against local bare repositories as the
# fake GitHub and the fake public lmgw; no network, nothing under ~/.config is read.
[doc("Test the publish scripts (publish-github.sh, pre-push-github.sh, publish_scan.py, release.sh, lmgw-pin.sh)")]
publish-check:
    python3 scripts/publish_github_test.py

# lmgw's checkout next to this repository away from LMGW_PIN is warned about, never failed.
[doc("Warn when lmgw's checkout is not at LMGW_PIN")]
pin-check:
    scripts/lmgw-pin.sh check

# Moves LMGW_PIN to ../lmgw's pushed HEAD, updates only lmgw's two crates in the core's lock, then checks.
[doc("Bump LMGW_PIN to ../lmgw's HEAD (must be on origin/main), update the lock minimally, run check")]
pin-bump:
    scripts/lmgw-pin.sh bump
    cd core && cargo update -p lmgw-client -p lmgw-api-types
    just check

[doc("The core's gates on the host: fmt, clippy, nextest, doc tests, no aws-lc")]
core-check: aws-lc-check
    cd core && cargo fmt --check
    cd core && cargo clippy --locked --all-targets -- -D warnings
    cd core && cargo nextest run --locked
    cd core && cargo test --locked --doc

[doc("The core's tests on the host (nextest and doc tests; nextest skips doc tests)")]
core-test:
    cd core && cargo nextest run --locked
    cd core && cargo test --locked --doc

# M7: rustls runs on ring; aws-lc-sys would need a C build under the NDK.
[doc("Fail when aws-lc-rs or aws-lc-sys is in the core's dependency tree for an Android target")]
aws-lc-check:
    #!/usr/bin/env bash
    set -euo pipefail
    cd core
    for target in aarch64-linux-android x86_64-linux-android x86_64-unknown-linux-gnu; do
        # Captured first: under pipefail a failing `cargo tree` must not read as "no aws-lc".
        tree=$(cargo tree --locked -e normal,build --target "$target")
        if grep -E 'aws-lc' <<<"$tree"; then
            echo "aws-lc is in the dependency tree for $target" >&2
            exit 1
        fi
    done
    echo "no aws-lc in the dependency tree (aarch64-linux-android, x86_64-linux-android, host)"

# Gradle's preBuild runs this per variant (android/app/build.gradle.kts) with the variant's output
# directories; by hand it builds the debug profile into the defaults. A debug build has both ABIs,
# a release build arm64-v8a only (design §2.2). Needs cargo-ndk and the NDK: run it in the image.
[doc("Build the core for Android with cargo-ndk and generate the Kotlin bindings (profile: debug|release)")]
core-android profile="debug" jni="android/app/build/core/jniLibs" bindings="android/app/build/core/kotlin" abis=(if profile == "release" { "arm64-v8a" } else { "arm64-v8a x86_64" }):
    #!/usr/bin/env bash
    set -euo pipefail
    root=$(pwd)
    case "{{profile}}" in
        debug) flag=(); dir=debug ;;
        release) flag=(--release); dir=release ;;
        *) echo "profile is debug or release" >&2; exit 2 ;;
    esac
    jni=$(realpath -m "{{jni}}")
    bindings=$(realpath -m "{{bindings}}")
    targets=(); for abi in {{abis}}; do targets+=(-t "$abi"); done
    cd core
    rm -rf "$jni" "$bindings"
    mkdir -p "$jni" "$bindings"
    cargo ndk "${targets[@]}" -o "$jni" build --locked "${flag[@]}" -p kai-mobile-core
    # The bindings come from the first ABI's library (the interface is the same in all).
    first=${targets[1]}
    case "$first" in arm64-v8a) triple=aarch64-linux-android ;; x86_64) triple=x86_64-linux-android ;; esac
    target_dir=$(cargo metadata --locked --format-version 1 --no-deps | python3 -I -c 'import sys,json;print(json.load(sys.stdin)["target_directory"])')
    cargo run --locked -q -p uniffi-bindgen -- generate --library "$target_dir/$triple/$dir/libkai_mobile_core.so" \
        --language kotlin --out-dir "$bindings" --config kai-mobile-core/uniffi.toml --no-format
    echo "core ({{profile}}: {{abis}}) -> $jni, bindings -> $bindings"

[doc("Build the CI image (localhost/kai-mobile-build:<hash of ci/Containerfile>)")]
image:
    scripts/in-container.sh true

[doc("Gradle in the image: just gradle assembleDebug")]
gradle *args:
    scripts/in-container.sh scripts/gradle.sh {{args}}

# A warm Gradle home skips metadata a fresh one resolves (.module files, parent POMs), so CI failed on
# what the file lacked. The temp home lives under target/ and is removed afterwards.
[doc("Regenerate android/gradle/verification-metadata.xml with an empty Gradle home, over every task CI runs")]
gradle-verification:
    #!/usr/bin/env bash
    set -euo pipefail
    cache=$(pwd)/target/gradle-verification
    trap 'rm -rf "$cache"' EXIT
    rm -rf "$cache"; mkdir -p "$cache/gradle"
    for d in android cargo cargo-target home; do mkdir -p "$HOME/.cache/kai-mobile/$d"; ln -sfn "$HOME/.cache/kai-mobile/$d" "$cache/$d"; done
    # -Pkai.spike=true: WP0's spike module (spikes/wp0/app) is configured too, so its dependencies
    # (libwebrtc) are in the file; the app's own builds never configure it.
    KAI_CACHE="$cache" scripts/in-container.sh scripts/gradle.sh --max-workers=6 --write-verification-metadata sha256 -Pkai.spike=true lint testDebugUnitTest testReleaseUnitTest assembleRelease assembleDebug help

# Debug: applicationId dev.rootshell.kai.dev, both ABIs, the SDK's debug key.
[doc("Build the debug APK (both ABIs) in the image")]
apk:
    scripts/in-container.sh scripts/gradle.sh assembleDebug
    @echo "{{apk_debug}}"

# Signed when the CI variables of §15.1 are set (see README), else app-release-unsigned.apk.
[doc("Build the release APK (arm64-v8a) in the image; unsigned without the signing variables")]
apk-release:
    scripts/in-container.sh scripts/gradle.sh assembleRelease
    @ls -la {{apk_release}}/*.apk

[doc("Check an APK: 16 KB alignment of every library, zip alignment, sizes (default: both built APKs)")]
verify-apk *apks:
    scripts/in-container.sh scripts/verify-apk.sh {{ if apks == "" { apk_debug + " " + apk_release + "/app-release-unsigned.apk" } else { apks } }}

# The first usable version goes onto the test phone this way (design §15.4): wireless debugging,
# paired once with `adb pair`. scripts/adb-serial.sh picks the one serial (ANDROID_SERIAL, else the USB
# entry of the phone, else its wireless entry; two distinct phones fail).
[doc("Install an APK with adb, the debug APK by default (just install apk=path/to/signed.apk); wireless debugging, the phone paired")]
install apk=apk_debug: (build-if-default apk)
    # --user current: a plain install goes into every user, work profiles included, where Kai must not go.
    adb -s "$(scripts/adb-serial.sh)" install --user current -r {{apk}}

# ---- WP0's device spike (design §17; spikes/wp0/RESULTS.md): a separate app, dev.rootshell.kai.spike.

# Gradle's preBuild of the spike module runs this with its output directories. The library carries
# the core and the spike-only FFI (spikes/wp0/ffi): the release profile, so the timings are a
# release build's, and arm64-v8a only. Needs cargo-ndk and the NDK: run it in the image.
[doc("Build the WP0 spike's library and its Kotlin bindings (the spike module's preBuild runs it)")]
spike-core jni="spikes/wp0/app/build/core/release/jniLibs" bindings="spikes/wp0/app/build/core/release/kotlin":
    #!/usr/bin/env bash
    set -euo pipefail
    jni=$(realpath -m "{{jni}}")
    bindings=$(realpath -m "{{bindings}}")
    cd core
    rm -rf "$jni" "$bindings"
    mkdir -p "$jni" "$bindings"
    cargo ndk -t arm64-v8a -o "$jni" build --locked --release -p kai-wp0-spike
    # kai-mobile-core is a cdylib as well, so cargo-ndk copies its library too; the spike's carries it.
    rm -f "$jni"/*/libkai_mobile_core.so
    target_dir=$(cargo metadata --locked --format-version 1 --no-deps | python3 -I -c 'import sys,json;print(json.load(sys.stdin)["target_directory"])')
    # Library mode over both crates; the global configuration points the core's bindings at this
    # library and gives the spike's exports their own package.
    cargo run --locked -q -p uniffi-bindgen -- generate --library "$target_dir/aarch64-linux-android/release/libkai_wp0_spike.so" \
        --language kotlin --out-dir "$bindings" --config ../spikes/wp0/ffi/uniffi-global.toml --no-format
    echo "spike library -> $jni, bindings -> $bindings"

[doc("Build the WP0 spike APK (release, not debuggable, the SDK's debug key, arm64-v8a) in the image")]
spike-apk:
    scripts/in-container.sh scripts/gradle.sh -Pkai.spike=true :spike:assembleRelease
    @ls -la {{spike_apk}}

# Host adb, as `just install`: the current user only, never into work profiles.
[doc("Install the WP0 spike APK on the phone with host adb, for the current user (just spike-apk builds it)")]
spike-install apk=spike_apk:
    @test -f {{apk}} || { echo "{{apk}} is missing: just spike-apk" >&2; exit 1; }
    adb -s "$(scripts/adb-serial.sh)" install --user current -r {{apk}}

# The spike writes its log (wp0.jsonl) and recordings under its external files directory, which
# host adb reads without the app being debuggable.
[doc("Pull the WP0 spike's log and recordings into spikes/wp0/results/<time>/, then fill RESULTS.md's measured blocks")]
spike-log:
    #!/usr/bin/env bash
    set -euo pipefail
    serial=$(scripts/adb-serial.sh)
    user=$(adb -s "$serial" shell am get-current-user | tr -d '\r')
    dest=spikes/wp0/results/$(date +%Y%m%d-%H%M%S)
    mkdir -p "$dest"
    adb -s "$serial" pull "/storage/emulated/$user/Android/data/dev.rootshell.kai.spike/files/wp0/." "$dest/"
    echo "pulled into $dest"
    python3 -I spikes/wp0/fill-results.py --results spikes/wp0/results --doc spikes/wp0/RESULTS.md

# Builds the debug APK only when `just install` was given no APK of its own.
[private]
build-if-default apk:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "{{apk}}" == "{{apk_debug}}" ]]; then just apk; fi
