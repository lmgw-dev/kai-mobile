# WP0 device spike: results

One verdict per point, as design §17's WP0 row asks (`docs/design/2026-10-14-kai-mobile-design.md`).
A failed point changes the design first.

- **The app:** `spikes/wp0/app` (`dev.rootshell.kai.spike`, "Kai WP0 spike"), separate from Kai: its
  own assistant role, Keystore aliases and data. `just spike-apk` builds it (release, not debuggable,
  the SDK's debug key, arm64-v8a), `just spike-install` installs it for the current user.
- **The log:** every measurement goes to the screen and to `wp0/wp0.jsonl` in the app's external
  files directory. `just spike-log` pulls it with the recordings into `spikes/wp0/results/<time>/`
  (gitignored) and runs `spikes/wp0/fill-results.py`, which rewrites the blocks between the
  `auto` markers below from every pulled log. Everything outside those blocks is written by hand:
  the fields marked *(you)* are need a person at the device (on either test phone).
- **The phones:** a foldable Samsung test phone (One UI 8, Android 16) for every point; a phone
  without Google services (e.g. LineageOS) for (a) and (o). Each log says which phone it came from.

<!-- auto:phones -->
*(no log pulled yet)*
<!-- /auto:phones -->

## (a) Invocation

**Procedure** ((a)'s screen lists it): check the setup; request the role and say whether a dialog
showed; set the spike as the digital assistant and, on the test foldable, as the side button's press and
hold; 10 warm presses on the cover screen and 10 unfolded; cold presses after "Kill the process"
(or `adb shell am force-stop dev.rootshell.kai.spike`); both Expand buttons; the
`setDisabledShowContext` toggle on and off; on the LineageOS device the power button's long press.

| Field | Result |
|---|---|
| One UI 8 offers the spike under Side button → Press and hold *(you)* | |
| The press reaches `onShow` (an `a.first_frame` with source `system`) | |
| `createRequestRoleIntent(ROLE_ASSISTANT)` shows a dialog (test foldable / LineageOS) | |
| LineageOS: the power button's long press reaches the overlay *(at the device)* | |
| LineageOS: a user setting for it exists, or `power_button_long_press 5` was needed *(at the device)* | |
| First frame, warm: cover / inner (median, max; target < 300 ms) | |
| First frame, cold: cover / inner (recorded) | |
| Whether a cold invocation happens at all (the system restarts the assistant's process at once?) | |
| Expand: `startAssistantActivity` / `startActivity` work, and their first frame | |
| `setDisabledShowContext` takes effect (no `onHandleAssist`/`onHandleScreenshot` with it on) | |

<!-- auto:a -->
*(no log pulled yet)*
<!-- /auto:a -->

**Go/no-go (design §9.4, §9.5, §10.4):** go when the side button (test foldable) and the power button
(LineageOS) reach `onShow` and the warm first frame stays under 300 ms on both screens; a miss is
recorded with its numbers and decided on. If the role request shows a dialog, it replaces the deep
link in setup (§9.5). If `startAssistantActivity` fails, Expand keeps `startActivity` with
`FLAG_ACTIVITY_NEW_TASK` (§9.2). If One UI 8 does not offer the spike for the side button, the
invocation paths of §9.4 fall back to the corner swipe, the tile and the shortcut, and WP4 changes.

**Verdict:**

## (b) The microphone type

**Procedure** ((b)'s screen): a LAN host for the ping; Run from the screen (the service starts as
`specialUse`, the visible activity adds `microphone`), screen off for 10 minutes, then the summary;
the same from the overlay (Mic on, then dismiss), with the service running and with it stopped; the
lock screen's notification and its End; the background control (Add in 10 s, then Home).

| Field | Result |
|---|---|
| `microphone` added from a visible activity (a `b.foreground` with origin `activity`, ok) | |
| `microphone` added from the assistant session, service running / not running | |
| `microphone` from the background (control): refused, or allowed by the assistant's exemption | |
| Dropped at the end (back to `specialUse` alone), the indicator goes out *(you)* | |
| The capture survives dismissal and 10 minutes of screen-off: lost audio, longest gap, silenced | |
| The microphone indicator during the capture, screen on and off *(you)* | |
| The notification: shown on the lock screen with Mute and End, End works *(you)* | |
| Wi-Fi latency, screen on against screen off (median, p95, losses) | |

<!-- auto:b -->
*(no log pulled yet)*
<!-- /auto:b -->

**Go/no-go (design §8.6, M16):** go when the type is added from the assistant session and from a
visible activity and the capture runs 10 minutes with the screen off without loss or silencing. If
the type cannot be added from the assistant session, the call gets a `microphone` service of its
own, as in v3 (§8.6). If screen-off latency rises markedly, a Wi-Fi lock is reconsidered (§8.6 has
none).

**Verdict:**

## (c) Time to listening (go/no-go of §7.2)

*Waiting for lmgw* (calls WP P1, or the str0m stand-in behind TLS). The go/no-go: from the side
button's `onShow` to the listening cue (DTLS connected, the earcon played), over Wi-Fi, warm
process, on the test foldable: **median 1.0 s or less over 20 runs, and no run over 1.5 s**, for the LAN URL
and the tunnel URL alike; mobile data through the tunnel and a cold process recorded beside it. The
press-to-DTLS gap split into its steps, the TLS handshake and its signature among them ((e) has the
handshake's own cost); the gap when an `AudioRecord` capture hands the microphone over to libwebrtc
(both for Q5). (c)'s peer also completes (d)'s software column (below).

**Verdict:**

## (d) Hardware against software echo cancelling

**Procedure** ((d)'s screen): a quiet room, the phone on a table; Run (three voice-call volumes ×
hardware and raw, about 2 minutes) folded, then unfolded; silent throughout. Optional: your own
clip as `wp0/clip.wav` (pushed, never committed). The recordings stay in `wp0/echo/`.

| Field | Result |
|---|---|
| False barge-ins, hardware (VOICE_COMMUNICATION + AcousticEchoCanceler), per volume, folded / unfolded | |
| False barge-ins, raw (no platform AEC), per volume, folded / unfolded | |
| False barge-ins after the NLMS stand-in, per volume | |
| `AcousticEchoCanceler.isAvailable()` and whether it enabled | |
| The recordings through a real VAD (lmgw's), if the energy VAD's counts are in doubt *(you)* | |

<!-- auto:d -->
*(no log pulled yet)*
<!-- /auto:d -->

**Go/no-go (design §8.2):** `hardware` stays the default where `AcousticEchoCanceler.isAvailable()`
when it gives no false barge-ins at the three volumes on the speaker, folded and unfolded; else
the default on the test foldable is `software` (WebRTC's AEC3). **Limit of this run:** AEC3 cancels only
what libwebrtc itself plays, so a clip played through `AudioTrack` cannot test it; the raw column
and the NLMS stand-in show the problem's size, and the AEC3 column is measured through (c)'s peer
(the clip as the remote track). See the design's dated note in §8.2.

**Verdict:**

## (e) UniFFI, JNA and TLS

**Procedure** ((e)'s screen): Run five times (each in a fresh process, so every figure is cold),
once with JNA keeping tokio's threads attached, once Run here. The real app's APK sizes come from
`just apk-release` and `just verify-apk`.

| Field | Result |
|---|---|
| The real app's release APK (arm64-v8a) size; the spike's APK size | |
| 16 KB alignment of every `.so` (zip offset and ELF p_align), on the phone and by `just verify-apk` | |
| The first call: JNA's library, then the core (load, registration, `Core::new`) | |
| `EventSink` from a tokio worker against the calling thread: the attach and detach per call | |
| The same with the threads kept attached (`CallbackThreadInitializer`, detach false) | |
| Keystore key generation; `SHA256withECDSA` through the callback, cold and warm; security level | |
| Full TLS 1.3 handshake with client authentication, loopback, ring both sides: first and warm | |
| Its Keystore signature inside the handshake | |

<!-- auto:e -->
*(no log pulled yet)*
<!-- /auto:e -->

**Go/no-go (design M4, M7, §2.1, §10.4):** the handshake with its signature is recorded and weighed
against HTTP/2 on lmgw's device listener (§7.2, §10.4); every library 16 KB aligned (a library that
is not blocks 16 KB-page devices); the attach and detach cost decides whether tokio's threads are
kept attached (a `CallbackThreadInitializer` in Kotlin, or `on_thread_start` attaching in the core).

**Verdict:**

## (f) Networks

*Waiting for lmgw* (calls route, P1). libwebrtc gathering on every network against lmgw's one
candidate: which local candidate wins on a LAN-URL and a tunnel-URL call; a Wi-Fi to mobile-data
handover mid-call over the tunnel; leaving the Wi-Fi mid-call on a LAN-URL call with and without a
tunnel that routes lmgw's LAN address; the core's URL race and its reconnect on a change of the
default network.

**Verdict:**

## (g) Push-to-talk's tail

*Waiting for lmgw* (calls route, P1). Clipped endings against the release tail, which sets
`voice.ptt_tail_ms`, and whether that tail fits inside lmgw's `webrtc_commit_window_ms` (else the
window is raised before WP3), with the press and release times kept for P6's alternative.

**Verdict:**

## (h) `starved`

*Waiting for lmgw* (calls route, P1, P2). `lmgw.output_audio.starved` in a tool turn with a spoken
preamble.

**Verdict:**

## (i) Headset buttons

**Procedure** ((i)'s screen): Run the button test with a wired headset and with a Bluetooth one,
pressing once, twice and holding; change the toggles if nothing arrives; Route and record over
Bluetooth while speaking; once without and once with `BLUETOOTH_CONNECT`.

| Field | Result |
|---|---|
| Wired: the keys that reach the session (single, double, long press) | |
| Bluetooth: the keys that reach the session, outside and during SCO | |
| SCO setup: `setCommunicationDevice` to the device change, and to the recording's route | |
| HFP capture: the client and device sample rates, the bandwidth (narrowband or wideband) | |
| Bluetooth names without `BLUETOOTH_CONNECT` *(compare the device lines)* | |
| A few turns over HFP once lmgw's calls exist (ASR accuracy) *(waiting for lmgw)* | |

<!-- auto:i -->
*(no log pulled yet)*
<!-- /auto:i -->

**Go/no-go (design §8.3, §8.4):** go when a headset's press reaches the session (mute, push-to-talk,
End as §8.4 maps them) and SCO sets up within a few seconds; if HFP is narrowband, the ASR's
accuracy over it is checked once lmgw's calls exist; if names need `BLUETOOTH_CONNECT`, WP3 adds it.

**Verdict:**

## (j) The takeover

*Waiting for lmgw* (calls route, P1, P8). `bind_pending` with `held_by`, Take over by
`lmgw.session.take_over`, the desktop's 4000, the desktop ending while the prompt shows, a decline,
the wait running out (`thread_bound`), End and the desktop's state after it; the microphone `Off`
while pending (the indicator out, the call alive without RTP until lmgw's bound) and the restart
from `session.created` to the first captured samples.

**Verdict:**

## (k) On the LAN

*Waiting for lmgw* (device link, P9). With a WireGuard tunnel profile up while on the LAN: which URL
answers first and whether a call connects through it; a full tunnel's "Exclude private IPs";
whether One UI 8 enforces Android 16's local network protection on the LAN URL.

**Verdict:**

## (l) Playout facts

*Waiting for lmgw* (calls route, P1, P2). `estimatedPlayoutTimestamp`, `jitterBufferDelay` and
`jitterBufferEmittedCount` in the remote track's `inbound-rtp` stats, how often they update, whether
the playout time lands on lmgw's sent timeline, how long a `getStats` read takes after the mute, and
that disabling the remote track silences it at once.

**Verdict:**

## (m) The held feed

*Waiting for lmgw* (P9, P11). The `specialUse` service holding a feed over mutual TLS at
`keepalive_s=120` for a day: the battery drain per day with lmgw reachable for part of the day and unreachable for the rest;
whether an event raises its notification within seconds after an hour of screen-off and Doze; the
battery exemption and Samsung's "Never sleeping apps" (its path on One UI 8); the restart after a
process kill and after a reboot, resumed by the cursor with `caught_up` and no double alert; how
soon a silent drop is noticed with the screen off; the inexact alarm's spacing under Doze.

**Verdict:**

## (n) The two screens

**Procedure** ((n)'s screen): Run folded and unfolded; a capture held by the service, then folds and
unfolds (also mid-sentence); the overlay shown while folding; One UI's "Continue apps on cover
screen" on and off.

| Field | Result |
|---|---|
| Cover screen: px, dp, density, width and height classes (`WindowMetrics`) | |
| Inner screen: the same | |
| A fold and an unfold mid-capture: recreated, capture alive, lost audio and the longest gap | |
| The overlay on both screens (its size on each, full width on the cover, ≤ 640 dp on the inner) | |
| Continue apps on cover screen: on / off, what happens to the open spike *(you)* | |

<!-- auto:n -->
*(no log pulled yet)*
<!-- /auto:n -->

**Go/no-go (design §10.6, M23, §10.4):** the cover screen is compact and the inner one medium or
wider, as the layouts assume; a fold or unfold mid-capture recreates the activity, keeps the
capture in the service and loses nothing (§10.4: "no reconnect, nothing lost"); the overlay works on
both screens.

**Verdict:**

## (o) LineageOS without Google's services

**Procedure** ((o)'s screen): grant notifications, set the spike as the assistant for the overlay
item, Run the self-check; on the LineageOS device it is done the same way (or
`adb shell am start -n dev.rootshell.kai.spike/.SpikeActivity --es autorun o`, then `just spike-log`).

| Field | Result |
|---|---|
| The APK installs and starts *(at the device)* | |
| The core loads; libwebrtc initialises and makes an offer; a Keystore key signs | |
| The overlay; a notification | |
| The Android version (M15: minSdk 31 stands unless it is older) | |

<!-- auto:o -->
*(no log pulled yet)*
<!-- /auto:o -->

**Go/no-go (design M15, M17):** go when every item passes on the LineageOS device; its Android
version is recorded for M15 (minSdk 31 stands unless it is older; below Android 14 there is no
`specialUse` type, see the design's dated note in §8.6).

**Verdict:**
