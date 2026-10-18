# Kai mobile

<p>
  <a href="https://github.com/lmgw-dev/kai-mobile/releases/latest"><img src="https://img.shields.io/github/v/release/lmgw-dev/kai-mobile" alt="Latest release"></a>
  <a href="https://github.com/lmgw-dev/kai-mobile/actions/workflows/ci.yml"><img src="https://github.com/lmgw-dev/kai-mobile/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="LICENSE.md"><img src="https://img.shields.io/badge/license-MIT-blue" alt="License: MIT"></a>
  <img src="https://img.shields.io/badge/Android-12%2B-informational" alt="Android 12 and later">
</p>

Kai mobile is an Android client of [lmgw](https://github.com/lmgw-dev/lmgw), the self-hosted gateway
that runs your models on your own GPU and can route to cloud providers too. It is the phone side of
[Kai](https://github.com/lmgw-dev/kai), the voice assistant for the KDE Plasma desktop: the same
ongoing conversation, held in lmgw's Chat, now on your phone. Kotlin and Compose around a Rust core
(`core/`, UniFFI).

- **Talk to Kai.** Voice calls over WebRTC, with the speech recognition, the chat model and the voice
  all coming from your lmgw. Or type: the conversation is the one you also read in lmgw's dashboard.
- **Assistant mode.** Kai can be Android's voice assistant, started from the assist gesture, a Quick
  Settings tile or a launcher shortcut.
- **Notifications from your gateway.** The app holds lmgw's Chat feed open while the phone is paired,
  so the answer to a job that outlived the turn reaches you as a notification.
- **Approvals by tap.** When a tool asks first, the card shows up on the phone and a tap answers it.
- **Nothing in between.** The phone talks to your lmgw only, over mutual TLS, with a key made in the
  Android Keystore that never leaves it. No Google services, no analytics, no message cache.

Kai mobile runs no models itself and has no account. Without an lmgw to pair with it does nothing.

## Status

Alpha, and this is the first public release (0.1.0). It is built against a pinned lmgw (`LMGW_PIN`)
and has had little time on real phones, so expect rough edges. Settings, screens and the wire to
lmgw can still change between releases. Bug reports are welcome as issues.

## What it needs

- **Android 12 (API 31) or later**, 64-bit ARM (`arm64-v8a`; the release APK carries nothing else).
- **lmgw 0.5.0 or later**, with its device link (the next sections pair it).
- **A network path from the phone to lmgw's device port**: the same Wi-Fi, or a VPN such as
  WireGuard. A pairing link can carry several addresses (LAN, VPN) and the app uses whichever
  answers. The device port is a TLS listener of its own, next to the dashboard's port, so the host
  running lmgw has to admit it through its firewall (TCP for the app, TCP and UDP for calls).

## Install

1. Download `kai-mobile-<version>.apk` and `SHA256SUMS` from the
   [latest release](https://github.com/lmgw-dev/kai-mobile/releases/latest).
2. Check the download with `sha256sum --check SHA256SUMS` in the folder that holds both. With the
   Android build tools, `apksigner verify --print-certs kai-mobile-<version>.apk` prints the signing
   certificate; it is the same for every release (an update signed with another key does not install
   over the old app).
3. Open the APK on the phone. Android asks once to allow installs from the app you opened it with
   (your browser or file manager). From a computer, `adb install --user current kai-mobile-<version>.apk`
   does the same.

To update, install the newer APK over the old one; the pairing stays. The releases are ordinary
GitHub releases, so an updater that follows a repository's releases can be pointed at this one (not
tried yet).

## Pair it with lmgw

1. In lmgw's dashboard open **Usage**, then **Keys & budgets**, and on the **Devices** card choose
   **Pair a device**. Name it, pick what it may reach, and review the step that shows exactly that.
   lmgw's device listener is off until the first pairing turns it on. The first device that is not
   lmgw's own computer makes lmgw widen it from loopback to the network, after a confirmation that
   names the exposure.
2. lmgw shows a QR code and the same link as text (`lmgw-pair:…`). It is a credential for as long as
   its token lives (ten minutes by default) and works once: nobody else should see it.
3. In Kai mobile, scan the QR code with the camera, or tap the link, or paste it. The app makes its
   key in the Keystore, enrols the public half with the link's one-time token and shows a short
   confirmation code such as `7F3A-91C2`.
4. On lmgw's Devices card, the pairing shows the same code. Check that it matches, then press
   **Confirm**. Until you do, lmgw refuses the device, so a photographed QR code buys a stranger a
   row that you see and delete, never a working device.

The card also disables or revokes a device, and re-pairs it (a new link for the same row); the app
says what happened and asks you to pair again. The designs record all of it:
[the device link](https://github.com/lmgw-dev/lmgw/blob/main/docs/design/2026-10-14-device-link-design.md)
in lmgw's repository and
[`docs/design/2026-10-14-kai-mobile-design.md`](docs/design/2026-10-14-kai-mobile-design.md) here.

## Build from source

Needs Podman and [`just`](https://github.com/casey/just); the Android toolchain lives in the image
(`ci/Containerfile`: JDK 21, the Android SDK and NDK, rustup with both Android targets, cargo-ndk).
`just check` also runs the core's gates on the host, so it needs cargo and
[cargo-nextest](https://nexte.st/) there as well.

- `just check`: what CI runs (the publish scripts' tests, core fmt, clippy, tests, no aws-lc; Gradle
  lint and unit tests)
- `just apk` / `just apk-release`: the debug APK (both ABIs, application id `dev.rootshell.kai.dev`) /
  the release APK (`arm64-v8a`, unsigned, see "Signing the release"), under
  `android/app/build/outputs/apk/`
- `just verify-apk`: 16 KB alignment of every library and the APK sizes
- `just install [apk=path]`: the debug APK (or the given one) onto the phone with host adb, for the
  current user only (`scripts/adb-serial.sh` picks the serial)
- `just core-test`: the core's host tests

**Never bind-mount the checkout, a worktree or a parent of one with `:Z` or `:z`.** The relabel is
recursive and outlives the container (the repo ends up `container_file_t` with a random MCS category,
which breaks confined tools such as pasta). `scripts/in-container.sh` therefore builds on a scratch copy
under `~/.cache/kai-mobile/stage/` and copies the APKs, reports and test results back; only that copy
and the caches are relabelled. Any new `podman run` follows the same rule (copy in, copy out).

lmgw's checkout must sit next to this repository (`../lmgw`): the core takes `lmgw-client` and
`lmgw-api-types` from it by path. `LMGW_PIN` names the source the build is made against, and the
container build stages the pinned tree. To get that lmgw, clone its published release:
`scripts/lmgw-pin.sh clone-published ../lmgw` (the pin's `lmgw_commit` belongs to lmgw's private
development history, so it is of no use outside the maintainers' network).

### Dependencies

Gradle checks every dependency against `android/gradle/verification-metadata.xml` (sha256) and cargo
builds run `--locked`. After a dependency bump, regenerate the metadata in the image with an empty
Gradle home (a warm cache hides the `.module` files and parent POMs a fresh one resolves):
`just gradle-verification`, and review the diff. The private CI's image tag (`KAI_IMAGE_TAG` in
`.gitlab-ci.yml`) must follow `ci/Containerfile`: `just image-tag` prints it, `just check` fails when
it is stale.

## Versions

`kai.version` in `android/gradle.properties` is the app's version. Three kinds of build carry it
differently (`android/app/build.gradle.kts`):

| Build | `versionName` | `versionCode` |
|---|---|---|
| GitHub release (`KAI_PUBLIC_RELEASE=1`, set by the release workflow) | `X.Y.Z` | `100000000 + X * 1000000 + Y * 1000 + Z` (0.1.0 is 100001000) |
| The maintainers' private CI, on every push to `main` (`KAI_VERSION_CODE` is its pipeline number) | `X.Y.Z+<pipeline>` | the pipeline number, 1 to 99999999 |
| A local build | `X.Y.Z+local` | 1 |

A release's `versionCode` rises with every release whatever the private pipelines count, stays far
below Android's limit of 2100000000 (a version field above 999 is refused) and is always higher than
any private build's. **GitHub is the public channel.** The private CI's builds are for the
maintainers. If it signs with the same key as the releases, a phone can go from one of its builds to a
GitHub release but not back: Android refuses a downgrade, and the way out is to uninstall the app,
which loses the pairing.

## Maintainers

### The lmgw pin

`LMGW_PIN` holds three lines naming the same lmgw source: `lmgw_commit` (the commit in lmgw's
development history, which the private CI clones), `lmgw_tree` (its tree: the public repository gets
one squashed commit per release, with other hashes but the same tree, so the tree identifies the
source) and `lmgw_tag` (the published lmgw release with that tree, which GitHub's workflows clone, or
`unpublished` while no release has it yet).

GitHub's workflows clone `https://github.com/lmgw-dev/lmgw` at `lmgw_tag` next to the checkout and fail
unless its tree is `lmgw_tree` (`scripts/lmgw-pin.sh clone-published ../lmgw`; with `unpublished` they
say so). A release therefore needs lmgw's release first, then its tag in `LMGW_PIN`.
`scripts/lmgw-pin.sh verify-published` checks the pin against GitHub, and `scripts/release.sh` and
`scripts/publish-github.sh` refuse a pin that fails it. `just pin-bump` moves the pin to `../lmgw`'s
pushed HEAD, updates only lmgw's two crates in the core's lock and runs `just check`.

### Releasing

1. `scripts/release.sh X.Y.Z [notes-file]` sets the version in `android/gradle.properties` and in
   `core/Cargo.toml` with its lock, commits `release X.Y.Z` on `main` and makes the annotated tag
   `vX.Y.Z`, whose message becomes the release notes (Markdown; wrapped lines are joined). When the
   tree already carries the version (0.1.0), it only tags. It pushes nothing.
2. `scripts/publish-github.sh --install-guard`, once per clone and before the first publish: it adds the
   `github` remote, disables a plain `git push github` and installs `scripts/pre-push-github.sh`. The
   hook refuses any push to GitHub that does not come from the publish script (`KAI_MOBILE_PUBLISH=1`,
   a variable of this repository alone) and the private history to every URL but the private
   origin's. `just publish-check` tests all of it against local bare repositories.
3. This repository receives one squashed commit per publish, an audited and scanned copy of a tree,
   never the development history:
   `scripts/publish-github.sh --dry-run --ref vX.Y.Z -F <message> --tag vX.Y.Z`, read the audit files
   it writes under `target/publish/`, `--approve <tree> --note "who audited"` (with `--ack-scan` once
   every scan hit is fine), then the same without `--dry-run`. It reads the public parent from the push
   URL (the first publish, to the empty repository, is a root commit), refuses anything that descends
   from the private history's root commit, and asks for the word `publish` on a terminal.
   `scripts/publish_scan.py` looks for secret shapes, every non-GitHub remote host and the words of a
   private denylist (`${XDG_CONFIG_HOME:-~/.config}/lmgw/publish-denylist`, or
   `KAI_MOBILE_PUBLISH_DENYLIST`; one case-insensitive regular expression per line, kept outside the
   repository). The commit's author and committer must be GitHub no-reply addresses.
4. The tag push starts `.github/workflows/release.yml`. A read-only job builds the unsigned APK from
   the tag (with no secret in reach), signs it in a container of its own and checks it (16 KB
   alignment, version, signature, and the signer's certificate against the pinned SHA-256); a second
   job with `contents: write`, which checks out no code of this repository, creates the GitHub
   release from the tag's message with the APK and its `SHA256SUMS` attached. A re-run of a tag whose
   release is still a draft replaces the draft; a published release makes it fail.

### Signing the release

Create the release keystore yourself and keep an offline backup: losing it means a reinstall and a
new pairing on every phone. In Settings, Secrets and variables, Actions of the GitHub repository:

- Repository **secrets**:
  - `KAI_KEYSTORE_B64`: the PKCS12 keystore as `base64 -w0 release.p12`.
  - `KAI_KEYSTORE_PASSWORD`: its password.
  - `KAI_KEY_PASSWORD`: optional; a PKCS12 keystore uses the store password for its key.
- Repository **variables** (public, not secrets):
  - `KAI_KEY_ALIAS`: the key's alias.
  - `KAI_SIGNING_CERT_SHA256`: the SHA-256 of the release certificate, 64 hex digits (colons are
    fine). Get it from the keystore with
    `keytool -list -v -keystore release.p12 -alias <alias>` (the line `SHA256:`), or from a signed
    APK with `apksigner verify --print-certs`. The release fails while the variable is unset, and
    again when the signed APK carries another certificate, so a changed key, a wrong keystore or a
    tampered build cannot reach a release unnoticed.

The secrets are visible to one step only. The build runs third-party code (Gradle plugins, build
scripts and proc macros of the dependencies), so it gets no secret and no keystore and produces
`app-release-unsigned.apk`. The next step decodes `KAI_KEYSTORE_B64` into a temporary file outside
the checkout and runs `scripts/github-sign.sh`: a container of the same image with no network and no
capabilities, which mounts only that keystore and the unsigned APK (read-only) and signs with the
image's `apksigner`. The step after it verifies the signed APK with no secret in the environment.
A local release APK stays unsigned until you run `apksigner` on it, as `scripts/github-sign.sh` does;
debug builds use the SDK's debug key.

The maintainers' private CI (`.gitlab-ci.yml`) takes the same values as protected CI/CD variables,
the keystore as a File variable, signs through Gradle and builds every push to `main` as
`X.Y.Z+<pipeline>`.

### GitHub's CI

`.github/workflows/ci.yml` runs `just check` on every pull request and push to `main`, in the image of
`ci/Containerfile`. The job builds the image itself (`scripts/github-container.sh`, about ten minutes)
instead of pulling or caching it: it is 8.7 GB, a third of the repository's cache quota. Only cargo's
registry, Gradle's caches and the core's target directory are cached, and saved from `main` only. A
release builds everything from scratch.

## Links

- [lmgw](https://github.com/lmgw-dev/lmgw), the gateway Kai mobile pairs with
- [Kai](https://github.com/lmgw-dev/kai), the desktop assistant
- [lmgw-dev.github.io](https://lmgw-dev.github.io/)

MIT, see [LICENSE.md](LICENSE.md).
