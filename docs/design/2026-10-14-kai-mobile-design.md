# Kai mobile: an Android client of lmgw

**Approved v4, 2026-10-14, ready for implementation.** v3 plus the owner's answers to Q1 to Q11
(§18), the device link that lmgw's design of the same day brings (lmgw 2328902a, both designs approved, with the socket groups), the
final names of lmgw's calls design, and a foldable test phone with two screens.
Nothing of it is built. *Changed 2026-10-15 (WP3 part 1):* built against lmgw e4c248d6 (the pin stays f6917516, whose
lmgw-client already has every call API the core uses; the pin is bumped when it lands): the call engine (§7.1 to
§7.11 as noted there) with host tests for the rules (`voice/call/tests.rs`, sans-IO on explicit times)
and the signalling against the scripted lmgw (`tests/it/voice.rs`: the POST by the folder on the warm
connection, the part and `new`, the folder's 404 and retry, a refusal, Reconnect's hangup, the
microphone off for another holder, a call from the cue to the End, a key refusal, no reply notification
during a call and again after it; after the review also an older call's events dropped, every owed
id hung up oldest first, and a call's id hung up across a process restart); Kotlin's libwebrtc side
(`voice/CallEngine.kt`), the microphone type and the call's notification, the minimal voice screen
(`ui/voice/VoiceControls.kt`) and the voice settings page. Not run against a dev lmgw: a real call
needs a model with ASR and TTS behind the folder and lmgw's device-link check script (its WP7) does
not exist yet, while lmgw's own suite already calls through the device listener with a str0m phone
(`realtime_calls::devices`); the acceptance checks below are the live run. **Part 2** (not built):
Bluetooth routing and SCO, the route button and the device-change listener (§8.3), the MediaSession
and headset buttons (§8.4), audio focus and "audio taken" (§8.5), the echo setting's half duplex and
the factory's rebuild between calls (§8.2), the polished voice screen on both screens with the fold
and unfold handling (§10.6), the ring's levels and animation (§10.3), polished earcons (§10.5).
*Changed 2026-10-15 (WP3 part 2):* part 2 is built against the same pin (34d25cba), each point as its
dated note says: the core's rules for the focus, the headset's buttons and half duplex (`Voice::focus`,
`Voice::headset`, `EchoMode::HalfDuplex`, `EndReason::AudioTaken`, host tests in
`voice/call/tests/device.rs` and `tests/it/voice.rs`), the app's `VoiceController` (§9.3: routing,
focus, the MediaSession, the earcons, around `CallEngine`, which rebuilds its factory when
`voice.echo` changed and feeds the ring's levels), the voice screen on both screens (`ui/voice/`),
and JVM tests for the routing rules, the focus and headset-key decisions, the canceller by the
setting, the ring's motion, the earcons and the levels. Not run on a phone: the acceptance checks for
part 2 follow part 1's. *Changed 2026-10-15 (WP3 part 2's review):* the dated notes of §8.3 to §8.5,
§9.3, §10.3 and §10.5 say what changed: a route Android leaves or never reaches is said and the rule
picks again at the next device change, a refused focus sets nothing up, a pause stops the idle clock
and nothing of Kai's sounds plays into another app's audio, a call's start always reaches the
controller before its end, the audio goes once the ending earcon really played, the ring draws only
what moves, TalkBack hears a change once, and the ring's colours are Kai's and readable in the light
theme too. The state changes of the routing, the controller and the call's start and end are pure,
with JVM tests for the cases the review named.
*Changed 2026-10-15 (WP4):* assistant mode is built against the same pin, each point as its dated
note says (§7.7, §9.1 to §9.6, §10.4): the core's rule for the overlay's dismissal
(`Voice::dismissed`, `EndReason::Dismissed`, `VoiceView::spoken`, host tests in
`voice/call/tests/dismiss.rs` and `tests/it/voice.rs`), the interaction service, the session overlay
on both screens, the stub recogniser, the `ASSIST` and `VOICE_COMMAND` filters, the Quick Settings
tile and the launcher shortcut, and setup's assistant card (`assist/`, `TalkActivity`,
`ui/screens/AssistantPage.kt`), with JVM tests for the overlay's decisions and faces, the filters and
the card's steps. Not run on a phone: WP0 (a) has no verdict yet, and the points its verdict can
change are each one named seam (§9.2's, §9.5's and §9.1's notes). The owner's checks are items 19 to
26 below.
*Changed 2026-10-16 (WP5):* approvals in a call and the polish are built against the same pin, each
point as its dated note says (§3.2, §7.6, §7.7, §9.2, §9.6, §10.2 to §10.6, §11, §12.3, §15.3, §17):
the core's book merges the call's `mcp_approval_request` items with the feed's open set into one card
per call, decides a reply through the call that showed it (or by the route), reads lmgw's answer to
the verdicts, says what the call waits for, and answers a late task result with the continuation
(`approvals/call.rs`, `voice/call/approvals.rs`, `voice/call/continuation.rs`, host tests in
`approvals/tests.rs`, `voice/call/tests/approvals.rs`, `tests/it/voice.rs` and `tests/it/held.rs`);
the app shows the card in the overlay, on the voice screen and in the conversation
(`ui/approvals/`), adds the ask's earcon and haptic, the confirm and reject haptics, and carries out
the audits' fixes (predictive back, insets, TalkBack, font scale, size classes, scrolling measured),
with JVM tests for the card's and the layouts' pure decisions and the frame tally. Not run on a
phone: the acceptance checks are items 27 to 40 below.

**Acceptance of WP3 part 1 on the foldable test device** (`just install` of the debug APK; a dev lmgw
with the Kai folder ongoing and a voice-capable model; each item names §17's "done when"):
1. *Round trips through the LAN URL and the tunnel URL in both modes:* Talk on the voice screen, allow
   the microphone; the ring turns listening with the earcon at DTLS-connected; say something, Kai
   answers; switch push-to-talk on, hold "Hold to talk", speak, release, Kai answers; repeat over the
   tunnel URL (WireGuard on, the LAN URL unreachable). Note: `voice.ptt_tail_ms` is not set, so the
   last syllable may be cut; set it from WP0 (g) or by ear on the voice settings page, within lmgw's
   `webrtc_commit_window_ms`.
2. *Words spoken right after the cue reach the turn:* speak at the earcon; the transcript caption has
   your first word.
3. *A stop by speech and by tap leaves the heard part in the thread:* interrupt Kai by speaking
   (automatic, speaker; the echo setting is part 2), and tap the ring or Stop mid-reply; the caption
   and the conversation keep only what was heard ("cut where it was heard").
4. *A held thread prompts on a connected call:* with desktop Kai listening, Talk; the screen says
   "Voice is on device '<desktop>'" with no earcon and the microphone indicator off; Take over moves
   the voice without a second setup (the earcon at the first samples); then press the desktop's talk
   key: the phone says "Voice moved to <desktop>" with Take over (as a notice when the app is not on
   screen, its Take over starting a new call); End while the prompt shows declines; leave the prompt
   unanswered: "Voice stayed on <desktop>" after lmgw's `realtime.ping_interval_s`. This result assumes
   `realtime.ping_interval_s` < `voice.idle_end_s` (a pending bind counts as quiet): otherwise the
   phone's idle end declines first, with the idle's words.
5. *End leaves no open response:* End while Kai speaks; the thread keeps the cut reply and no turn
   runs on afterwards.
6. *10 minutes with the screen off:* start a call, lock the phone; the lock screen's notification
   shows "Kai · voice call" with Mute and End; talk for 10 minutes; Mute and End from the lock screen.
7. *A fold and an unfold mid-call keep the call:* fold and unfold while Kai speaks; the call goes on
   (the polished two-screen layout is part 2: item 13).
8. *A Bluetooth headset routes and its button mutes:* **part 2** (part 1 routes a wired or USB headset
   only): item 12.
9. *A dev hold shows the badge and the fallback's name:* switch the GPU hold on in the dev lmgw; the
   voice screen shows the hold, a held stage, and "answered by <alias> (fallback)" where a fallback
   answered, or the turn's error with its code where none is set.
10. *A dev lmgw stopped mid-call says so:* stop the dev lmgw during a call: "lmgw is shutting down, or
    its device access changed" with Reconnect; Reconnect once it is back connects.
11. *The lock screen shows no content under Android's default settings* (§12.3, the owner's decision;
    the lock screen's notification setting left at "Show all notification content"): with an approval,
    a reply and a call shown, lock the phone and wake it: the lock screen shows only "Kai needs your
    approval", "New reply from Kai" and "Kai · voice call" with Mute and End (no title, thread, words,
    count, Approve or Talk); unlock: the content and the actions are back.
Also worth a look: the idle end after 30 s of quiet (`voice.idle_end_s`), the microphone indicator
during mute (lit: mute sends silence) and while a bind is pending (out), a new conversation's rebind
at the next quiet point, and the logcat lines tagged `kai` for each step.

**Acceptance of WP3 part 2 on the foldable test device** (as part 1's; *added 2026-10-15*). Each item
names §17's "done when" (or the row of §10.4 or WP0 point it closes):
12. *A Bluetooth headset routes and its button mutes* (§17; item 8 above; WP0 (i)): with a Bluetooth
    headset connected, Talk: the route button names the headset (with "switching…" until Android
    has the SCO link up), Kai is heard in it and hears you through it, the earcon plays in it. Press
    its button while Kai is quiet: "Muted"; again: listening; while Kai speaks: Kai stops (the ring's
    tap, §8.4's note). Say whether a press drops the headset's audio (some headsets end HFP's call
    on their button): the button then says "Android moved the call to …" and the call stays where
    Android put it until the next device change or your choice in the button's menu. Hold the
    button: Android keeps the long press for voice input (§8.4's note), so expect the assistant to
    open and Kai to pause ("Paused: another app took the audio") or end ("voice ended: another app
    took the audio"), and say which; a headset that sends play or pause may end the call instead. In
    push-to-talk: a press starts the turn, the next commits it. The same with a wired headset.
    Connect the headset mid-call: it takes over; switch it off: the call goes back to the speaker.
    Say whether the route button showed the headset's own name (no `BLUETOOTH_CONNECT`, §8.3's note)
    and whether the ASR understood you over it.
13. *A fold and an unfold mid-call keep the call* (§17; item 7 above; §10.4's row; WP0 (n)): unfolded
    with the conversation beside the call, fold while Kai speaks mid-sentence: the cover screen shows
    the same call (the ring speaking, the captions, the route) and Kai's sentence goes on; unfold: the
    inner screen shows it beside the conversation. No reconnect: logcat shows no new "a call starts"
    and no warm-up between the fold and the call's end. Again holding push-to-talk (the fold releases
    the press and the turn is committed), during a pending prompt, and with the cover screen in
    landscape (the ring beside the controls).
14. *The echo modes at three volumes* (§17's "a stop by speech and by tap leaves the heard part"; §8.2;
    WP0 (d)): on the speaker at a low, a middle and the highest call volume, folded and unfolded, a
    few turns with `voice.echo` at the default, at software and at half duplex (the setting applies
    from the next call): with the default and software Kai never interrupts itself and your speech
    cuts it; at half duplex the screen says so, speech does not cut Kai and the tap does. Note any
    false interruption per mode and volume beside WP0 (d)'s counts.
15. *The route button* (§8.3): during a call on the speaker, the button's menu lists the earpiece
    and the speaker (and any headset); the earpiece takes the call at once and the speaker brings it
    back; `voice.route` at earpiece starts the next call there; a device Android refuses, or does not
    switch to within `lmgw.timeout_s`, is said under the button instead of "switching…".
16. *The ring* (§10.3): listening, its halo follows your voice; speaking, Kai's; thinking (a tool
    turn shows it best), an arc runs; connecting, it breathes; muted, idle and at rest it is still,
    struck through when muted; with Developer options → Animator duration scale off, it shows only
    the colours (the change applies with the screen open); in silence, listening, the display's
    refresh rate may drop (Developer options → Show refresh rate); TalkBack reads "Kai is listening"
    and says each state's change once. In the light theme the ring's amber, blue and grey and the
    amber notes read clearly.
17. *Another app takes the audio* (§8.5, §7.7's "audio taken"): mid-call, let another phone ring this
    one and take the call: Kai stops and the screen says "Paused: another app took the audio"; no
    earcon or haptic of Kai's plays into the phone call; talk on the phone for longer than
    `voice.idle_end_s` (30 s): Kai's call is still there afterwards (the pause stops the idle clock),
    muted until you tap, and the idle end counts again from the phone call's end. Start music in
    another app mid-call: the call ends with "voice ended: another app took the audio", without the
    ending earcon.
18. *The earcons* (§10.5): a rising pair of notes at listening, a falling pair at every end (the
    owner's End, the idle end, a lost connection) but one while another app holds the audio, on the
    call's route and heard to its end in a Bluetooth headset (the route goes once it played);
    `voice.earcon` off silences both, `voice.haptics` off the ticks.

**Acceptance of WP4 on the foldable test device and a LineageOS device** (as part 1's, with setup's
"Kai as your digital assistant" page done first: Settings → Kai as your digital assistant; *added
2026-10-15*). Each item names §17's "done when" (or the §10.4 row or WP0 point it closes). The
overlay logs each first frame with the tag `kai` ("assist: the overlay's first frame … ms after
onShow, … after onPrepareShow (warm|cold, compact|wide …)"): `adb logcat -s kai` beside the phone.
If WP0 (a) has not run yet, run it first (the steps below): items 19, 20 and 24 depend on it.
19. *The side button opens the overlay within the budget on both screens and listens* (§17; §10.4's
    first row; WP0 (a)): with Kai set as the digital assistant and the side button's press and hold
    on Kai, the app in the background (warm), press and hold 10 times folded and 10 times unfolded:
    the sheet shows at the cover screen's width, and centred and narrower on the inner screen; the
    ring breathes, then listens with the earcon; say something, Kai answers. The log's first frame
    after `onShow` is under 300 ms warm (a miss is recorded with its numbers, as §10.4 says). Once
    after a force stop (`adb shell am force-stop dev.rootshell.kai.dev`), cold: recorded. Say
    whether the log ever reads "the system handed over screen content" (it must not:
    `setDisabledShowContext`). Fold while the sheet shows, once each way: the sheet redraws at the
    new width, and a locked phone still shows only the unlock face.
20. *The LineageOS power button does the same* (§17; WP0 (a), (o); on a LineageOS device): the page names the
    power button's setting and reads its value; with it at 5 (Settings where the build offers it,
    else `adb shell settings put global power_button_long_press 5`), a long press opens the overlay
    and listens. Say whether the build offered the setting.
21. *Dismissal before a turn closes the call* (§17; §7.7): invoke, say nothing, swipe the sheet down
    (and again with back, a tap above the sheet, Home, and the power button): the falling earcon,
    the microphone indicator goes out, the notification goes back to the link's state, and the voice
    screen's last call shows no words. Invoke and dismiss at once, before the ring listens: no call
    stays either (the earcon only where the call's audio was up already). Invoke, cough only,
    dismiss: the call ends once lmgw finds no words in the turn (`empty_turn`, or a blank
    transcript). Dismiss in the middle of a sentence: the call goes on if the sentence had words.
    A headset's voice button (if the headset sends one) opens the voice screen and starts no call.
22. *After a turn the call goes on* (§17; §9.2): invoke, ask something, dismiss while Kai answers or
    after: Kai's answer goes on in the background, the notification shows the call with Mute and
    End, and the idle end follows after 30 s of quiet; invoke again: the sheet shows the same call
    (no new one, no earcon), says that closing it leaves the call going on, and closing it does.
    The sheet's line before the first turn says that closing it ends the call.
23. *Expand moves the call into the app without a reconnect* (§17; §9.2; WP0 (a)): mid-call (and
    while Kai speaks), tap Expand: the voice screen shows the same call, captions and route, Kai's
    sentence goes on; logcat shows no new "a call starts" and no warm-up between the Expand and the
    call's end, and "assist: into the app by ASSISTANT_ACTIVITY" (or `START_ACTIVITY`). Keyboard:
    the conversation opens (its sheet on the cover screen), the call goes on. Leave the app with
    Home and invoke the overlay again: the call is shown, and closing the sheet leaves it going on
    (it is the app's now). Say whether the app opened as its usual task (Recents shows one Kai).
24. *The locked phone* (§9.6, the privacy decision): lock the phone and press and hold the side
    button: no call starts; if the sheet shows over the lock screen it says only "Unlock to talk to
    Kai" with Unlock (no captions, no thread); Unlock asks for your PIN or fingerprint, then the
    voice screen starts the call; unlocking with the sheet up starts the call in the sheet. With a
    call running, lock and invoke: "Kai · voice call" with Mute and End only. Say whether the sheet
    showed over the lock screen at all.
25. *The tile and the shortcut* (§9.4): add "Talk to Kai" to Quick Settings; tap it unlocked: the
    voice screen starts a call; locked: the unlock comes first. Long-press Kai's launcher icon →
    "Talk": the same. Without the assistant role both still work.
26. *A pending bind in the overlay, and the microphone* (§7.9, §9.5): with desktop Kai listening,
    invoke: "Voice is on device '<desktop>'" with Take over and End; Take over moves the voice;
    dismissing instead declines. Revoke Kai's microphone permission in Android's settings and
    invoke: the sheet says Kai needs the microphone with Open Kai, which asks for it and then starts
    the call on the voice screen. Also worth a look: another app's speech input (a keyboard's
    microphone that uses the system's recogniser) gets an error while Kai is the assistant; logcat
    names the request.

**Acceptance of WP5 on the foldable test device, and a second device's** (as part 1's, with a dev lmgw
whose Kai folder lets desktop Kai's tools ask for approval: `require_approval` on a state-changing
tool such as `desktop__open_file` in the folder's defaults, on lmgw's dashboard; *added 2026-10-16*).
Each item names §17's "done when" (or the §10 row it closes). `adb logcat -s kai` beside the phone.
27. *An approval from the desktop's tools decided on the phone in a call (the card)* (§17; §11): on
    the voice screen, Talk, and ask Kai for something that runs the tool. Kai stops after its
    preamble; the ask plays (one high note twice, with a firm click), the state line says "Kai waits
    for your decision on desktop__open_file", and the card shows the arguments with "In this call:
    Kai speaks the answer once you decide" (above the controls on the cover screen; on the inner
    screen in the conversation beside, at Kai's message, and not on the voice side). No notification
    while the app shows. Wait past `voice.idle_end_s` (30 s): the call stays (a pending approval is no
    quiet). Approve: a confirm tick, the card says sent, Kai speaks the resumed answer in the call and
    the tool runs on the desktop; logcat shows "the verdicts go through the call" and no POST to
    `…/approvals`; the card goes once lmgw has recorded the decision (the feed's
    `approval.decided`). Again with Decline (Kai says it was declined), and once by saying something
    else instead (the card goes: the new turn declined it).
28. *The card in the overlay* (§17; §9.2, §9.6): the same through the side button: the card shows in
    the sheet and decides there. Lock the phone with the card up and press the side button again:
    the sheet says only "Kai needs your approval" with Unlock (no tool, no arguments); after the
    unlock the card is back; Approve. With the sheet dismissed and the call going on in the
    background, the approval comes as one notification (with the ask in the call), not two.
29. *Outside a call, WP2's notification* (§17; §11, §12.3): the app closed and no call, desktop Kai
    asks: one notification; its Approve asks for the unlock first, its Decline does not. Opening the
    app shows the same call as a card; deciding there withdraws the notification; deciding from the
    notification shows the card's verdict (held for the reply's other calls, or sent). Never two
    asks for one call.
30. *A verdict lmgw does not take* (§11): remove the tool from the phone's Device tools (its
    policy's tool scope) and approve: the card says "lmgw does not let this phone approve it (…).
    Decline it here, or approve it on lmgw's dashboard." with the reject haptic, and asks again;
    Decline goes through.
31. *A late task result in a call* (§7.6): a tool that runs as an MCP task (Kai desktop's builder,
    if set up) started in a call; when the task ends, Kai speaks its result at the next quiet moment,
    never over its own speech or yours; logcat shows "the continuation answers the task's result".
    Once more with the task ending while Kai is speaking (ask Kai something long just before it
    ends): Kai finishes, then speaks the result.
32. *Approve while still talking* (§11's note): with the card up in a call, start saying something
    ("actually, what time is it") and tap Approve before you stop. Kai answers what you said; the card
    stays "Approved: sent to lmgw, waiting for its answer" until lmgw records the decline, then goes,
    and you are told "desktop__open_file did not run: what you said or typed reached lmgw before your
    approval, and a new message declines what waits. Ask Kai again if you still want it." (a
    snackbar in the app). The tool does not run on the desktop.
33. *Approve, then End at once* (§7.6's note): with the card up in a call, Approve and tap End right
    after. The tool runs once on the desktop; the card closes with no "lmgw did not take the verdict"
    and no "Already decided"; logcat shows "the call ended before lmgw answered the verdicts" and
    one POST to `…/approvals` (lmgw's 409 `approval_decided` when the call had decided it, the route
    deciding it when it had not). If lmgw decided it and its turn could not start, you are told
    "desktop__open_file was approved, but it never ran: …".
34. *TalkBack* (§10.2's audit): TalkBack on. The voice screen reads Settings, then the ring with the
    state and the words once per change, the call's facts, the controls; in push-to-talk a
    double-tap on the ring or on Hold to talk starts the turn ("Start talking") and the next sends
    it ("Send what you said"), and while muted it is refused with the reject haptic; a turn started
    so and then left (fold or unfold the phone) is sent, never held open. An approval card
    is announced as it appears ("Kai wants to run …", reachable by heading), Approve and Decline are
    labelled; a caption line is one item ("Kai, …"); a message in the conversation starts with "You"
    or "Kai"; the settings' switches, radio buttons and checkboxes are read with their labels and
    state, and setup's steps say done or to do.
35. *200 % font* (§10.6's audit): Settings → Display → Font size and style at its largest (and
    Android's 200 % where the build offers it): on the cover screen the voice screen scrolls as one
    column with every control reachable, rows of buttons wrap, a card's arguments scroll inside the
    card, the overlay's sheet scrolls when it is taller than the screen; on the inner screen voice
    and conversation stay side by side; nothing is cut off.
36. *Both screens* (§10.6): cover screen in portrait and in landscape (the ring beside the rest),
    the inner screen (voice and conversation side by side; the pairing screen and the conversation
    at a readable width); fold and unfold with a card up: the same card and its held verdict.
37. *Edge to edge and predictive back* (§10.2): no content under the system bars or the cover
    screen's camera in landscape; the conversation's field sits on the keyboard with no gap (inner
    screen and the cover screen's sheet); back from the conversation's sheet, from Settings and from
    pairing plays Android 16's predictive back animation; on the overlay the back gesture moves the
    sheet down as you swipe, letting go brings it back, completing it dismisses (by §7.7's rule).
38. *Scrolling measured* (§17; §10.4's last row): scroll a long conversation on both screens, and the
    captions in a long call: each scroll logs "conversation: scrolled N frames (compact 3xx dp), K
    over the display's deadline (8.3 ms), the worst … ms" (K should be 0). For the frames in
    detail, a Perfetto trace while scrolling (Developer options → System Tracing, or `adb shell
    perfetto -o /data/misc/perfetto-traces/kai.pftrace -t 10s gfx view sched freq` and `adb pull`
    it), opened in ui.perfetto.dev: Kai's Actual Timeline shows any frame past its deadline. The
    numbers go into §10.4.
39. *The app has proven usable* (§17): a few days of real use on the test device (the held
    feed, replies, calls, the overlay, approvals); the verdict.
40. *A second device* (§17), once P12's release exists: on a LineageOS device against a separate
    lmgw, items 27 to 29, 34 and 35.

**Running WP0 (a) on the spike** (`spikes/wp0/RESULTS.md`, its (a) section; the spike is a separate
app with its own assistant role, so Kai's role goes back to Kai afterwards):
1. `just spike-apk` and `just spike-install`, then open "Kai WP0 spike" → (a).
2. Run "check the setup", then "Request the role" and say whether a dialog showed (if it did, the
   role dialog replaces setup's deep link: `AssistantSetup.SET_KAI`).
3. Set the spike as the digital assistant and, on Samsung, Side button → Press and hold → the spike
   (say whether One UI 8 offers it); on LineageOS, the power button's long press.
4. 10 warm presses folded and 10 unfolded, closing the sheet each time; then "Kill the process" and
   cold presses.
5. In the sheet, both Expand buttons (the order Kai tries them is `EXPAND_PATHS`).
6. The `setDisabledShowContext` toggle off and on, killing the process between (the log shows
   whether `onHandleAssist` came).
7. `just spike-log`, fill (a)'s verdict, then set Kai as the digital assistant again.

The first implementation step is WP0 on the foldable test device, with WP1's scaffold
beside it. Everything under "Decided" is the owner's: eight rulings of 2026-10-14 and the owner's
standing rules that bind every client of lmgw. "Taken in this design" (M1 to M23) is this
document's own. Decisions marked
*shared* are the contract between this design and the calls design: a change to one is a change to
both. The questions are §18 (Q1 to Q11), all answered, with the owner's answers after them; the
lmgw-side prerequisites are §16 (P1 to P12). The appendix maps the device link's list of conflicts
with v3 (its §6.4) to where this version resolves each.

Companions, cited by short name:
- "kai §n", "kai K<n>", "kai ruling <n>": [Kai's design](../../../kai/docs/design/2026-10-06-kai-design.md),
  the Plasma client;
- "api §n": [lmgw's client-apps design](../../../lmgw/docs/design/2026-10-06-client-apps-design.md)
  (device keys, the Chat feed, ongoing folders, device-hosted MCP, approvals);
- "device link Dn, §n": [lmgw's device link design](../../../lmgw/docs/design/2026-10-14-device-link-design.md)
  (v2, approved): mutual TLS from pairing, the device listener, enrolment and its confirmation, and
  the Chat feed as the devices' push channel;
- "calls Cn": lmgw's WebRTC realtime design, `lmgw/docs/design/2026-10-14-realtime-calls-design.md`
  (approved), cited by its decision numbers (C1 to C24);
- "chat-voice §n" (2026-10-03) and "realtime §n" (2026-10-01), in lmgw's `docs/design/`.

Paths are relative to the kai-mobile repository unless they start with `lmgw/` or `kai/`.

Kai mobile is the second Kai client: an Android app that talks to lmgw, voice-first like the
frontier labs' chat apps, settable as the system's digital assistant, continuing the same ongoing
Kai conversation as the desktop, and told by lmgw what happens while it is closed. It holds no
models; every model call goes through lmgw.

## Decided by the owner (2026-10-14)

Rulings:
1. **A native app, no cross-platform UI framework.** Kotlin and Jetpack Compose on Android; SwiftUI
   later for iOS, in this same repository. Rejected, and not to be proposed again: Flutter, React
   Native, Tauri mobile, Compose Multiplatform UI.
2. **The shared layer is Rust:** `lmgw-client` (sans-IO) through UniFFI. Protocol logic lives in
   Rust and is never re-implemented in Kotlin. Audio I/O and OS integration stay native.
3. **Android first.** iOS waits until the Android app has proven usable.
4. **Mobile use goes over WireGuard; voice uses WebRTC** (`/v1/realtime/calls`). This replaces kai
   ruling 19's "audio over WebSocket first; WebRTC only if mobile jitter hurts" and its "WireGuard
   or Tailscale". *Changed after the owner's answers, 2026-10-14:* ruling 8 makes WireGuard routing,
   not security: away from home the tunnel carries the traffic, at home the LAN does.
5. **Kai clients may rely on lmgw-only APIs.** Kai is an extension of lmgw and only ever works with
   lmgw. As long as lmgw stays compatible with OpenAI, it may add as many APIs of its own for Kai
   clients as the cleaner approach needs. (Answered on draft v2.) This design reads it as:
   - a stock OpenAI client sees unchanged behaviour;
   - lmgw's extensions appear only on calls bound to a Chat thread or folder, as `lmgw.*` events,
     or on lmgw's own routes;
   - Kai mobile relies on them freely. "Minimal lmgw-only additions" is not a goal for this client,
     and where the cleaner phone side needs an lmgw event or parameter, the phone asks for it.
6. **Push in the first version** (the answer to Q11). What happens on lmgw
   while the app is closed (a reply, an approval waiting) reaches the phone as a notification in
   the first usable version.
7. **The Chat feed is the push channel** (the follow-up to Q11; device link ruling 5). Kai mobile's
   own foreground service holds the feed over the device link from the first version. No ntfy, no
   UnifiedPush, no Web Push.
8. **The device link** (device link rulings 1, 3 and 4, approved; it answers Q2). lmgw and the
   phone talk over mutual TLS with keys exchanged at pairing: the pairing link pins lmgw's device CA
   through `fp`; the phone generates a non-extractable key in the Android Keystore, enrols its
   public half with a one-time pairing token, and from then on authenticates by its certificate.
   WireGuard is routing, not security: the link carries several URLs (LAN, tunnel), and the phone
   uses whichever answers.

The owner's standing rules, binding on this design:

9. **No hidden limits.** A cap is a visible setting or derived from a real value; an overrun
   surfaces as an explicit error.
10. **A configured alias fallback always applies.** Privacy is choosing an alias without a
    fallback. Kai mobile adds no local-only guards and no content-based exceptions (kai ruling 7).
11. **lmgw's own wire has no backward-compatibility constraint** (alpha). When the cleaner shape
    needs an lmgw change, lmgw changes and the clients move with it. Fidelity to OpenAI's shapes
    still applies (kai ruling 8) to what a stock OpenAI client sees (ruling 5).
12. **Clients talk to lmgw only through its network API** (kai ruling 1, api ruling 1).
13. **A GPU-hold 503 (`code: gpu_hold`) is no outage.** Local models are paused on purpose; cloud
    aliases still answer. Kai mobile never retries it and never treats lmgw as down for it.
14. **lmgw may simply not be running.** It may not be running. "lmgw is not running" is a normal
    state, shown plainly, never an error dialog.

Kai's rulings that carry over unchanged: one ongoing conversation "Kai" (kai ruling 3), every tool
can be set to `allow` (kai ruling 28), personality is an lmgw Chat feature (kai ruling 29). Kai
ruling 31 (Kai is not active on a locked screen) is the desktop's; the phone's lock screen is Q7,
answered: later, on a temporary thread.

## Taken in this design

**M1. Repository layout and identity.**

```
kai-mobile/
├── android/                Gradle project (Kotlin DSL), one module `app` to start
│   ├── settings.gradle.kts, build.gradle.kts, gradle/libs.versions.toml, gradlew
│   └── app/src/main/kotlin/dev/rootshell/kai/…
├── core/                   Cargo workspace (its own Cargo.lock)
│   ├── Cargo.toml          [workspace]; lmgw-client and lmgw-api-types by path
│   ├── .cargo/config.toml  Android target flags only (§2.1)
│   ├── kai-mobile-core/    the UniFFI crate: lib + cdylib, a Kai-shaped API (§3)
│   └── uniffi-bindgen/     the bindgen binary at the workspace's uniffi version
├── ios/                    later (ruling 3)
├── spikes/wp0/             WP0's trial code and RESULTS.md
├── ci/Containerfile        the Android build image (§15.1)
├── docs/design/
├── justfile
├── LMGW_PIN
├── LICENSE.md              MIT
└── README.md
```

- **Application id `dev.rootshell.kai`**, Kai's desktop app id (kai §8.3): one product, one id on
  every platform, and the iOS bundle id later. An Android application id cannot change after the
  first install without a reinstall and a new pairing, so the final one is chosen now. Debug builds
  add `applicationIdSuffix = ".dev"` and install beside a release. The Kotlin namespace is
  `dev.rootshell.kai`; the generated bindings live in `dev.rootshell.kai.core.ffi`.
- **`LMGW_PIN`** in Kai's three-line form (`lmgw_commit`, `lmgw_tag`, `lmgw_tree`; kai K3's notes).
  `core/Cargo.toml` names `../../lmgw/crates/lmgw-client` (lmgw's checkout beside kai-mobile's).
  `just check` warns, never fails, when that checkout is at another commit or has changes; CI clones
  lmgw at the pin.
- **MIT**, as Kai (FSL-1.1-MIT until 2026-10-18). Publishing to GitHub follows Kai's squashed-publish
  practice.
- *Why:* rulings 1 and 2 give two native trees around one Rust workspace. The Gradle project stays
  a plain Android project that Android tooling opens without knowing Rust exists.

**M2. The Rust core owns lmgw's protocol and its I/O; Kotlin owns the platform.**
- **The core** (`kai-mobile-core`, on tokio, with reqwest as Kai's feed uses it, and rustls) makes
  every request to lmgw: enrolment, the feed (SSE), folders, `current`, the Kai thread, the send
  stream, approvals, the call's SDP POST and a stale call's hangup, and later the `/mcp/host`
  WebSocket. It wraps `lmgw-client`'s rules: the pairing link and enrolment, request builders and
  readers, `FeedReader` with its open approval set, `realtime::parse` with the reassembly of
  `lmgw.event.part`, `voice::Machine`, `key_refused`. It also holds the Kai-level rules: the URL
  race, the folder lookup, the conversation's thread, the voice session's lifecycle, approvals,
  which feed events notify, settings and Markdown parsing.
- **Kotlin** holds the UI, libwebrtc and the audio devices, the Keystore and its signatures, the
  camera, notifications, the foreground service, alarms, the VoiceInteractionService, network
  callbacks and intents.
- **The API Kotlin sees is app-shaped** (pairing, connection state, the Kai thread, messages, a
  voice controller, approvals, notifications, settings; §3.2), not a 1:1 mirror of `lmgw-client`.
  No `serde_json::Value` crosses UniFFI: what `lmgw-client` keeps as JSON becomes a typed record
  or, where a payload is passed on whole, JSON text.
- **TLS from one constructor.** Every connection to lmgw is mutual TLS on lmgw's device listener
  (ruling 8, M7), and every lmgw-facing client in the core (reqwest for HTTP and SSE now,
  tokio-tungstenite for `/mcp/host` later) is built by one constructor from the one pinned
  `rustls::ClientConfig`; a test holds the core to it (device link D15). Kotlin opens no connection
  to lmgw. The manifest keeps cleartext off. Android's network security config has no say over the
  core's sockets and needs none: no trust decision is the platform's, the pin is lmgw's CA.
  *Changed after the owner's answers, 2026-10-14:* v3 spoke `http` and guarded a bearer key with
  a tunnel rule.
  *(changed 2026-10-15: WP2's security review: `clippy.toml` refuses the `reqwest::ClientBuilder`
  type outside the one constructor, which covers `ClientBuilder::default()`. clippy names no trait
  impl's method ("does not refer to a reachable function"), so `reqwest::Client::default()` cannot
  be listed there; a test of the core refuses it in the sources instead.)*
- *Why:* ruling 2. The desktop's hard-won reading of lmgw (kai §3.2's waits, §3.3's revocation
  kinds, §4's folder rules) is Rust already. A small, app-shaped surface keeps UniFFI's generated
  code small and the Kotlin side simple, and iOS gets the same core unchanged.

**M3. WebRTC lives in Kotlin; signalling and the events channel go through the core.** Kotlin
creates the PeerConnection (LiveKit's `io.github.webrtc-sdk:android` build, standalone through the
`org.webrtc` package), the microphone track and the `oai-events` channel, and hands the offer to
the core, which POSTs it and returns lmgw's answer. Every channel message passes through the core
(`Voice::channel_message` in, the `EventSink` callback out). The core decides when the microphone
captures, sends silence or stops, and when Kai's audio is muted locally; Kotlin applies it, and
reads libwebrtc's stats when the core asks for them (§7.6).
- *Why:* libwebrtc owns capture, playback, echo cancelling and the jitter buffer, and its Android API
  is Java (ruling 2: audio native). The events are protocol (ruling 2: Rust), so one parser serves
  Kai's WebSocket and the phone's channel. Audio never crosses the FFI.
- *Rejected:* a Rust WebRTC stack with PCM over the FFI (Android audio, echo cancelling and jitter
  buffering in our own code, no hardware canceller); parsing events in Kotlin (ruling 2).

**M4. UniFFI with JNA, built by cargo-ndk from a just recipe; Gobley not used.** §2.2, §3.1.
- uniffi 0.32.x proc-macros, library-mode bindgen; JNA at 5.16.0 or newer (16 KB pages).
  `just core-android` builds and generates; Gradle's `preBuild` calls it.
- From 0.32's Kotlin breakages: no async primary constructors, no `[ByRef] bytes`. Async functions
  run on tokio (`async_runtime = "tokio"`) and become `suspend` functions. `panic = "unwind"` in
  every profile, so a panic becomes a Kotlin exception instead of killing the app.
- **Synchronous entry points only enqueue.** `channel_message` and its kind run on the caller's
  thread, libwebrtc's signalling thread for the channel's callbacks; they queue a message and
  return, never taking a lock.
- State is pulled (`Core::next_update()` into a `Flow`); the callback interfaces are `EventSink`
  (the call, M3) and `Keystore` (the device key's signatures, M7), called from tokio's threads
  through JNA, whose attach cost WP0 (e) measures.
- *Why:* cargo-ndk plus `uniffi-bindgen` is the documented path. Gobley's value is Kotlin
  Multiplatform, which ruling 1 rules out.

**M5. WireGuard is routing and the device link protects, so every network may carry lmgw's
traffic.** §5.1, §5.2. *Changed after the owner's answers, 2026-10-14:* v3's tunnel gate, the
tunnel's identification, the process binding, the VPN-only `networkIgnoreMask` and the debug switch
`dev.allow_untunnelled` are gone (ruling 8; device link §6.4 items 4 and 9).
- **Several URLs, whichever answers.** The pairing link carries lmgw's `https` URLs in the owner's
  order (a LAN address, a tunnel address), and enrolment and the feed's `hello` refresh them. The
  core races them in `lmgw-client`'s order (the URL that last connected first, then the link's
  order), starting the next one `lmgw.url_stagger_ms` (default 250, RFC 8305's connection attempt
  delay) after the previous one if no TCP answer has come. The first handshake that verifies wins.
  No hairpinning and no gate: a URL that lands on a stranger's host (a foreign Wi-Fi with the same
  subnet) fails lmgw's verification before the phone sends its certificate, and the phone holds no
  secret it could send.
- **The core's sockets follow Android's default routing.** No `bindProcessToNetwork`: Android sends
  each URL where its routes say (the tunnel's address into the VPN, a LAN address onto Wi-Fi, a full
  tunnel's exclusions as its profile has them). On a change of the default network the core drops
  its pooled connections and races again (§5.2).
- **libwebrtc gathers on every network.** `networkIgnoreMask` is left at its default, and gathering
  is continual. lmgw answers one candidate, the local address the POST arrived on (calls C6): a
  call POSTed to the LAN URL gets lmgw's LAN address, one POSTed to the tunnel URL its tunnel
  address, and the phone needs a local candidate on whichever network reaches it. libwebrtc binds
  each ICE socket to its own network (`android_setsocknetwork`), so a Wi-Fi socket sends to the
  tunnel address outside the VPN's routes and only the VPN socket reaches it; on a LAN-URL call the
  Wi-Fi socket is the one. ICE's checks find that pair. The others fail after a few STUN requests,
  which carry nothing but the call's integrity-protected ICE credentials.
- *Rejected:* the VPN-only mask (it breaks every LAN-URL call); a mask that drops the VPN (it
  breaks every tunnel-URL call); a mask chosen from the network that carried the POST (Android names
  that network only through a socket binding, which the core does not set, and the mask would
  forbid the gathering that carries a call across Wi-Fi and mobile data, §7.11);
  `bindProcessToNetwork` (it pins every socket of the process to one network, so the LAN URL fails
  while the tunnel is bound and a lost network keeps sockets failing, and libwebrtc's own binding
  overrides it for the media anyway).
- *Why it is safe:* lmgw's pinned verification and the client certificate protect the HTTP side on
  any network, and the media is DTLS-SRTP whose fingerprints travelled inside that mutual TLS
  (device link §8). Which network carries a byte is routing.
- **The emulator** pairs with a dev lmgw through a URL `https://10.0.2.2:<device port>` added to the
  device's row, whose host then appears in every server certificate (device link D3). Calls do not
  work there (the POST arrives on lmgw's loopback, so the candidate is one the emulator cannot
  reach), and the emulator proves nothing about audio anyway. No debug switch is needed.

**M6. Pairing: scan, tap or paste the link v2; the parser in `lmgw-client`; the key made in the
Keystore and enrolled.** §4. *Changed after the owner's answers, 2026-10-14:* the link is v2 and
carries a one-time token instead of a key (ruling 8; device link §6.4 items 6 and 7).
- The link is `lmgw-pair:?v=2&name=…&url=…[&url=…]&fp=sha256:<CA SPKI hash>&token=lmgw-enrol-…`
  (device link D6). Scanning uses CameraX with ZXing's `core` (Apache-2.0, pure Java, no network,
  no Play services); a tapped `lmgw-pair:` link and a pasted one lead to the same confirmation. The
  parser is `lmgw_client::pairing` (P4).
- The phone generates a non-extractable P-256 key in the Android Keystore (TEE), enrols its public
  half with the token (`POST /devices/enrol`), shows the confirmation code, and waits until the
  owner confirms the code on lmgw's Devices card (device link D23). Then it opens its first mTLS
  feed.
- **What the core stores is public** (§3.3): the certificate, lmgw's CA certificate, the dates, the
  code, the URLs and the feed's cursor. v3's AES-GCM bearer file (`credentials.bin`) is gone, and
  so is its decrypt at process start.
- *Rejected scanners:* ML Kit's bundled one (closed source, and its terms let it send usage metrics
  to Google while the frame it reads carries a pairing token) and Google's code scanner (needs Play
  services and runs Google's UI on the frame).

**M7. TLS from the first version: lmgw's CA pinned by `fp`, the phone's key in the Keystore, rustls
with ring in the core.** §4.3, §4.4. *Changed after the owner's answers, 2026-10-14:* v3 had no
TLS and refused every link with `fp` (device link §6.4 item 5).
- **The provider is ring**, so the core never builds aws-lc-sys under the NDK; `lmgw-client`'s
  `tls` feature takes the provider from the app (device link D15).
- **lmgw is verified against the pinned CA.** At enrolment `lmgw-client`'s `EnrolVerifier { fp }`
  finds, among the certificates lmgw sent, the one whose SubjectPublicKeyInfo hashes to `fp`, and
  verifies the chain against it alone. The core stores the answer's `ca_certificate` only if its
  SubjectPublicKeyInfo hashes to `fp`. From then on rustls's stock `WebPkiServerVerifier` runs with
  that CA as the only root: chain, dates, `serverAuth`, and the URL's host against the leaf's names,
  IP addresses included (lmgw puts every URL host into every leaf, device link D3).
- **The phone authenticates by its certificate.** The core implements rustls's `SigningKey`
  (`ECDSA_NISTP256_SHA256`, the stored SubjectPublicKeyInfo) with a `Signer` whose `sign` calls the
  `Keystore` callback into Kotlin. Kotlin signs with `Signature("SHA256withECDSA")` and returns the
  DER signature, which is what TLS 1.3 wants for that scheme. `Signer::sign` is synchronous, so a
  handshake blocks one tokio worker for one Keystore signature. That is acceptable: one signature
  per new connection, the pool keeps connections, the runtime has other workers. WP0 (e) measures
  it.
- **URLs stay `https`.** A refreshed list (`hello.urls`) keeps only its `https` URLs.
- **One constructor** builds every lmgw-facing client from the one configuration (M2).
- *Rejected:* aws-lc-rs (aws-lc-sys's C build under the NDK, for nothing ring lacks here);
  `rustls-platform-verifier` (the system's trust store has no say: the pin is lmgw's own CA); a
  software key in app storage (copyable, where the Keystore key is not); StrongBox (slow, and a
  signature sits on every new connection's path, a call's included).

**M8. Connectivity states, failing fast.** Each state has its words and one action (§5.3): not
paired, waiting for confirmation (with the code), pairing refused (the code used by someone else,
the link expired or unknown, not the lmgw the link pairs with), certificate lapsed, key refused
(revoked, disabled, expired), lmgw not running, the computer not answering, connected. The GPU hold
(with the fallback that answered, kai K9) and a certificate's coming lapse are badges on any of
them, not states. A connect is bounded by `lmgw.timeout_s`, default 10 s: twice WireGuard's 5 s
`REKEY_TIMEOUT`, so a handshake's first retry fits when the tunnel URL is the one that answers, and
Kai's value; the message names the setting. No spinner runs longer than a stated bound. *Changed
after the owner's answers, 2026-10-14:* "WireGuard off" and "another VPN" are no states any more
(device link §6.4 item 8).

**M9. The Kai conversation is the desktop's.** The phone finds the same folder by the same rule
(kai §4's five steps, moved into `lmgw-client`: P5), asks `current` at process start, whenever the
app becomes visible and at every relevant feed event, binds a call by the folder so that lmgw picks its current thread (P8), offers New
conversation, and states the owner's steps when the folder is not ready. It has no "Open in
lmgw": lmgw's dashboard needs the owner's login, which the phone does not hold; where the owner
must act on lmgw, the phone says the steps in the dashboard's words.

**M10. No message cache in the first usable version; the pairing and the feed's cursor persist.**
The core keeps the pairing's public facts with the feed's cursor (§3.3), and its settings, in JSON
files in app-private storage. At process start it resumes the feed from the persisted cursor
(`Last-Event-ID`), so `caught_up` and `hello.open_approvals` (§5.4) tell it what happened while it
was gone (the open approvals as a set to reconcile against, not a replay), and it re-reads `current` and the Kai thread's messages; while the service keeps the
process alive, the feed keeps them current. A local message cache (an instant start without the
re-read) is Later, measured against WP2's start time. *Changed after the owner's answers,
2026-10-14:* v3 started the feed at "now" on every start; a feed held in the background needs the
cursor (device link §6.4 item 3).

**M11. A voice session starts only by the owner's act and never takes the voice over unasked.**
*Shared.* §7.
- **The bind.** The phone POSTs with `?chat_folder=<id>&takeover=ask` (`&new=true` after New
  conversation); `takeover=ask` exists on calls only (calls C1, C21). lmgw binds the folder's
  current thread in the same step and names it in `session.lmgw.resolved.chat_thread`. When
  another client holds that thread, the call still connects and lmgw sends
  `lmgw.session.bind_pending {held_by}` instead of `session.created`. The phone shows "Voice is on
  <held_by>" with **Take over**. Only that tap sends `lmgw.session.take_over`, and
  `session.created` follows; if the holder ends first, `session.created` arrives by itself.
  Declining hangs up. A pending call binds nothing, so the desktop's voice moves only on the tap
  (§7.9). While the bind is pending the phone sends nothing but `lmgw.session.take_over`, or hangs
  up: lmgw answers any other client event with an `error` of code `bind_pending` and drops it.
- A call ends at End, at `voice.idle_end_s` of quiet (default 30, 0 = never), when the overlay is
  dismissed before the first turn, when a pending bind is declined or left unanswered for one of
  lmgw's `realtime.ping_interval_s`, at a takeover, a revocation, a drop, lmgw stopping, or lmgw's device access changing (the device listener turned off or its scope or port moved: `shutting_down` too, shown as "lmgw's device access changed" with Reconnect, §7.8). *Changed 2026-10-14 (consistency pass):*
- **Stopping Kai** mutes Kai's audio on the phone at once (instant silence) and sends
  `lmgw.response.stop {playout_ntp_ms}`, the time the phone was playing at the mute. The event is
  idempotent, cancels whatever is open (thinking, speaking, a tool) and never answers
  `response_cancel_not_active`, so the phone sends it without asking `Machine::current()` first.
  lmgw's barge-in verdict, `output_audio_buffer.cleared` (calls C12), mutes the same way. The mute
  lifts at the next `output_audio_buffer.started`.
- **End** sends `lmgw.response.stop`, then closes `oai-events`. lmgw applies every event it
  received before the close and then ends the call, so the stop and the thread's heard part travel
  on the one channel and race nothing. No wait for `response.done`, and no hangup over HTTP.
- **Why a call ended** is the call's last event, `lmgw.session.closed {kind, by, message}`, read
  through `lmgw-client` (§7.8). The phone reads its end reason from that event alone; a call that
  dies without one shows "connection lost".
- No automatic reconnect (kai K5). Reconnect first hangs up the stale call id over HTTP (the one use
  of the hangup route), then binds again with `takeover=ask`. K16's silence while another
  principal's turn runs is adopted.

**M12. Audio: libwebrtc's device module, the hardware canceller by default, a MediaSession for
headset buttons, the microphone type on Kai's foreground service during a call, no Telecom in
v1.** §8.
- `JavaAudioDeviceModule` with the hardware AEC and NS where reported, a setting for WebRTC's
  software canceller or half duplex; `MODE_IN_COMMUNICATION`, `setCommunicationDevice`, audio
  focus. At a call's start Kai's foreground service (M16) adds the `microphone` type, and its one
  ongoing notification shows the call (Mute, End) until the end, when the type is dropped again.
- An active `MediaSession` during a call takes the headset's buttons (mute, push-to-talk, End).
  Without it, play/pause goes to the last media app, whose playback takes the audio focus and ends
  the call.
- *Why no Telecom:* Core-Telecom is for call-like VoIP (a `phoneCall` service, `MANAGE_OWN_CALLS`,
  a CallStyle notification within 5 s) and forbids `setCommunicationDevice` beside it. An assistant
  conversation is not a phone call; car integration is Later.

**M13. Assistant mode through a VoiceInteractionService.** §9. A session overlay (a bottom sheet
with the ring and captions, on both screens of a foldable, M23) that starts a call at once and
expands into the app; the WebRTC factory and the device module prepared in `onReady` and
`onPrepareShow` without opening the microphone; the microphone type added to Kai's service from
the session (the assistant's while-in-use exemption), so the conversation survives screen-off and
leaving the overlay; a stub RecognitionService (AOSP rejects the service without one); the `ASSIST`
filter; setup that deep-links to the digital-assistant setting and states the side button's steps.
No lock-screen use in the first usable version (Q7: later, on a temporary thread).

**M14. Compose and Material 3, one activity, no DI framework; Markdown parsed in the core.** §10.
- Navigation Compose in a single activity. Plain constructor wiring and a few singletons instead of
  Hilt: the object graph is small, and every annotation processor costs build time.
- Markdown: the core parses with pulldown-cmark into a small block-and-inline tree, and Compose
  renders it with composables of our own. iOS reuses the parse, the look is the platform's, and no
  third-party Compose Markdown library sets it. Tables show as their source text until Later.

**M15. minSdk 31; targetSdk the newest stable API level at WP1** (recorded in WP1's notes; 36 or
later).
- *Changed 2026-10-14 (WP1):* **targetSdk and compileSdk 37** (Android 17, platform `android-37.0`, on the
  SDK manager's stable channel; 37.1 and 37.2 are minor SDK releases of the same API level). Built with
  AGP 9.4.1, Kotlin 2.4.21, Gradle 9.8.1, Compose BOM 2026.09.00, NDK 29.0.14206865 (r29), build tools
  37.0.0, JNA 5.19.1, uniffi 0.32.2. The image is Fedora 43 because Fedora 44 ships only JDK 25.
- *Why 31:* `setCommunicationDevice` and its listener (API 31) are the routing path (M12). The
  primary test device runs Android 16; a second test device's version is recorded in WP0, and 31
  stands unless it is older.
- From targetSdk 36 Android ignores orientation and resizability restrictions on screens 600 dp and
  wider, so the app is fully resizable from the start (M23).

**M16. Kai's foreground service holds the feed for as long as the phone is paired.** Rulings 6 and
7; §12. *Changed after the owner's answers, 2026-10-14:* v3 ran the feed only while the app was
visible or a call ran, with no push (device link §6.4 items 1 and 2).
- **One service, `KaiService`, of type `specialUse`**, started once a pairing is confirmed, at boot
  and after an update, and `START_STICKY`. Not `dataSync`: for an app targeting Android 15 or later
  the system lets `dataSync` services run 6 hours in 24 and then calls `onTimeout`, after which the
  service must stop (Android's foreground-service timeout page and the Android 15 behaviour
  changes, checked 2026-10-14). `specialUse` has no timeout; it needs
  `FOREGROUND_SERVICE_SPECIAL_USE` and a `PROPERTY_SPECIAL_USE_FGS_SUBTYPE` text, which only Google
  Play reviews (Kai mobile is not on Play), and it is not among the types a `BOOT_COMPLETED`
  receiver may not start.
- **Setup makes it survive** (§9.5): the battery-optimisation exemption, and on Samsung the "Never
  sleeping apps" list.
- **The feed it holds** asks a long keep-alive (`?keepalive_s=`, the visible setting
  `feed.keepalive_s`, default 120, the value the device link suggests for a phone) and persists its
  cursor, so `caught_up` and the resume work after the process dies (M10).
- **Notifications are built from events alone:** `approval.requested` and `message.*` carry
  `thread_title`, and the open approvals come from `hello.open_approvals` and the `state` frames' set
  (device link D17, D18, §7.3).
- **A push never starts voice.** The microphone type is while-in-use, so the notification's tap
  brings the app forward, and the app starts the call (M11).
- **A call runs in the same service:** its start adds the `microphone` type, its end drops it, and
  the one ongoing notification shows the link's state or the call. *Why one service:* two would show
  two ongoing notifications for one app. If WP0 (b) finds that the type cannot be added from the
  assistant session, the call gets a service of its own, as in v3.
- A call does not depend on the feed: its end reason comes on the call (M11), and a call starts
  without waiting for `hello`. The feed carries what the call does not: another principal's turn
  in the thread (K16, §7.5), the folder's rollover or a New conversation from another client
  (§7.7), who holds the voice when a call starts (§7.9), the hold badge, and the conversation's
  messages.
- **WP0 (m) measures** the battery drain per day of the held feed on the test device, with lmgw up and
  with lmgw stopped for several hours.

**M17. No Google Play services and no third-party telemetry.** No ML Kit, no Firebase, no crash or
analytics SDK. Logs go to logcat and stay on the phone; the owner shares them by hand when wanted
(§3.5).
- *Why:* the owner's data stays on the owner's machines. The app also runs on a phone without
  Google's services; a LineageOS test device is that check (WP0 (o)).

**M18. Build, test, install.** §15. Gradle from the command line with the SDK and NDK in a Podman
image (CI on the build runner, and locally); cargo-ndk for `arm64-v8a`, plus `x86_64` in debug
builds for the emulator; Rust tests on the host, Kotlin JVM tests, instrumented tests on the phone
over wireless adb, audio only on real hardware. The first usable version goes onto the test
phone with `just install` over adb. CI's build job keeps the signed release APK as an installable
artifact, which is how a tester without a build setup gets it (Q1); Obtainium follows the project's releases
once the app has proven usable (Q10).

**M19. Phone tools are a later phase with their own design; v1 keeps the seams.** §13. The core will
host an MCP server over `/mcp/host` in Rust (as Kai's `src/host/` does, kai PD2), authenticated by
the device's certificate like every other connection, and Kotlin implements the tools behind a
callback interface. No Android MCP SDK and no custom Android transport are needed.

**M20. Approvals by tap in v1.** §11. A card in the overlay, the voice screen and the conversation;
a notification whenever the app is not visible, call or no call (ruling 6), whose Approve needs an
unlocked phone. Voice approvals with Kai's phrase matcher are Later.

**M21. Time to listening is the core of the experience, and a go/no-go.** *Shared.* WP0 (c)
measures invocation to listening against a stated target (§7.2). Listening starts when DTLS
connects: lmgw keeps the call's RTP from the first frame on the session's timeline when the bind
succeeds directly, so speech from then on reaches the turn. Everything that can happen before the
invocation does: the core and its pairing at process start (the held service keeps the process
alive), the WebRTC factory and device module in `onReady`, and from `onPrepareShow` a warm
connection to lmgw, so that the TLS handshake and its Keystore signature run while the offer is
built (§7.2). At the invocation nothing waits for a lookup: the POST names the Kai folder and lmgw
binds its current thread (P8). Whether the phone should also capture before the PeerConnection
exists and send it as `lmgw.input_audio.preroll` is Q5, decided after WP0 on its merits.

**M22. The first usable version is the Kai conversation, voice, assistant mode and push.** Deferred
until the app has proven usable: the message cache (M10), the threads browser and search, Markdown
tables, the Baseline Profile module, publishing for Obtainium (Q10), the lock screen (Q7), silent
certificate renewal (lmgw's device link WP8), and the tabletop posture (M23). Each is in Later with
the reason it waits.

**M23. Foldable screens: adaptive layouts by window size class, fully resizable.** §10.6.
*Added after the owner's answers, 2026-10-14* (Q1).
- The cover screen (about 904×2316 px) is narrower than 600 dp; the inner screen (1812×2176 px) is
  600 dp or wider. WP0 (n) reads the real dp from `WindowMetrics` on both.
- Layouts follow the window size class (`currentWindowAdaptiveInfo()`), never the device model:
  compact on the cover screen; on the inner screen the conversation and voice screens use the width
  (§10.6).
- Fully resizable, no orientation lock: from targetSdk 36 Android ignores both on screens 600 dp
  and wider, and the temporary opt-out property is gone at targetSdk 37.
- Folding or unfolding mid-call recreates the activity. The call lives in the service and the
  `VoiceController` (§9.3), and the screens draw from the core's state, so nothing reconnects and
  nothing is lost; WP0 (n) and WP3 test it.
- The assistant overlay works on both screens (WP0 (a), WP4).
- The tabletop posture (Jetpack WindowManager's `FoldingFeature`) is Later.

## What exists today (relied on, not re-derived)

**lmgw** (main at the pin; api §n as built):
- **Device keys** (api §1): scope, budget, tool scope, `concurrency_limit`, `self_admin` level per
  device, rotate, disable, expiry, delete. Revocation reaches a client as the feed's `revoked`, a
  401 with its code (`device_key_unknown`, `device_disabled`, `key_expired`) or a 4003 close whose
  reason starts with the kind's token. The feed holds no concurrency slot; a Chat turn a device
  starts holds one, and so does a realtime session (api §1.2, §1.3). Today a device presents a
  bearer key; the device link (below) replaces it with a certificate and keeps the row's policy.
- **The pairing link** `lmgw-pair:?v=1&url=…&name=…&key=lmgw-device-…`, with `fp=sha256:` reserved
  (api §1.4). The Devices card shows it once, with Open and Copy; its `url` is editable for a
  tunnel's address. The device link replaces it with v2 (below).
- **The Chat feed** `GET /chat/api/feed` (api §2): SSE, `hello` first (`keepalive_s`,
  `retention_days`, `principal`, `hold`, live `voice` and `turns`), `Last-Event-ID` or `?since=`,
  paged catch-up, `resync`, `thread.*`, `folder.*`, `folder.current`, `turn.*`, `voice.bound`,
  `voice.ended {thread_id, by, reason, taken_over_by}`, `hold`, `state`, `revoked`, `approval.*`.
  Its keep-alive is one global setting (`chat_feed_keepalive_s`, 15 s).
- **Ongoing folders** (api §3): `POST /chat/api/folders/{id}/current` with `{new: true}`,
  `folder_no_model`, `not_ongoing`, the idle rollover, `folder.current`.
- **The Chat routes** under `/chat/api/`: threads, `send` (an SSE stream of `turn`, `retrieval`,
  `delta`, `usage`, `tool`, `error`, `done`), approvals, transcribe, search, attachments; a text
  send cancels the thread's live turn.
- **Device-hosted MCP** `GET /mcp/host` (api §5) and **approvals** (api §6): `require_approval` on
  thread tools, `approval.requested` and `approval.decided` in the feed, `mcp_approval_request` and
  `mcp_approval_response` in a bound realtime session, `POST /chat/api/threads/{id}/approvals`. The
  first decision wins; a new user message declines every pending call.
- **The realtime WebSocket** `/v1/realtime?chat_thread=&takeover=` (chat-voice §8): the thread owns
  model, prompt, tools, ASR, TTS and voice; the client owns turn detection, `half_duplex`,
  `echo_tail_ms` and the buffer and response events. A thread whose resolved turn detection is
  push-to-talk starts with `turn_detection: null`. A negative `chat_thread` binds a temporary
  thread. `input_image` content is refused (realtime §19).
- **The GPU hold**: 503 `gpu_hold`; a fallback that answered is named by `x-lmgw-fallback` and by
  `answered_by` in `lmgw.response.timing`.

**lmgw, being designed: calls** (calls, approved, lmgw 2328902a, with the device link's changes, device
link §8). A stock OpenAI client sees OpenAI's behaviour; everything `lmgw.*` below applies to calls
bound to a Chat thread or folder (ruling 5), and calls C24 lists it in one table:
- `POST /v1/realtime/calls`, raw `application/sdp` or multipart with `sdp` and an optional
  `session` part, which lmgw applies as the session's first update right after `session.created`;
  the query as on the WebSocket (`chat_thread`, `chat_folder` with `new`, `takeover`, `model`);
  `201` with the answer and `Location: /v1/realtime/calls/{id}`; `POST …/hangup`, which takes no
  concurrency slot. A call holds one slot for its life (C1, C3).
- **`chat_folder=<id>`** (with `new=true` for a new conversation): lmgw binds the folder's current
  thread in one step, rolling it over as `current` does, and reports it in
  `session.lmgw.resolved.chat_thread`. Its refusals are 404 `chat_folder_not_found`, 409
  `not_ongoing` and 409 `folder_no_model`. `session.lmgw.turn_mode` is `"automatic"` or
  `"push_to_talk"`; for `automatic` lmgw itself applies the thread's turn detection, or
  `semantic_vad` (C23).
- **`takeover=ask`, on calls only** (the WebSocket refuses it, C21): when another client holds the
  thread as `oai-events` opens, the call still connects and lmgw sends
  `lmgw.session.bind_pending {held_by}` ("device 'desktop'", or "the dashboard" for the owner)
  instead of `session.created`, and discards inbound audio. Every other client event while pending
  is answered with an `error` of code `bind_pending` and dropped. `lmgw.session.take_over` binds
  with a takeover, and `session.created` follows; when the holder ends meanwhile, `session.created`
  arrives by itself. The wait lasts one `realtime.ping_interval_s`, after which the call ends with
  `lmgw.session.closed {kind: "thread_bound"}`. What the other `takeover` values do is the calls
  design's; Kai does not use them.
- **One host candidate, the local address the POST arrived on** (C6), which on the device listener
  is a LAN or a tunnel address, whichever URL answered (device link §8); no STUN or TURN; the device
  socket group (calls C4, §3.1, WP7): one socket per device-listener address, following it live. A reverse proxy in front of
  lmgw is not covered (lmgw's Later list). Opus with in-band FEC. The SDP, and with it the DTLS
  fingerprints, travels inside the device link's mutual TLS.
- **RTP from the first frame**, before `oai-events` opens and before the bind, is kept on the
  session's timeline when the bind succeeds directly (C13).
- The `oai-events` channel carries the WebSocket's events except the audio deltas;
  `input_audio_buffer.append` is refused on a call. `output_audio_buffer.started`, `.stopped` and
  `.cleared`.
- **`lmgw.response.stop {playout_ntp_ms}`**: idempotent, cancels whatever is open (thinking,
  speaking, a tool), never answers `response_cancel_not_active`. `playout_ntp_ms` is the phone's
  libwebrtc `estimatedPlayoutTimestamp` at the stop; without it lmgw takes heard = sent (C11, C12).
- **The barge-in verdict is `output_audio_buffer.cleared`** (C12), not a new event: lmgw sends it
  only once it has stopped sending a response's audio, after its gate, never on a `speech_started`
  alone. The cancelled `response.done` just before it says why (`status_details.reason`
  `turn_detected` for a barge-in, `client_cancelled` for a stop).
- **`lmgw.output_audio.playout {playout_ntp_ms, jitter_buffer_ms}`** from the phone while Kai's
  audio plays; lmgw uses it for drained and for the barge-in window. When none has arrived for one
  `realtime.ping_interval_s` while audio played, lmgw considers the reports stopped and falls back
  to its formula (C10).
- A commit or clear waits `webrtc_commit_window_ms` (default 120; lmgw's WP6 measures the value) and
  covers the frames stamped up to it plus the window, so a push-to-talk commit cannot overtake its
  turn's last frames (C8).
- **`lmgw.event.part {id, index, count, data}`**: an event larger than the peer's
  `max-message-size` travels as parts, in both directions, and `lmgw-client` reassembles them
  before parsing. `event_too_large` with emptied fields remains only for unbound calls, which Kai
  never makes (C14).
- `lmgw.output_audio.starved {response_id, starved}`: `true` at the first padded tick while a tool
  runs, `false` at the next real frame (C22).
- A call is dead after one `realtime.ping_interval_s` with nothing from the phone (C16).
- **`lmgw.session.closed {kind, by, message}`** is the last event before lmgw ends a bound call;
  `kind` is `taken_over`, `revoked`, `out_of_reach`, `closed`, `shutting_down` or `thread_bound`.
  `lmgw-client` maps them onto `CloseKind`, which gains `ThreadBound`; `closed` and a kind it does
  not know map onto `Other` (C17).
- **End by the client:** it closes `oai-events`; lmgw applies every event received before the
  close, then ends the call (C16).

**lmgw, being designed: the device link** (device link v2, approved, lmgw 2328902a):
- **A device listener of its own** (D10, D24): TLS 1.3 with client certificates, on the HTTP port's
  number plus one (8788 by default, written into the settings at its first start), loopback only
  until the first remote device, then every address after the owner's confirmation; off until the
  first pairing. It serves every `Inference` and `Chat` route and `POST /devices/enrol`, nothing
  else. The plain listener keeps serving local OpenAI clients and the dashboard and authenticates
  no device: bearer device keys end (D12).
- **lmgw's device CA** (P-256, its key wrapped in the Secret Service) signs the server
  certificates and every device certificate. The link's `fp` is `sha256:` and the hex SHA-256 of
  the CA's SubjectPublicKeyInfo. Every server certificate names every host of every device's URL
  list, IP addresses included (D1 to D3).
- **The pairing link v2** `lmgw-pair:?v=2&name=…&url=…[&url=…]&fp=sha256:…&token=lmgw-enrol-…`:
  `url` repeats, `https` only, in the owner's order; the token is single-use and lives
  `devices.token_minutes` (10) (D6, D8).
- **`POST /devices/enrol`** `{token, public_key, client}`, without a client certificate, answers 201
  `{device, certificate, ca_certificate, not_after, notice_after, urls, confirm_code, confirmed}`
  with `Connection: close`. The same key presenting a used token again gets the same answer. Its
  refusals: 400 `bad_request`, 400 `public_key_unsupported`, 401 `pairing_token_unknown`, 401
  `device_disabled`, 409 `public_key_in_use`, 410 `pairing_token_used` (another key: the row is
  marked contested), 410 `pairing_token_expired`, 413 `body_too_large`, 429 `pairing_rate_limited`
  with `Retry-After`, 503 `devices_ca_unavailable` (D7, §3.4).
- **The confirmation** (D23): the code is the first 32 bits of the SHA-256 of the enrolling key's
  SubjectPublicKeyInfo, shown as `7F3A-91C2` on both ends. Every request answers 403
  `device_unconfirmed`, naming the code, until the owner clicks Confirm on the card.
- **The principal, per request** (D5, D11): a bearer, a cookie or an API key on the device listener
  is 400 `credential_on_device_listener`; no client certificate is 401
  `client_certificate_required`; a certificate past its `notAfter` is 401
  `device_certificate_expired`; otherwise the row decides (`device_unconfirmed`, `device_disabled`,
  `device_key_unknown`). Long-lived links end at the certificate's date with the kind
  `certificate_expired`, on the feed's `revoked`, a 4003's reason, and a call's
  `lmgw.session.closed {kind: "revoked"}`.
- **Certificates** last `devices.client_cert_days` (365), with a notice from two thirds
  (`notice_after`). Re-pairing (a new token for the same row) renews; silent renewal over mutual
  TLS is lmgw's WP8, after the first usable version (D4).
- **The feed as push channel** (D16 to D18): `?keepalive_s=` per stream (up to
  `chat_feed_keepalive_max_s`, default 900) with `TCP_USER_TIMEOUT` set from it;
  `hello.open_approvals` and a live `caught_up {cursor}`, and the same set on every `state`;
  `hello.urls` and `hello.cert`; `thread_title` on `approval.requested` and on `message.added` and
  `message.updated`. *Changed 2026-10-15:* built in lmgw (device link WP5) are `?keepalive_s=`,
  `hello.open_approvals`, `caught_up`, `state.open_approvals` and `thread_title` on
  `approval.requested`; not yet built (they come with the device link's WP2 and WP3):
  `TCP_USER_TIMEOUT`, `hello.urls`, `hello.cert`, `certificate_expired` and the per-link details.

**`lmgw-client`** (lmgw `crates/lmgw-client`, about 3,500 lines with tests; dependencies only
`lmgw-api-types` and `serde_json`; no I/O, no clock, time passed as `now_ms`; builds for wasm):
`realtime` (`ServerEvent` via `parse`, `ClientEvent::to_json`, `TurnDetection`, `close_kind`),
`voice` (`Machine`: idle, listening, thinking, speaking, interrupted; `ptt_down`, `ptt_up`,
`set_tool`, `starved`, `played_out`, `cut`, `current`, `quiet`), `truncate` (`Replies`,
`PlaybackCursor`), `feed` (`SseDecoder`, `FeedReader`), `requests` (builders for folders,
`current`, threads, the feed, realtime, `mcp_host`, approvals, transcribe, tasks and profiles;
`read_refusal`, `key_refused`, `idle_rollover_at`, the error codes), `mcp_host`. Its types are
owned and FFI-friendly, but several DTOs carry `serde_json::Value`, and a few hot-path getters lend
(`&str`), which a wrapper clones.

**Kai** (the desktop client):
- `kai/src/pairing.rs` (binary crate): `PairingLink::parse` (strict v=1), `DeviceKey` with a
  redacted `Debug`, `Fingerprint`, `normalize_base_url`, `verify_fingerprint` (fails closed),
  `PairFacts`, `plain_http_remote`, `display_name`. Its dependencies are `url`, `form_urlencoded`
  and `thiserror`. The device link moves it into `lmgw-client` for v2 (P4), where `PairingToken`
  replaces `DeviceKey`, the enrolment verifier replaces `verify_fingerprint`, and
  `plain_http_remote` goes.
- `kai/src/approvals/words.rs`: the whole-utterance yes/no matcher (kai §11.1).
- `kai/crates/kai-desktop`: the tool contract and capability traits, no desktop or I/O dependency;
  `kai/crates/kai-delegate`: the delegate loop behind traits.
- After a takeover (4000) desktop Kai goes idle and does not reconnect by itself (kai §5.6); Q3's
  answer changes that when the phone's call ends (§7.9), a change in Kai's repository.
- Plasma-bound, not reusable: PipeWire audio, GTK and layer-shell overlays, WebKitGTK, ksni, D-Bus,
  the Secret Service, the RPM updater.

**Not built:** the calls route with its `lmgw.*` events and `chat_folder` (calls); the device link
(its listener, CA, enrolment and confirmation, the feed's push additions, the Devices card's QR
code); `message.*` feed events; the typed `Message`, `Attachment`, `SendRequest` and send frames.

**Android platform facts** (researched 2026-10-14; V = verified on a primary source, U = unverified,
to confirm in WP0 or the WP that uses it):

| Fact | | Used in |
|---|---|---|
| A `VoiceInteractionService` needs `BIND_VOICE_INTERACTION`, meta-data `android.voice_interaction` pointing at a `<voice-interaction-service>` XML, and AOSP rejects it without `sessionService` and without `recognitionService` | V | §9.1 |
| `supportsAssist`, `supportsLaunchVoiceAssistFromKeyguard`, `supportsLocalInteraction` and `settingsActivity` are optional; `onLaunchVoiceAssistFromKeyguard()` must start a `showWhenLocked` activity | V | §9.1, Q7 |
| `ROLE_ASSISTANT` exists from API 29; `createRequestRoleIntent(ROLE_ASSISTANT)` reportedly shows no dialog, so setup deep-links `Settings.ACTION_VOICE_INPUT_SETTINGS` and re-checks `isRoleHeld` on resume (ChatGPT does the same) | U | §9.5 |
| The power button's long press reaches a third-party assistant when the OEM setting points at "digital assistant" (Pixel) | U | §9.4 |
| Samsung's side button: Settings → Advanced features → Side button → Press and hold defaults to Gemini and offers other digital assistant apps instead of the power-off menu (Samsung's support page); whether a sideloaded VoiceInteractionService is offered on One UI 8 | V / U | §9.4, WP0 (a) |
| Samsung's "Never sleeping apps" list under Background usage limits (Samsung's developer page gives Settings → Device care → Battery → Background usage limits; some releases say "Never auto sleeping apps"): its apps are never put to sleep; the path and label on One UI 8 | V / U | §9.5, WP0 (m) |
| LineageOS overlays `config_longPressOnPowerBehavior` to 1, the global actions menu (5 is the assistant, AOSP's default); `Settings.Global.power_button_long_press` overrides it at runtime; whether LineageOS offers the choice as a user setting | U | §9.4, WP0 (a) |
| The corner swipe arrives as `SHOW_SOURCE_ASSIST_GESTURE`; a headset button as `SHOW_SOURCE_PUSH_TO_TALK` or `ACTION_VOICE_COMMAND` | U | §9.4 |
| `onHandleAssist` comes only with `SHOW_WITH_ASSIST`, `onHandleScreenshot` only with `SHOW_WITH_SCREENSHOT`, and null when the user turned "use text from screen" or "use screenshot" off | V | §9.7 |
| The session window is `TYPE_VOICE_INTERACTION`; `onShow` can repeat; `hide()` is preferred over `finish()`; `startAssistantActivity(intent)` moves into the app | U (window type V) | §9.2 |
| Hotword APIs need `MANAGE_HOTWORD_DETECTION` (and `CAPTURE_AUDIO_HOTWORD` for the DSP path): privileged, unavailable to a sideloaded app | V | Q9 |
| Android 14+: a `microphone` foreground service must start while visible, except for the app providing the active VoiceInteractionService; the type needs `FOREGROUND_SERVICE_MICROPHONE` and `RECORD_AUDIO`; no timeout is listed for it (Android 16 specifics unverified) | V / U | §8.6 |
| Core-Telecom 1.0.0 (`phoneCall` service, `MANAGE_OWN_CALLS`, CallStyle within 5 s) forbids `setCommunicationDevice` and SCO beside it | V | M12 |
| `io.github.webrtc-sdk:android` 150.7871.01 (2026-09-05) is the freshest libwebrtc build; standalone use with the `org.webrtc` package is unverified. *2026-10-15 (WP0 spike build):* it builds and packages standalone (its classes are `org.webrtc.*`); its arm64-v8a library is 12.3 MB, stored uncompressed and 16 KB aligned (`p_align` 0x4000); it also has `JavaAudioDeviceModule.prewarmRecording()` and `requestStartRecording()` (for Q5) and `getPlatformAudioProcessingState()`; initialising on the phones is WP0 (o)'s | V / U | §7, WP0 (c), (o) |
| libwebrtc's Android network monitor binds each ICE socket to its network with `android_setsocknetwork` (`sdk/android/src/jni/android_network_monitor.cc`), overriding a process binding | V | M5, §5.2 |
| `JavaAudioDeviceModule` uses the hardware AEC and NS where the device reports them; with them off WebRTC's software APM (AEC3) runs; some devices report a broken hardware AEC | V | §8.2 |
| A WebRTC track that is disabled keeps sending RTP, with silence | V | §7.5 |
| The Wi-Fi low-latency mode is active only for the foreground app with the screen on (source.android.com, Wi-Fi low latency) | V | §8.6 |
| `TileService.startActivityAndCollapse(Intent)` throws for apps targeting API 34+; the `PendingIntent` overload replaces it | V | §9.4 |
| JNA supports 16 KB pages from 5.16.0 (JNA issue 1618) | V | §2.1 |
| WireGuard's `REKEY_TIMEOUT` is 5 s: a lost handshake initiation is retried after it | V | M8 |
| uniffi-rs 0.32.1 (2026-09-08); Kotlin through JNA with `libjnidispatch` per ABI; 0.32.0 broke `[ByRef] bytes` and async primary constructors in Kotlin; a JNI generator exists unreleased | V | M4 |
| ChatGPT has been a default-assistant app since March 2025 (overlay and voice on invocation, no hotword); Gemini offers use without unlocking, with personal content gated on unlock | U | §9, Q7 |
| For an app targeting Android 15+, `dataSync` services run 6 hours in 24, then `onTimeout` and a stop within seconds; `mediaProcessing` likewise; no other type has a timeout (developer.android.com, foreground-service timeouts and the Android 15 behaviour changes, checked 2026-10-14) | V | M16 |
| `specialUse` needs `FOREGROUND_SERVICE_SPECIAL_USE` and a `PROPERTY_SPECIAL_USE_FGS_SUBTYPE` property, has no runtime prerequisite, and is reviewed only by Google Play | V | M16, §12.1 |
| Android 15+: a `BOOT_COMPLETED` receiver may not start a `dataSync`, `camera`, `mediaPlayback`, `phoneCall`, `mediaProjection` or `microphone` service; `specialUse` is not listed | V | §12.1 |
| A process with a foreground service keeps its network in Doze; whether One UI 8 keeps a held socket through Doze and its own app sleeping | U | WP0 (m) |
| `setAndAllowWhileIdle` is inexact, never fires early, needs no exact-alarm permission and may fire while idle; how far Doze spaces it | V / U | §12.2 |
| Targeting API 36, on displays 600 dp and wider: `screenOrientation`, `resizableActivity`, `minAspectRatio`, `maxAspectRatio` and `setRequestedOrientation()` are ignored; the opt-out property `PROPERTY_COMPAT_ALLOW_RESTRICTED_RESIZABILITY` stops working at API 37 (Android 16 behaviour changes) | V | M23 |
| Android 16 announces local network protection: traffic to a LAN address will need a permission (opt-in now, `NEARBY_WIFI_DEVICES` restores it, a new permission in the Nearby devices group later); whether One UI 8 enforces it | V / U | §5.2, WP0 (k) |
| A foldable's cover screen (e.g. 904×2316 px) and inner screen (e.g. 1812×2176 px); their dp and size classes | V / U | M23, WP0 (n) |
| One UI's "Continue apps on cover screen" decides whether an app stays open when the phone folds | U | §10.6, WP0 (n) |
| A Keystore EC key signs with `Signature("SHA256withECDSA")`, giving a DER signature; `PublicKey.getEncoded()` is the DER SubjectPublicKeyInfo | V | M7, §4.3 |
| rustls's `Signer::sign` is synchronous (device link §6.1) | V | M7 |

## 1. Overview

```
┌───────── Kai mobile: one process, kept alive by KaiService (specialUse; M16) ──────────────────┐
│ Kotlin                                              Rust core (kai-mobile-core, UniFFI/JNA)    │
│                                                                                                │
│ Compose UI ◀─ Flow ◀─ next_update() ◀────────────── app state, Kai rules, notifications        │
│ VoiceInteractionService ─▶ session overlay           lmgw-client: pairing, parse, Machine, feed│
│ Keystore P-256 key ◀─ Keystore.sign() ◀──────────── rustls (ring): lmgw's CA pinned, our cert  │
│                                                                                                │
│ libwebrtc (every network): mic, AEC, Opus ═══ SRTP over UDP, lmgw's one candidate ═════════════╪══▶ lmgw
│   oai-events ── channel_message() ──▶ voice rules ── EventSink.send() ──▶ oai-events           │
│   offer SDP ──▶ Voice::open() ── POST /v1/realtime/calls?chat_folder= (sdp + session) ─────────╪──▶ device
│ CameraX+ZXing, notifications, alarms,               mTLS: enrolment, the held feed, /chat/api/…╪──▶ listener
│ MediaSession, routing, network callbacks                                                       │
└────────────────────────────────────────────────────────────────────────────────────────────────┘
```

| Phase | Scope | lmgw needs (§16) |
|---|---|---|
| First usable version (WP0 to WP5) | pairing over the device link, connection states, the Kai conversation by text, the held feed and its notifications, voice over WebRTC, assistant mode, approvals by tap, both screens of a foldable | P1 to P5, P8 to P11; P12 for a second device |
| After it | the message cache, threads browser, lock screen, publishing for Obtainium, Baseline Profile, silent certificate renewal | lmgw's device link WP8 |
| Later | phone tools (device-hosted MCP), voice approvals, screen context, the tabletop posture, iOS | `input_image`, own designs |

## 2. Repository and build

### 2.1 Toolchains

- **Gradle** with the wrapper committed, Kotlin DSL, a version catalog (`gradle/libs.versions.toml`)
  pinning AGP, Kotlin, Compose BOM, CameraX, ZXing, JNA and the libwebrtc build. JDK 21 as the
  Gradle toolchain; the Compose compiler is the Kotlin plugin's.
- **Rust** stable, targets `aarch64-linux-android` and `x86_64-linux-android`, cargo-ndk, `just`.
- **16 KB pages.** Android 15+ devices with 16 KB pages need 16 KB-aligned native libraries. NDK r28
  or newer aligns the core by default; JNA is pinned at 5.16.0 or newer, the first with 16 KB
  support for `libjnidispatch`; libwebrtc's `.so` is checked by WP0 (e) (`zipalign -c -P 16`, the
  NDK's alignment check).
- **Per-target flags go into `core/.cargo/config.toml`**, never into `RUSTFLAGS` in the environment:
  that variable replaces the configured flags of every target and changes fingerprints.

### 2.2 The core and its bindings

- **`kai-mobile-core`** is `crate-type = ["lib", "cdylib"]` (the lib for host tests, the cdylib
  for Android). Its library is `libkai_mobile_core.so`. `uniffi.toml` sets the Kotlin package
  `dev.rootshell.kai.core.ffi`.
- **`just core-android [profile]`:** `cargo ndk -t arm64-v8a [-t x86_64] -o
  ../android/app/src/main/jniLibs build --profile <p>`, then `uniffi-bindgen generate --library
  <the .so> --language kotlin --out-dir ../android/app/build/generated/uniffi`. Both outputs are
  gitignored.
- *Changed 2026-10-14 (WP1):* the recipe takes the variant's output directories as arguments
  (`just core-android <profile> <jniLibs> <bindings> <abis>`, defaults under `android/app/build/core/`),
  so a debug and a release build in one Gradle run cannot overwrite each other; nothing generated lands
  in `src/`. The container build stages lmgw's two crates from `LMGW_PIN`'s tree into `.stage/lmgw`
  (`scripts/lmgw-pin.sh stage`) and mounts them at `/src/lmgw`.
- *Changed 2026-10-15 (WP0 spike build):* uniffi 0.32's `--config` takes a global file (`[defaults]`,
  `[crates.<name>]`) and ignores a flat `uniffi.toml` with a warning, so `just core-android`'s
  `--config kai-mobile-core/uniffi.toml` does nothing; the bindings are right because bindgen finds the
  crate's own `uniffi.toml` through cargo metadata. A cdylib that carries the core beside another
  UniFFI crate (the WP0 spike's `kai-wp0-spike`, later perhaps the phone tools) must invoke
  `kai_mobile_core::uniffi_reexport_scaffolding!()`, or the linker leaves the core out, and gives the
  core's bindings its library name through `[crates.kai_mobile_core.bindings.kotlin] cdylib_name`.
- **Gradle:** `preBuild` depends on an Exec task that runs the recipe for the variant (`debug`:
  both ABIs; `release`: `arm64-v8a` only), and the generated directory is a Kotlin source set. ABI
  filters keep `libjnidispatch` to the same ABIs.
- **Profiles:** release `opt-level = 3`, `lto = "thin"`, `codegen-units = 1`, `panic = "unwind"`
  (M4), `strip = "debuginfo"`; CI keeps the unstripped library as an artifact for `ndk-stack`.
  *Changed 2026-10-14 (WP1 review):* the release profile has `debug = "line-tables-only"` and `strip = "none"`: AGP strips the
  packaged copy anyway (the APK does not grow), and the unstripped library CI keeps gives `ndk-stack`
  file:line. Also: every cargo build, test and tree command of the recipes runs `--locked`,
  and `android/gradle/verification-metadata.xml` pins every Gradle dependency by sha256.
- **R8:** full mode for release, with keep rules for JNA (`com.sun.jna.**`) and the generated
  bindings, which JNA reaches by reflection.
- **Versions:** `versionName` is `<X.Y.Z>+<pipeline IID>` and `versionCode` the pipeline IID, which
  only grows within the project (Kai's `+IID` practice, kai §13.1). A local build uses `0` and is
  never mistaken for a CI build. *Changed 2026-10-14 (WP1):* AGP refuses a versionCode of 0, so a local
  build is `<X.Y.Z>+local` with versionCode 1. The About page shows the app's version, the core's and the lmgw pin.
  *Changed 2026-10-19:* the public release scheme (GitHub releases). `versionName` is the plain `X.Y.Z`, and
  `versionCode` is `100000000 + major * 10^6 + minor * 10^3 + patch` (each field up to 999). The private
  builds keep `<X.Y.Z>+<IID>` with the IID as the code, which stays below 100000000, so a phone goes one way only,
  from a private build to a public release; Android refuses the way back.

### 2.3 Dependencies of the core

`lmgw-client` (with its `tls` feature) and `lmgw-api-types` by path; `uniffi` (with `tokio`);
`tokio` (rt-multi-thread, time, sync, macros); `rustls` 0.23 with the `ring` provider and no
aws-lc-rs (M7); `reqwest` with `default-features = false`, `stream`, `multipart` and its rustls
backend without a bundled provider (the feature's name at the chosen version), handed the pinned
configuration through `tls_backend_preconfigured`; `pulldown-cmark`; `serde` and `serde_json`;
`tracing`; a logcat writer (`android_log-sys` behind a small `tracing` layer); `thiserror`; and, as
a dev-dependency only, rcgen with its ring feature for the tests' certificates (§15.3). WP1 checks
with `cargo tree` that aws-lc-sys is not in the build. No WebSocket crate and no SQLite in
the first version: voice is WebRTC, the feed is SSE, and the message cache waits (M10).
tokio-tungstenite (with `Connector::Rustls` from the one constructor) joins with the phone tools
(§13), rusqlite with the cache. *Changed after the owner's answers, 2026-10-14:* v3 had no TLS.

## 3. The Rust core

### 3.1 Inside

```
core/kai-mobile-core/src/
├── lib.rs          UniFFI exports only: Core, Voice, records, errors (§3.2)
├── link/           the pinned TLS configuration and the one client constructor (M7), the URL
│                   race (M5), the feed task with its cursor, backoff, wake-ups, reachability
├── signer.rs       rustls's SigningKey over the Keystore callback (M7)
├── conversation/   the folder (P5's rule), current, the Kai thread's messages, the send stream
├── voice/          the call's signalling, the events channel, Machine, captions, mic decisions,
│                   the pending bind, the stop and playout reports, end reasons from the call
├── approvals/      the open set from the feed and pending calls from the call, decisions
├── notify/         which feed events notify, and the notifications' ids (§12.3)
├── settings.rs     the JSON settings file
├── store.rs        the pairing's public facts and the feed's cursor (§3.3)
├── markdown/       pulldown-cmark → blocks and inlines (M14)
└── pairing/        a thin layer over lmgw_client::pairing (P4): the v2 link, enrolment, the
                    confirmation's wait
```

- **One tokio runtime** (multi-threaded, the default worker count), created by `Core::new` and kept
  for the process's life. Time for `lmgw-client` comes in milliseconds from the elapsed-real-time
  clock (`CLOCK_BOOTTIME`), which keeps counting while the phone sleeps (§12.2); the idle
  rollover's wall-clock reading follows kai K2-8 (`last_message_at`).
- **Each area is a task that owns its state** (link, conversation, voice, approvals). Every FFI
  entry point sends its area a message: a synchronous one returns at once, an async one awaits the
  area's answer. No entry point takes a lock, so a call on libwebrtc's signalling thread or the main
  thread never waits behind the core (M4).
- **Updates** reach Kotlin through one unbounded channel that `next_update()` reads. A slow UI never
  drops an update: they are snapshots (connection, voice) or deltas with ids (messages), and a
  collector that falls behind merges them.

### 3.2 The API Kotlin sees (a sketch; WP1 fixes the names)

```rust
#[derive(uniffi::Object)] pub struct Core { /* … */ }

#[uniffi::export(async_runtime = "tokio")]
impl Core {
    #[uniffi::constructor] pub fn new(dirs: AppDirs, facts: PlatformFacts,
        keystore: Box<dyn Keystore>) -> Arc<Self>;           // reads its stored pairing (§3.3)
    pub fn pairing_preview(&self, text: String) -> Result<PairingPreview, PairingError>; // no I/O
    pub async fn pair(&self, preview: PairingPreview) -> Result<Pairing, CoreError>; // key, enrol
    pub fn check_now(&self);                                 // the confirmation's Check now (§4.2)
    pub async fn forget(&self);
    pub fn set_network(&self, net: NetworkFacts);            // the default network changed (§5.2)
    pub fn set_foreground(&self, visible: bool);             // current, notifications (§12)
    pub fn wake(&self);                                      // an alarm or a wake: deadlines (§12.2)
    pub async fn next_update(&self) -> Update;               // pulled into a Flow
    pub async fn send_text(&self, text: String) -> Result<(), CoreError>;
    pub async fn new_conversation(&self) -> Result<(), CoreError>;
    pub async fn decide(&self, approval: ApprovalRef, approve: bool) -> Result<(), CoreError>;
    pub fn settings(&self) -> Settings;
    pub fn set_settings(&self, settings: Settings) -> Result<(), CoreError>;
    pub fn voice(&self) -> Arc<Voice>;
}

#[uniffi::export(async_runtime = "tokio")]
impl Voice {
    pub fn prepare(&self);                                   // warm a connection (§7.2)
    pub async fn open(&self, offer_sdp: String, start: CallStart, sink: Box<dyn EventSink>)
        -> Result<String /* answer SDP */, CoreError>;       // chat_folder, takeover=ask[, new]
    pub fn transport(&self, state: TransportState);          // ICE/DTLS states; DTLS up = listening
    pub fn channel_open(&self);
    pub fn channel_message(&self, text: String);             // enqueues only; parts reassembled
    pub fn channel_closed(&self, why: String);
    pub fn take_over(&self);                                 // the pending bind's Take over
    pub fn press(&self); pub fn release(&self);              // push-to-talk
    pub fn tap(&self);                                       // stop Kai (K6: else the mute)
    pub fn set_muted(&self, muted: bool);
    pub fn stats(&self, request: u64, stats: PlayoutStats);  // Kotlin's answer to read_stats
    pub async fn end(&self, why: EndReason);                 // stop, then close the channel
}

#[uniffi::export(callback_interface)]
pub trait EventSink: Send + Sync {
    fn send(&self, text: String);                            // one oai-events message or part
    fn set_mic(&self, mic: MicState);                        // Live, Silent, Off (§7.5, §7.9)
    fn set_playback(&self, audible: bool);                   // Kai's remote track (§7.6)
    fn read_stats(&self, request: u64);                      // inbound-rtp stats → Voice::stats
    fn close_channel(&self);                                 // End, after the stop
    fn ended(&self, end: CallEnd);                           // the core ended the call
}

#[uniffi::export(callback_interface)]
pub trait Keystore: Send + Sync {
    fn generate(&self, alias: String) -> Result<Vec<u8>, KeystoreError>; // DER SPKI (§4.3)
    fn sign(&self, alias: String, message: Vec<u8>) -> Result<Vec<u8>, KeystoreError>; // DER
    fn delete(&self, alias: String);
}
```

`CallStart` carries whether the call starts a new conversation (`new=true`) and whether the owner
already chose Take over (§7.9). `MicState` is `Live` (the track enabled), `Silent` (the track
disabled: RTP with silence, for mute, push-to-talk between turns and K16) or `Off` (the sender's
encoding inactive: no capture, no RTP, the microphone indicator off, for a pending bind).
`PlayoutStats` carries the remote audio track's `inbound-rtp` fields as libwebrtc reports them
(`estimatedPlayoutTimestamp`, `jitterBufferDelay`, `jitterBufferEmittedCount`), each optional;
Kotlin answers every `read_stats`, with empty fields when a stat is missing or the read failed.

`Update` is an enum: `Connection(ConnectionView)` (the state of §5.3, the confirmation code while
it waits, the certificate's notice), `Conversation(ConversationView)`, `Messages(MessagesDelta)`,
`Voice(VoiceView)` (state, captions, muted, held, loading alias, who holds the thread, a pending
bind's `held_by`, the end and its reason), `Approval(ApprovalView)`, `Notify(NotificationView)` and
`Withdraw(NotificationId)` (§12.3), `WakeAt(Option<u64>)` (§12.2), `Notice(Notice)`. `CoreError`
carries a kind, lmgw's code and lmgw's message verbatim, and the setting that bounded a wait where
one did.

`Keystore` is Kotlin's: it generates the device key under the alias the core names and returns its
DER SubjectPublicKeyInfo, signs a TLS handshake's message with `SHA256withECDSA`, and deletes a key
the core no longer needs. `Pairing` is what the confirmation screen shows: the device's name as
lmgw gave it, the code, and the URLs.

*Changed 2026-10-15 (WP2, as built):* `Core::new(dirs, facts, keystore)`, where `PlatformFacts` carries
the OS for the enrolment's `client` text, the app's version, and the system's active approval and
reply notifications at start (`notified_approvals`, `notified_replies`): the first feed's reader must
know them (§5.4), and an entry point called after the constructor would race the first connect. `pair`
takes the link's text, not a preview record (the token crosses only inside the text);
`pairing_preview` is async, since it asks the link which pairing it would replace. `retry()` is both
Check now and Try again. `decide` answers `Decided::Sent` or `Waiting { remaining }`: lmgw needs a
verdict for every call of a reply at once (api §6.3), so the phone holds a reply's verdicts until its
last call has one, as Kai desktop does. Beside them: `paired()`, `dismiss_approval(id)` (a swiped
approval stays dismissed) and `notification_gone(id)` (a tapped reply). `Update` gains
`Approvals { open }`, the cards' set; `PairingView` is the confirmation screen's `Pairing`.
`next_update` runs in the caller's future and is cancellation-safe (a collector cancelled while it
waits loses no update). Markdown arrives as flat blocks naming their list and quote depth, since
UniFFI carries no recursive types. Kotlin's `CoreFacade` passes the bindings' records as they are
(plain data classes) and hides only the `Core` object.
*(changed 2026-10-15: WP2's security review: a `decide` before the first `caught_up` since start
(a notification's action that started the process) is held until the open set is known, at most
`lmgw.timeout_s`, then refused with "the decision was not sent" (§11). `dismiss_approval` is async
and answers once `approvals.json` holds the dismissal. The `Keystore` callback gains `aliases()`
(its `kai-device-*` keys, for the start's sweep, §4.3) and tells `KeystoreError::Gone` (the key
lost for good) from `Failed` (anything else). `GatewayView` carries the key's alias, so the settings
page can read its security level.)*

*Changed 2026-10-16 (WP5, as built):* `Core::set_overlay(showing_cards)` (the assistant overlay shows
the approval cards, unlocked: with the app's visibility it says whether a card is seen, §11, §12.3);
`EventSink::asked()` (a call waits for the owner's verdict in this call: the ask's earcon and haptic,
§10.5); `ApprovalView` gains `sent`, `waiting`, `in_call` and `note` (§11's note); `VoiceView` gains
`awaiting`, the bound thread's tools waiting for a verdict (§7.6's note).

### 3.3 What crosses the boundary and what is stored

*Changed after the owner's answers, 2026-10-14:* v3 carried a bearer key across the FFI twice and
stored it encrypted in `credentials.bin` (device link §6.4 item 7). Now no secret crosses the
boundary or reaches storage:
- **The private key never leaves the Keystore.** Across the FFI travel its SubjectPublicKeyInfo
  (once, at generation) and, per new connection, the handshake message to sign and its signature.
  Neither is secret.
- **The pairing token** crosses once, inside the link's text, to `pairing_preview` and `pair`. In
  the core it is `lmgw_client::pairing::PairingToken` (redacted `Debug`, no `Display`); Kotlin drops
  the text from memory and from the text field after `pair`. Neither side logs it. Once used it is
  worthless, and before that its use still needs the owner's Confirm (device link D23).
- **`link.json`**, written atomically (a temp file and rename) by the core: the gateway's name, the
  URL list and the URL that last connected, the device certificate, lmgw's CA certificate (kept only
  if its SubjectPublicKeyInfo hashes to the link's `fp`), `not_after`, `notice_after`, the
  confirmation code, the Keystore alias, and the feed's cursor. All of it is public.
- `android:allowBackup="false"`, and data-extraction rules exclude everything: a Keystore key never
  migrates, so a restored `link.json` names a key the new phone lacks. A new phone pairs again.
- **Forget gateway** deletes `link.json`, `approvals.json` (§5.4) and the Keystore entry.
  *Changed 2026-10-15 (WP2, as built):* `link.json` also keeps the key's SubjectPublicKeyInfo (what
  rustls's signing key reports, checked against the certificate when the pinned configuration is
  built) and the Kai folder the conversation remembers (kai §4's remembered id, §6.1, which §6.1
  called a setting): the id is the gateway's own, so it lives with the pairing and goes with Forget
  gateway.
  *Changed 2026-10-15 (WP3 part 1's review):* `calls.json`, written atomically by the voice task,
  keeps the ids of the voice calls lmgw may still hold for this device (§7.8's note). Forget gateway
  leaves it: any lmgw answers an older pairing's call id 404, which counts as hung up.

### 3.4 Settings

Kept in the core's JSON file, edited through `set_settings`, all shown on the settings page:

| Key | Default | Meaning |
|---|---|---|
| `lmgw.timeout_s` | 10 | a connect (the whole URL race), an answer's head, the feed's first byte (kai §3.2's waits); M8 |
| `lmgw.url_stagger_ms` | 250 | the wait before the race tries the next URL (RFC 8305's connection attempt delay); M5 |
| `lmgw.retry_max_s` | 30 | the feed's backoff ceiling while the app is visible |
| `lmgw.retry_max_background_s` | 300 | the feed's backoff ceiling while the app is not visible, so that a phone whose lmgw is off for hours wakes at most this often (§12.2) |
| `feed.keepalive_s` | 120 | the keep-alive the held feed asks of lmgw (`?keepalive_s=`, up to lmgw's `chat_feed_keepalive_max_s`); a silent drop is noticed after two intervals (§12.2) |
| `pairing.confirm_poll_s` | 2 | how often the core asks again while it waits for the owner's Confirm and the screen with the code is visible (§4.2) |
| `conversation.folder_name` | "Kai" | kai §4 |
| `voice.mode` | thread | the thread's mode as lmgw reports it (`session.lmgw.turn_mode`), or always automatic, or always push-to-talk (§7.4) |
| `voice.connect_timeout_s` | 15 | from the trigger to `session.created` or a pending bind (kai §5.3, §7.3) |
| `voice.idle_end_s` | 30 | 0 = never; a pending bind counts as quiet (§7.7) |
| `voice.playout_report_ms` | 1000 | how often the phone reports Kai's playout to lmgw while Kai's audio plays (§7.6); lmgw treats the reports as stopped after one `realtime.ping_interval_s` without one, so the setting's text says to keep it well below that |
| `voice.ptt_tail_ms` | from WP0 (g) | how long the track stays on after a push-to-talk release (§7.5) |
| `voice.echo` | `hardware` where the device reports a canceller, else `software` | §8.2 |
| `voice.echo_tail_ms` | the dashboard's value | sent with half duplex |
| `voice.route` | speaker | the default output when no headset is connected (§8.3) |
| `voice.earcon`, `voice.haptics` | on | §10.5 |
| `appearance.colours` | Kai | Kai's graphite and blue, or the system's dynamic colour (Q6) |

*Changed 2026-10-15 (WP3 part 2, as built):* `voice.echo` takes `auto` (the default since part 1:
`hardware` where the device reports a canceller, else `software`), `hardware`, `software` and
`half_duplex` (§8.2); `voice.echo_tail_ms` is sent with half duplex only, and unset leaves lmgw's own
value (the part names none). The voice settings page shows both; a change applies from the next
call.

### 3.5 Logging

`tracing` in the core and `android.util.Log` in Kotlin, both to logcat, tag `kai`. Nothing leaves the
phone (M17). An "Export log" item writes the app's own logcat lines to a file through Android's
share sheet only when the owner taps it. The pairing link and its token, signatures and message
contents are never logged; ids, codes and the confirmation code are.

## 4. Pairing and the device link

*Changed after the owner's answers, 2026-10-14:* v3 stored a bearer key and probed the feed with it
through the tunnel. Pairing is now enrolment, the owner's confirmation and the first mTLS feed
(ruling 8; device link §6.4 items 5, 6, 7 and 10).

### 4.1 Getting the link

1. On lmgw's Devices card the owner chooses **Pair a device…** (device link §3.2) and fills in the
   device's name (e.g. "phone"), its policy as Q4's answer has it (the aliases the Kai folder uses,
   a budget when any of them has a cloud fallback, the tool scope `kai__*`, `self_admin` off, a
   `concurrency_limit` of at least 2), "another device", and the URL list: the device listener's
   LAN address and its WireGuard address, in the order the owner prefers. The first such pairing
   turns the device listener on and, after its confirmation, widens it beyond loopback (device link
   D24). The QR card (P10) shows the QR code, the link with Copy, the token's expiry, the URLs and
   `fp` shortened.
   *(changed 2026-10-15: lmgw's device-tools grant (client-apps design L16, the owner's decision). A tool scope naming `kai__*` no longer reaches Kai desktop's tools by itself. The phone's policy ticks **Device tools: kai** on the pairing form (or later in the device's policy dialog), and its tool scope stays `all`; any other scope then filters Kai's tools as it does any server's. A Kai-folder thread on a phone without the grant gets an `error` frame with `code: "tool_unreachable"` and `server_label: "kai"`, then answers as a plain chat turn; an offline desktop gives `tool_offline` the same way. `lmgw-client` reads the code with `ErrorFrame::label_code`.)*
2. The phone takes it one of three ways:
   - **Scan** (first-run screen and settings): CameraX preview, `ImageAnalysis` frames to ZXing's
     `QRCodeReader` on the Y plane. `CAMERA` is asked when the scanner opens, not before. The screen
     says not to scan the code with the system camera or Lens instead, since their frame may leave
     the phone.
   - **Tap:** `MainActivity` has a `VIEW` + `BROWSABLE` filter for scheme `lmgw-pair`. Any app or
     web page can fire it, so it only ever leads to the confirmation.
   - **Paste:** a text field; surrounding whitespace is ignored (the parser's rule).
   *Changed 2026-10-15 (WP2, as built):* all three lead to one pairing screen (the facts, then Pair);
   the scanner is reached from it. Scanning is built but cannot be tried against lmgw until lmgw's QR
   card (device link WP4); paste and tap can. The link's text lives in memory only (the app's hub,
   never saved state) and is dropped after a pairing.
   *(changed 2026-10-15: WP2's security review: Pair sends exactly the text the shown facts came
   from, never a text changed since the preview. The field turns autocorrect off
   (`autoCorrectEnabled = false`, the URI keyboard); Compose 1.12 has no switch for the keyboard's
   personalised learning. The window is `FLAG_SECURE` while the pairing screen or its scanner
   shows. The exported `MainActivity` reads one thing from an intent, a tapped `lmgw-pair:` link;
   notification taps go through a trampoline that is not exported (§12.3). Left for the owner's
   check on the phone: whether the system's activity START log line shows the `lmgw-pair:` link's
   query, with its token.)*
3. `Core::pairing_preview` parses with `lmgw_client::pairing::PairingLink::parse` (strict v=2) and
   returns the confirmation's facts (`PairFacts`): the device name as `display_name` shortens it,
   the URLs, `fp` shortened, and the pairing it would replace. A v1 link gets "this link is from a
   pairing before TLS; make a new one on lmgw's Devices card". A parse error is the parser's
   message, which never echoes a value from the link.

### 4.2 Enrolling and confirming

1. **The owner confirms on the phone** (kai K13; a client confirms before replacing a pairing and
   shows the new URLs). A re-pair ends any call and forgets the old gateway's thread first (kai
   §3.1's order).
2. **The key.** The core names a fresh Keystore alias and calls `Keystore.generate`, which returns
   the new key's DER SubjectPublicKeyInfo (§4.3).
3. **Enrolment.** The core races the link's URLs (M5) with `EnrolVerifier { fp }` and no client
   certificate, and POSTs `requests::enrol(token, public_key, client)` to the first that verifies;
   `client` is "Kai mobile <version> (Android <release>)". `read_enrolment` refuses an answer whose
   `ca_certificate` does not hash to `fp`, drops non-`https` URLs and gives `Credentials`, which the
   core writes to `link.json` (§3.3). On a re-pair it then deletes the old key: lmgw replaced the
   row's key at enrolment (device link §5.2), so the old one opens nothing any more. The answer's
   `Connection: close` ends that connection. An answer lost on the way is asked for again with the
   same key, which gets the same answer (device link D8); a new key would meet
   `pairing_token_used`.
   - A refusal is read by its code: 410 `pairing_token_used` is "someone else used this code", 410
     `pairing_token_expired` and 401 `pairing_token_unknown` ask for a new link (§5.3); 401
     `device_disabled`, 409 `public_key_in_use`, 429 `pairing_rate_limited` (retried once after its
     `Retry-After`) and 503 `devices_ca_unavailable` are shown with lmgw's code and message. Nothing
     is stored on a refusal, and the new key is deleted.
     *Changed 2026-10-14 (device link WP6):* lmgw's device listener requires a client certificate whenever no pairing token is pending, so
     a used, ended or expired pairing link meets the TLS alert `certificate_required` during enrolment.
     `lmgw-client` reports it as its own "link ended" refusal, and the 410 `pairing_token_expired` is
     mostly unreachable; the states for a used or expired link (§5.3) are shown from that refusal.
   - A server among whose certificates none hashes to `fp` is "not the lmgw this link pairs with",
     and nothing is sent to it.
   *Changed 2026-10-15 (WP2, as built, lmgw f6917516):* the `certificate_required` alert is
   lmgw-client's `TlsRefusal::Unproven`, not "the link ended": an alert is unauthenticated and may come
   before the server proved anything, so it is a failed handshake to try again, and the words suggest
   a new link without stating that the link ended (Kai desktop's review of the same switch). The core
   keeps one key per token in memory (by the token's SHA-256, never the token) until another link's
   enrolment replaces it: a retry of the same link sends the same key, so a lost answer comes back as
   it was. A refusal lmgw answered over the pinned link deletes the key; a failure that reached no
   answer (no URL answered, `Unproven`, a stranger under another CA) keeps it for the retry. These
   refusals are `pair()`'s error kinds (`PairingFailure`), shown on the pairing screen with §5.3's
   words, not connection states: nothing is stored on a refusal.
   *(changed 2026-10-15: WP2's security review: the key is deleted only after a 4xx refusal that
   carries lmgw's code and is not a 429. A 5xx, any 429 (the second one included), a 4xx without
   lmgw's code, and a 2xx that lmgw-client's own checks refuse (`ca_not_pinned`,
   `certificate_not_for_key`, `confirm_code_mismatch`) keep the key, so Pair again sends the same
   one. A re-pair writes `link.json` before anything of the old pairing goes. If that write fails,
   `pair()` answers a Storage error and the old pairing stays as it was, key included.)*
4. **The confirmation.** The phone shows "Confirm 7F3A-91C2 on lmgw's Devices card" with the code
   from the answer; lmgw's QR card shows the same code beside the phone's address. The core opens
   the feed with its certificate, and until the owner clicks Confirm lmgw answers 403
   `device_unconfirmed`, naming the code. The core asks again every `pairing.confirm_poll_s` (2 s)
   while the screen with the code is visible, on the kept-alive connection (no new handshake), and
   on the feed's backoff otherwise; **Check now** asks at once. The wait has no bound of its own:
   it is a state with words and actions (Check now, Pair again), not a spinner.
5. **Connected.** The first feed's `hello` ends the wait: the state is connected, `hello.urls`
   replaces the stored URL list, the service starts (M16), and setup continues (§9.5).
   *Changed 2026-10-15 (WP2, as built):* the service starts once the enrolment is stored, from the
   visible app, not at the first `hello`: Android refuses to start a foreground service from the
   background, and the owner may confirm on the computer while the phone's app is away. It holds the
   confirmation's wait as well.

### 4.3 The Keystore key

- `KeyPairGenerator` for `EC` in `AndroidKeyStore`; `KeyGenParameterSpec` with `PURPOSE_SIGN`, curve
  `secp256r1`, digest `SHA-256`; TEE-backed and non-extractable.
- No user-authentication requirement and no `setUnlockedDeviceRequired`: the held feed connects
  while the phone is locked (device link §6.1).
- Not StrongBox: a signature sits on every new connection's path, a call's included, and StrongBox
  is slow.
- `PublicKey.getEncoded()` is the DER SubjectPublicKeyInfo that enrolment sends and the core's
  `SigningKey` reports; `Signature("SHA256withECDSA")` gives the DER signature TLS 1.3 wants (M7).
- One key per pairing, under the alias kept in `link.json`. Forget gateway deletes it.
  *(changed 2026-10-15: WP2's security review: at start, before the link's actor answers
  anything, every `kai-device-*` key that is neither the stored pairing's nor a pending token's is
  deleted. Nothing is swept when `link.json` does not read. The settings page shows the key's
  `KeyInfo.securityLevel` and warns when it is neither the TEE nor StrongBox; it never refuses the
  key. Only a key gone for good stops the link with "pair again": `KeyPermanentlyInvalidatedException`,
  `UnrecoverableKeyException`, no entry, or keystore2's `ERROR_KEY_DOES_NOT_EXIST`. Any other
  Keystore failure is said once in a notice, until the next `hello`, and tried again on the
  backoff.)*

### 4.4 TLS and the certificate's dates

- **Every connection after enrolment** uses `lmgw-client`'s `client_config(provider, credentials,
  signing_key)` with ring: `WebPkiServerVerifier` over the stored CA alone, and a client-certificate
  resolver for the stored certificate with the Keystore-backed `SigningKey`, TLS 1.3 only. Every
  lmgw-facing client is built from it by the one constructor (M2), and a test checks that each one
  refuses a server under another CA.
- **lmgw's server certificates** change by themselves (new keys and names; lmgw re-issues them at
  half their validity). The phone trusts the CA, not a leaf, and notices nothing (device link D1,
  D3).
- **Verification failures** are said by kind: a URL host that lmgw's certificate does not name is
  "lmgw's certificate does not name <host>: add this URL to the device's row on lmgw's Devices
  card"; an unknown issuer after enrolment is "lmgw's device authority changed, or this is not your
  lmgw: pair again" (device link §5.5).
- **The certificate's dates.** From `notice_after` (two thirds of lmgw's `devices.client_cert_days`,
  365 by default) the settings page and a one-time notice say "this device's certificate lapses on
  <date>; pair it again before then", from the stored dates and `hello.cert`. Re-pairing with
  **New token…** on the card renews it without changing the device's row. After `not_after` the
  state is "certificate lapsed", whether the phone's own clock says so, lmgw answers 401
  `device_certificate_expired`, or a link ends with the kind `certificate_expired`. Silent renewal
  over mutual TLS is lmgw's WP8, after the first usable version (Later).
- The WebRTC media needs nothing of it: the DTLS fingerprints travel in the SDP, inside this TLS.

## 5. Connectivity

### 5.1 The owner's setup (no lmgw work)

*Changed after the owner's answers, 2026-10-14:* WireGuard is routing (ruling 8), and the LAN serves
at home, so no hairpinning is needed (Q2; device link §6.4 item 9).
- **lmgw's device listener** runs on the HTTP port's number plus one (8788 by default; device link
  D10). The first remote pairing widens it beyond loopback after a confirmation that names the
  exposure. The firewall admits TCP and UDP on that port (the device socket group, calls C4, §3.1, WP7: one socket per device-listener address, following it live; device link §8) in the zones the phone comes from; Fedora's default Workstation zone
  admits both already.
- **The URL list** names lmgw's LAN address and its WireGuard address, as the pairing form proposes
  them, in the owner's order. Each must reach lmgw's own listener: lmgw's one ICE candidate is the
  local address the call's POST arrived on (calls C6), so a reverse proxy in front of lmgw (another
  host, a TLS terminator, a different address) is not covered. The candidate would be the proxy's
  side of lmgw, which the phone may not reach; lmgw keeps that on its Later list.
- **Away from home** the WireGuard app's tunnel routes lmgw's tunnel address, and Kai is not
  excluded from it in the tunnel's per-app settings. Android's "Always-on VPN" keeps the tunnel up
  without attention; it is a choice, not Kai's need.
- **At home** the LAN URL answers, and the tunnel need not work from inside the home: no NAT
  hairpinning, no LAN-endpoint profile. One condition remains: a full tunnel (`0.0.0.0/0`) that
  cannot reach its endpoint from home takes the LAN with it, unless it leaves private addresses out
  (the WireGuard app's "Exclude private IPs"). WP0 (k) checks a WireGuard profile on the home LAN.
- **A user with a separate lmgw** has the same setup on that side: its device listener, its URLs, a
  phone paired with it.

### 5.2 Networks and URLs

*Changed after the owner's answers, 2026-10-14:* v3's tunnel identification, process binding and
gate are gone (M5).
- **The URL race** (M5): the URL that last connected first, then the link's order, the next one
  `lmgw.url_stagger_ms` later if no TCP answer has come. The first handshake that verifies wins and
  is remembered in `link.json`. The whole race is bounded by `lmgw.timeout_s`, and each URL's
  failure is kept for §5.3's words: refused, no answer, a name error, an unknown issuer.
- **Kotlin reports the default network**, not VPN facts: `registerDefaultNetworkCallback` hands the
  core whether a default network exists and its transport (`NetworkFacts`), for §5.3's words and to
  act on a change. Kotlin binds nothing.
- **On a change of the default network** (Wi-Fi to mobile data, a VPN coming up or going away) the
  core drops its pooled connections, races the URLs again and reopens the feed from its cursor
  (§5.4), at once and with the backoff reset. A running call is libwebrtc's to carry (§7.11).
- **libwebrtc** gathers on every network, continually (M5).
- **Android 16's local network protection** is opt-in today. Once enforced, traffic to a LAN address
  (TCP and UDP) needs a permission: `NEARBY_WIFI_DEVICES` now, a new one in the Nearby devices
  group later. The LAN URL and its calls depend on it, so the manifest declares what the enforced
  rule asks for when it ships, setup asks for it, and WP0 (k) records whether One UI 8 enforces it
  already. Whether a tunnel address counts as local under the rule is unverified.

### 5.3 States

The connection is one of these, first match wins; the voice screen's state line, the overlay and
the service's notification show it, and every state has one action (or two):

| State | Detected by | Words (examples) | Action |
|---|---|---|---|
| not paired | no `link.json` | "Not paired with lmgw" | Pair |
| waiting for confirmation | 403 `device_unconfirmed` | "Confirm 7F3A-91C2 on lmgw's Devices card." | Check now; Pair again |
| code used by someone else | 410 `pairing_token_used` at enrolment | "Someone else used this pairing code. Delete the device 'phone' on lmgw's Devices card and pair with a new link." | Pair again |
| link expired or unknown | 410 `pairing_token_expired`, 401 `pairing_token_unknown` | "This pairing link expired (devices.token_minutes on lmgw)." / "lmgw does not know this pairing link." Both: "Make a new one on the Devices card." | Pair again |
| not this lmgw | no certificate hashing to `fp` at enrolment; an unknown issuer after it | "This is not the lmgw this link pairs with." / "lmgw's device authority changed, or this is not your lmgw: pair again." | Pair again; Try again |
| certificate lapsed | the stored `not_after` passed, 401 `device_certificate_expired`, or a link ended with the kind `certificate_expired` | "This device's certificate lapsed on <date>. Pair it again (New token… on lmgw's Devices card)." | Pair again |
| key refused | 401 by code, the feed's `revoked` | §5.5's wording per kind | Pair again, or Try again |
| no network | no default network | "The phone has no network." | none: the next network retries at once |
| URL not in the certificate | a name error on every URL that answered | "lmgw's certificate does not name <host>: add this URL to the device's row on lmgw's Devices card." | Try again |
| lmgw not running | every URL refused the connection | "lmgw is not running on <host>, or its device listener is off." | Try again (and the backoff) |
| computer not answering | `lmgw.timeout_s` passed with no TCP answer from any URL | "The computer with lmgw does not answer at <LAN address> or <tunnel address> (asleep, off, or out of reach of its network, e.g. away without the VPN). Gave up after 10 s (lmgw.timeout_s)." | Try again; Open WireGuard |
| connected | the feed's `hello` | the device name lmgw gave the key | — |

- **Held** is a badge on any state, not a state (kai K9): "Local models are paused (GPU hold)",
  plus, per turn, which alias answered (§7.10). **The certificate's coming lapse** (§4.4) is a badge
  too, from `notice_after`.
- **Fail fast.** Every wait has a stated bound. The feed retries with the backoff, 1 s doubling to
  `lmgw.retry_max_s` while the app is visible and to `lmgw.retry_max_background_s` while it is not
  (§12.2), and the state line shows the next try's time. A new default network or the app becoming
  visible retries at once. A voice start or a send while not connected answers at once with the
  state's words, never a spinner.
- **Open WireGuard** launches the WireGuard app's launcher intent (`com.wireguard.android`, declared
  in `<queries>`), for the case away from home with the tunnel down.
- *Changed 2026-10-15 (WP2, as built):* the states are `NotPaired`, `Connecting` (the first try, no
  failure yet), `WaitingForConfirmation`, `Connected`, `NoNetwork`, `CertificateLapsed`, `KeyRefused`
  (by kind: pair again, disabled, expired, a bug in Kai, an unknown kind), `KeyMissing` (the Keystore
  cannot sign with the device key: pair again), `NotThisLmgw`, `HostNotNamed`, `NotRunning`,
  `NotAnswering` and `Unreachable` (anything else in its own words: a broken feed, a refusal of the
  feed by another code, an alert), each with every URL's failure as its detail. **A handshake's alert
  is never a refusal** (Kai desktop's review B1): rustls takes a fatal alert before the server proved
  anything, so a squatter on an address can send `bad_certificate`, `unknown_ca` or
  `certificate_expired`; it is said and tried again on the backoff. A lapse is decided by the phone's
  own date (`Credentials::state`, the dates `hello.cert` refreshes), checked before each try; lmgw's
  real refusals come over the authenticated link (a 401 by code, 403 `device_unconfirmed`, the feed's
  `revoked`) and stop the retries until Try again or Pair again, except a bug's and an unknown kind's,
  which go on with the backoff (§5.5). `NotThisLmgw` goes on with the backoff too: an unknown issuer at
  an address on a foreign network proves nothing about the owner's lmgw. A refusal of a request made
  under an earlier pairing (its identity, the SHA-256 of the certificate it went with) is ignored, so
  an old pairing's request cannot tear down a new one after a re-pair. The app becoming visible resets
  the backoff even while a try is under way.

### 5.4 The feed

*Changed after the owner's answers, 2026-10-14:* held for as long as the phone is paired and resumed
from a persisted cursor (rulings 6 and 7; device link §6.4 items 1 and 3). *Changed 2026-10-15:*
the open approvals are a set re-sent by lmgw (on `hello` and on every `state`) and reconciled by
the client at each connection, not replayed through the catch-up (device link §7.3).

One `GET /chat/api/feed?keepalive_s=<feed.keepalive_s>` (lmgw takes 1 to its
`chat_feed_keepalive_max_s`, default 900, at most 86 400, and answers anything outside with a 400
naming the bound and the setting; `hello.keepalive_s` is the value the stream keeps), held by Kai's service (M16, §12) over the
device link, built and read as Kai's (kai §3.2): `requests::feed` with the keep-alive, `FeedReader`,
`hello.keepalive_s` for the dead-link verdict (two intervals without a byte, judged on a clock that
counts sleep, §12.2), the waits of kai §3.2's 2026-10-08 note.
- **The cursor** is the id of the last stored event the core applied, written to `link.json` after
  each one. A restart of the process resumes with `Last-Event-ID`; the first feed after a pairing
  starts at "now".
- **`caught_up {cursor}`** follows the catch-up's last stored record, or `hello` at once when there
  is nothing to catch up; it is live (no `id`) and comes once per stream. Before it the core applies
  records to what it shows but raises no notification; at it, it notifies once for what is open and
  new and withdraws what is shown and gone (§12.3).
- **`hello.open_approvals`** is the open approval set at the head: every open call of a thread's
  last reply within the phone's reach, read with the head in one transaction. `FeedReader` keeps it
  and applies `approval.*` only after `caught_up`, so a catch-up neither raises nor withdraws
  anything twice (device link D17). The catch-up replays no approval.
- **Every `state` carries the set again** (`open_approvals`), read as the `state` is sent. lmgw
  sends one after every move of the phone's reach, after a thread or folder comes into its reach
  after `caught_up` (the calls already waiting there bring no `approval.requested`), after a resync
  past the head and after a prune's resync. `FeedReader` replaces its set with it: silently before
  `caught_up`, after it as `Withdrawn` and `Added` by id. A `state` sent because the set may have
  moved never goes out without it: when lmgw cannot read it the stream ends, and the reconnect's
  `hello` carries it. Any other `state` (a lag, a resize, a refresh) says `null` and the reader
  keeps its set.
- **The set is reconciled at every connection.** A reader lives for one connection, so `caught_up`
  yields `Open`, the whole set, each time, and a call decided while the link was down is only absent
  from it. The core keeps, across reconnects and process deaths, the ids it notified for and the
  ids the user dismissed: the notified ones are the system's active notifications, which outlive the
  process (the shell hands them to the core at start), and the dismissed ones are kept in
  `approvals.json` beside `link.json`, written the same way and cut at each `caught_up` to the ids
  still in the set, so the file never holds more than lmgw's open set. Each reader is built with
  `FeedReader::knowing(those ids)`, so `caught_up` yields
  `Withdrawn` for the ones it shows that are gone and `Added` for those it does not know, never
  `Open`. It filters both `Added` and `Open` by the dismissed ids: a call that left the phone's
  reach and came back arrives as `Added` again. A thread's or a reply's deletion withdraws too.
- **`hello.urls`** replaces the stored URL list (its `https` URLs only, M7), and **`hello.cert`**
  refreshes the certificate's dates (§4.4). *Seam:* lmgw does not send them yet (device link WP3);
  until it does, the URLs are the pairing link's and the dates the issued certificate's own.
  *Changed 2026-10-15 (WP2, as built):* lmgw sends both since 496eeaf7 (device link WP5); the core
  reads them through `FeedReader::urls()` and `certificate()` and writes them to `link.json`, and
  `hello.dashboard_url` shows on the gateway's settings page. A dismissed id stays dismissed when a
  `Withdrawn` names it (a call that left the reach and comes back within one connection stays
  dismissed); only `caught_up` cuts `approvals.json` to the open set.

| Event | Kai mobile |
|---|---|
| `hello`, `state` | connected; the hold badge; who holds voice in the Kai thread; turns of others; the open approvals (a `state`'s set replaces the core's); the URLs and the certificate's dates (`hello`; a seam, above) |
| `caught_up` | the catch-up is over: the open set is reconciled (`Withdrawn` and `Added`) and replies notify (§12.3) |
| `hold` | the badge |
| `folder.current` (Kai's folder) | the conversation moves; a running call rebinds at its next quiet point (§7.7) |
| `folder.updated`, `folder.deleted` (Kai's folder) | retry `current` when it waited for a model; look up again |
| `thread.updated`, `thread.deleted` (the Kai thread) | its title; `thread.deleted` (deleted, or out of this device's reach) asks `current`; a call bound to it ends by its own `lmgw.session.closed` (§7.8) |
| `message.added`, `message.updated`, `message.deleted` (P3) | the open conversation; an assistant message in the Kai folder's threads while the app is not visible and no call of the phone's runs is a notification (§12.3) |
| `turn.started`, `turn.done` by another principal | "<by> is answering" in the conversation; K16 in a call (§7.5) |
| `voice.bound` by another client | the banner "Voice is on <by>"; a call started now expects a pending bind (§7.9) |
| `voice.ended` | the banner clears; never read as the phone's own call's end (§7.8) |
| `approval.requested`, `approval.decided` | §11; after `caught_up`, a notification raised or withdrawn (§12.3); `thread.deleted` and `message.deleted` of the call's reply withdraw too |
| `resync`, a `FeedItem::Unreadable` | reload what is shown |
| `revoked {reason, message, kind}`, then the stream ends | §5.5; a running call ends by its own `lmgw.session.closed` (§7.8) |
| `device.revoked` | ignored in v1 (no hosted work to cancel) |
| unknown event | logged by name and skipped (`FeedItem::Unknown`) |

### 5.5 Revocation, expiry, disabled, unconfirmed

Read exactly as Kai reads them (kai §3.3's notes of 2026-10-07): by the code
(`requests::key_refused`) and the feed's `revoked` `kind`, never by lmgw's words, which are shown
without the token.
*Changed after the owner's answers, 2026-10-14:* the device link's kinds are added (device link
§6.4 item 11).
- `key_unknown` (a re-pair elsewhere, or deleted), an owner key's `revoked`: "pair again"; Pair
  only.
- `device_disabled`: "this device was disabled on lmgw: enable it on its Devices card, or pair
  again"; the key stays; Try again and Pair.
- `key_expired` (the row's policy): "this device's access expired on lmgw: move its expiry on its
  Devices card, or pair again"; the key stays; Try again and Pair.
- `certificate_expired` (the feed's `revoked`, a 4003's reason, the message of
  `lmgw.session.closed {kind: "revoked"}`) and 401 `device_certificate_expired`: "certificate
  lapsed" (§4.4); Pair again.
- 403 `device_unconfirmed`: "waiting for confirmation" with the code (§4.2), which a re-pair meets
  again until its own Confirm.
  *Changed 2026-10-14 (device link WP6):* the code shown is the one the app derived from its own key, not read from lmgw's refusal; the
  refusal itself is a plain `Unconfirmed`. A used or expired link is the TLS alert `certificate_required`
  at enrolment (§4.2), the "link ended" refusal.
- 401 `client_certificate_required` and 400 `credential_on_device_listener`: the phone sent no
  certificate, or a credential the device listener refuses. That is a bug in Kai, never the owner's
  to fix: shown with lmgw's code and message and "please report it with the log", tried once more,
  then left to the backoff.
- an unknown kind: lmgw's sentence in a notice, one more try with the key, then the backoff (kai
  K4-7).
  *(changed 2026-10-15: WP2's security review: a 401 with a code this build does not know is that
  unknown kind too (`KeyRefusal::Unknown`): lmgw's sentence in a notice, the key kept, the backoff.
  It is not "pair again" with the retries stopped. Only `device_key_unknown` stops with pair
  again.)*

## 6. The Kai conversation

### 6.1 Folder and thread

- **The folder** is found by the rule of kai §4 (2026-10-07, WP6): the remembered ongoing folder,
  else the ongoing folder named `conversation.folder_name` with the lowest id, else the remembered
  one not ongoing, else the plain namesake with the lowest id, else a new plain folder; a create
  settles before a reconnect may replace it, and racing creates converge on the lowest id (kai
  K2-22). The rule moves into `lmgw-client` as a pure function over the folder list (P5), so the
  phone and the desktop pick the same folder, whichever paired first. The remembered folder id is a
  setting.
- **Not ready** (`folder_no_model`, `not_ongoing`): the conversation and the voice screen show the
  owner's steps in the dashboard's words (kai §4's text), to be done on the computer. Voice and
  sends wait for `folder.updated`.
- **The thread** is asked with `requests::current(folder, new)` at process start, whenever the app
  becomes visible, after `resync`, and on `folder.current` or a deleted or archived current thread.
  A call does not ask: its POST names the folder (`chat_folder`), lmgw binds the folder's current
  thread in the same step, and `session.lmgw.resolved.chat_thread` tells the phone which thread
  that is (§7.3). The phone runs no idle timer: lmgw rolls the thread over when `current` is asked,
  or a call binds by the folder, after the idle time, and every visible start does one of the two
  first. *Changed after the owner's answers, 2026-10-14:* with the process kept alive by the
  service, "every app start" became "whenever the app becomes visible"; that request doubles as a
  check that the held feed's path still works (§12.2).
- **New conversation** (the conversation's top bar and the overlay's menu): without a call, it
  sends `current` with `{new: true}`. During a call, the call ends at its next quiet point and the
  next call's POST carries `new=true`, so the rollover and the bind are one step (§7.7).
  *2026-10-15:* the screen shows an empty new conversation the moment New conversation is chosen,
  not when lmgw answers: the core clears the conversation (messages, title, an unsaved turn, who is
  answering) and emits a `reset` delta with no thread, before `current` is asked. With no thread,
  every late frame of the old thread (a still-streaming reply, a `message.added`) fails the
  thread test and is dropped, so nothing of it returns while the call has not rebound yet; the
  new thread arrives by `current`'s answer or `folder.current`, whichever comes first. A failed
  lookup leaves the view empty with the failure's note. Before, the old thread stayed until the
  first message of the new one.
- The phone writes no thread settings, folder defaults or retention (kai §4: a device's write of
  retention is refused, and the model is the owner's choice).

### 6.2 Messages

- **Read:** `GET /chat/api/threads/{id}` for the Kai thread's messages (its `Message` typed by P3)
  at process start, after `resync` and when the thread changes, then the feed's `message.*` events
  keep it current; a call adds its own as they happen
  (`lmgw.chat.user`, `lmgw.chat.reply`; a later `lmgw.chat.reply` replaces the earlier for the same
  message, api §4.1).
- **Rendering:** each message's Markdown is parsed in the core (M14) into blocks (paragraph,
  heading, list, quote, code block, rule; a table as its source text) and inlines (text, emphasis,
  strong, code, link). Links open in the browser after a tap; images in messages (attachments,
  tool-result images) are fetched by the core into the app's cache directory and decoded by Kotlin
  with `ImageDecoder`.
- **Marks:** a voice turn shows a microphone mark; a reply answered by a fallback shows "answered
  by <alias> (fallback)" from `answered_by` or `x-lmgw-fallback` (ruling 10: shown, never refused).
- *Changed 2026-10-15 (WP2):* images in messages (fetched by the core, decoded by Kotlin) are not
  built yet; WP2 shows the messages' text. They join with WP5's polish or the attachments (Later).
- *(changed 2026-10-15: WP2's security review: links are model output, so the core keeps only
  `http`, `https` and `mailto` targets. Any other target is plain text with the target shown after
  it, so a tap cannot crash the feed's process (`file:`), reach another app's content (`content:`,
  `intent:`) or open the pairing screen (`lmgw-pair:`). The shell opens kept links through a
  listener that logs instead of throwing when no app takes them.)*

### 6.3 Sending text

`POST /chat/api/threads/{id}/send` with the typed `SendRequest` (P3), the reply read as SSE through
`lmgw-client`'s `SseDecoder` and the typed send frames (P3): `delta` appends to the reply in place,
`tool` shows the tool's line, `error` is shown verbatim with its code, `done` closes the turn.
- A send cancels the thread's live turn, a voice turn included (lmgw's rule); the input says so
  while a call runs ("sending text stops Kai's current answer").
- A send's turn needs a concurrency slot of the key, and a running call holds one for its life; at
  the key's limit the send answers lmgw's 429 `key_rate`, shown verbatim with the hint that a call
  holds a slot. The phone's key has a `concurrency_limit` of at least 2 (Q4's answer); the held feed
  takes none.
- *Changed 2026-10-15 (WP2, as built):* `lmgw-client` has no builder for the send yet, so the core
  makes `POST /chat/api/threads/{id}/send` from the typed `SendRequest` and reads the stream with
  `SseDecoder` and `TurnEvent::parse`; the builder belongs in `lmgw-client` (protocol, ruling 2), a
  follow-up there. The reply streams under a negative id until `done` names the saved one; a stream
  that ends without `done` keeps what came with why. Only the head is bounded (`lmgw.timeout_s`): a
  turn takes as long as its model.
- *(changed 2026-10-15: the send is `lmgw_client::requests::send`; the core's hand-built request is gone.)*
- *(2026-10-15: label notices.)* An `error` frame with a `LabelCode` (`tool_unreachable`,
  `tool_offline`, `tool_disabled`, `tool_unknown`, `tool_failed`) is a notice, not a failure: the
  turn goes on and answers without that label's tools. The app reads the code, never the message's
  text, and keeps the notice on the saved reply as a small line under the answer
  (`MessageView.notices`), past `done` and past a reload or the feed's copy of the reply.
  `tool_offline` says "<device> is offline. Answered without its tools." (the device's name is
  taken from lmgw's words where they name it, else the label); `tool_unreachable` says the phone
  may not use the label's tools and where to grant them (the policy's Device tools); the other
  three show lmgw's message plainly. Any other `error` frame is the turn's error, shown with its
  code as before; when no reply was saved it stays on the unsaved turn until the next send.
  Notices are shown in the conversation only: never in a notification, so never on the lock
  screen.
- Attachments are Later.

## 7. Voice

### 7.1 States

The voice state is `lmgw-client`'s `voice::Machine` (idle, listening, thinking, speaking,
interrupted), plus Kai's own (kai §5.1): off, connecting, bind pending, muted, ended with its
reason (§7.8), and the connection states of §5.3; the hold is a badge.
- **Connecting** runs from the trigger to DTLS-connected, where the ring turns to listening
  (§7.3). Between that cue and `session.created` the Machine has not started yet: what the owner
  says there is on lmgw's session timeline and is answered once the session starts.
- **Bind pending** is "Voice is on <held_by>" with Take over (§7.9): the call is connected but
  holds no thread, the microphone is off, and nothing the owner says is kept.
- **Kai's audio muted locally** is a fact beside the state, not a state: from a stop or lmgw's
  `output_audio_buffer.cleared` to the next `output_audio_buffer.started` (§7.6). It silences what
  is already in the jitter buffer; the ring shows the Machine's state.

The core publishes them as `VoiceView`; the ring (§10.3) and the service's notification (§8.6) show
them.

*Changed 2026-10-15 (WP3 part 1, as built):* the core publishes `Update::Voice { view }`
(`VoiceView`: the call's number, its phase `Off`, `Connecting`, `Pending`, `Live`, `Ending` or
`Ended`, the ring's state, the state line's words, the cue, the mode, the owner's mute, the press,
Kai's local mute, whether a tap stops Kai, the pending bind's holder and note, K16's `waiting_for`,
the thread, the running tool, the hold and a held stage, the latest error, a note for a static value,
the end) and the captions as `Update::Caption { line }`, one line whole per change by `(call, id)`
(the owner's from the transcript, Kai's per response, lmgw's word as a note), so a long call never
resends its captions. In automatic mode an idle bound call shows the ring listening (lmgw listens);
in push-to-talk it shows idle ("Hold to talk").

### 7.2 Time to listening

Invocation to listening is what the owner feels first (M21). *Shared.*
- **Before any invocation:** the core runs with its pairing loaded, kept alive by the service
  (M16); `onReady` creates the PeerConnectionFactory with its device module (no microphone opens
  until a call's audio starts) and `onPrepareShow` makes sure they exist. A change of `voice.echo`
  rebuilds them between calls.
- **A warm connection.** The held feed occupies its own HTTP/1.1 connection, and lmgw closes an idle
  kept-alive one after `devices.header_timeout_s` (30 s by default), so the call's POST usually
  needs a new connection: TCP, the TLS 1.3 handshake and one Keystore signature. `Voice::prepare()`,
  called from `onPrepareShow` and from every other trigger, opens one to the URL that last answered
  unless the pool holds an idle one, so the handshake runs while Kotlin builds the offer. WP0 (c)
  and (e) measure the handshake and the signature. Sharing the feed's connection needs HTTP/2 on
  lmgw's device listener, which is lmgw's Later item.
- **At the invocation:** the offer is built and POSTed at once. The POST names the Kai folder and
  lmgw binds its current thread, so nothing is looked up first (§7.3).
- **The cue is DTLS-connected.** lmgw keeps RTP from the first frame on the session's timeline when
  the bind succeeds directly, so the ring listens, with the earcon, as soon as libwebrtc reports the
  PeerConnection connected: before `oai-events` opens and before `session.created`. WP0 (c) checks
  that speech from the cue on reaches the turn. When the phone expects a pending bind, the cue
  waits for `session.created` (§7.9).
- **The target, a go/no-go for WP0 (c):** from the trigger (the side button's `onShow`) to the
  listening cue (DTLS connected, the earcon played), over Wi-Fi, warm process, on the primary test device:
  median 1.0 s or less over 20 runs, and no run over 1.5 s, for the LAN URL and for the tunnel URL
  alike. Mobile data through the tunnel and a cold process are recorded beside it.
- **Before the cue** words are lost: libwebrtc captures only once the call's audio runs, and lmgw's
  timeline starts at the first RTP frame. WP0 (c) splits the press-to-DTLS gap into its steps and
  measures the gap when an `AudioRecord` capture hands the microphone over to libwebrtc. Q5 decides
  with those numbers whether the phone captures from the press and sends that audio as
  `lmgw.input_audio.preroll`. If the go/no-go fails, the same numbers show which step to shorten.

### 7.3 Starting a call

1. **Trigger:** the overlay's `onShow` (§9.2), a tap on the ring, the Quick Settings tile, the
   launcher shortcut, Take over from an ended call's words (§7.9), Reconnect (§7.8), or the rebind
   after New conversation or `folder.current` (§7.7). Voice needs a confirmed pairing and a key
   that is not refused; in any other state the trigger answers at once with that state's words. The
   trigger calls `Voice::prepare()` (§7.2). The feed is normally up already (M16); when it is not
   (a fresh process), it opens alongside, and the call never waits for it. A stale call id that is
   still kept is hung up first (§7.8).
2. **Kotlin** adds the `microphone` type to Kai's service (§8.6), requests audio focus, activates
   the MediaSession (§8.4), sets `MODE_IN_COMMUNICATION` and the route (§8.3), and creates the
   PeerConnection from the prepared factory: no ICE servers, unified plan, `max-bundle`, `rtcp-mux`
   required. It adds the microphone track (a `sendrecv` audio transceiver), creates the `oai-events`
   channel (ordered, reliable) before the offer, creates the offer and sets it locally. It does not
   wait for ICE gathering: lmgw is ICE-lite, and the phone's checks reach it as peer-reflexive
   candidates (as OpenAI's own browser example posts the offer at once); WP0 (c) verifies this.
   The SDP is not edited.
3. **The core**, as soon as Kotlin hands it the offer, POSTs
   `/v1/realtime/calls?chat_folder=<id>&takeover=ask` (`&new=true` after New conversation, §6.1)
   as `multipart/form-data` with the parts `sdp` and `session` (§7.4), over the device link with the
   device's certificate, to the URL that last answered (the race of §5.2 when none has). No
   `current` comes first: lmgw binds the folder's current thread in the same step, rolling it over
   after the idle time as `current` does, so no thread id can go stale between a lookup and the
   bind. `lmgw.timeout_s` bounds the answer's head. When the feed shows another client holding the
   Kai thread's voice, the core sets the microphone `Off` before it returns the answer, so capture
   does not start (§7.9).
   - `201`: the answer SDP and `Location: /v1/realtime/calls/{id}`; the core keeps the id.
   - The folder's refusals (calls C23): 409 `folder_no_model` or `not_ongoing` shows §6.1's steps;
     404 `chat_folder_not_found` (a folder that is gone or out of reach) runs the folder lookup
     again (§6.1), which may create one, and POSTs once more.
   - 401 and 403: §5.5. Anything else (503 `webrtc_unavailable` naming its setting, a policy
     refusal) is shown verbatim with its code.
   - A held thread is no refusal: with `takeover=ask` the call connects and the bind waits (step 5).
4. **Kotlin** sets the answer; ICE pairs the phone's candidates with lmgw's one candidate, the
   address the POST arrived on (M5), then DTLS.
   **DTLS connected is the listening cue:** the ring turns to listening, with the earcon and a
   haptic tick (§10.5), and lmgw keeps what the owner says from the first RTP frame on (§7.2).
   With the microphone `Off` for an expected pending bind there is no cue yet. libwebrtc's ICE and
   DTLS states reach `Voice::transport`. SCTP follows, and the channel's open reaches
   `Voice::channel_open`.
5. **The bind's outcome** is lmgw's first word on the channel:
   - `session.created`: bound. lmgw applies the `session` part as the session's first update
     right after it, before any event from the phone, and answers it with `session.updated` (calls
     C1). The core takes the thread from `session.lmgw.resolved.chat_thread`; when it differs from
     the conversation's, the folder rolled over at the bind, and the conversation moves to it
     (§6.2). The mode comes from `session.lmgw.turn_mode` (§7.4). A microphone that was `Off` goes
     `Live`, and the cue plays at its first captured samples (§7.9).
   - `lmgw.session.bind_pending {held_by}`: the prompt of §7.9.
   - An `error` followed by `lmgw.session.closed`: a refusal at the open (lmgw runs its checks again
     when the channel opens, calls C21), shown with its code; the call has ended (§7.8).
   An event larger than the channel's message size arrives as `lmgw.event.part` messages and is
   reassembled before it is parsed (§7.10), so `session.created` always arrives whole.
6. **Bound:** from the trigger to `session.created` or `lmgw.session.bind_pending`,
   `voice.connect_timeout_s` (default 15). Past it the attempt is given up: the PeerConnection is
   closed, a call id is kept as stale (§7.8), and the screen says "lmgw did not start the voice
   session within 15 s (voice.connect_timeout_s)", adding, when the cue had played, that nothing
   said since was answered. A pending bind waits for the owner, not for this bound, and lmgw's own
   bound ends it after one `realtime.ping_interval_s` (§7.9).

*Changed 2026-10-15 (WP3 part 1, as built):*
- **The API** splits §3.2's `open`: `Voice::start(CallStart, EventSink)` is the trigger (it answers
  at once with the state's words when voice cannot start: not paired, no network, the owner's Confirm
  owed, a refused or lost key, a lapse, the folder's steps, a call running; a link that is only
  reconnecting holds nothing back, so the call never waits for the feed) and `Voice::offer(sdp)` the
  POST, which answers lmgw's SDP or the refusal (the call has ended then). `EventSink` gains
  `close_peer` (the core decides when the PeerConnection closes) and `cue`; `Voice::transport` takes
  libwebrtc's own name for the state, for "connection lost (<state>)"; `Voice::stop` is the screen's
  stop button beside `tap`; `Voice::capturing` is the device module's first samples after `Off`.
  *Changed 2026-10-15 (WP3 part 1's review):* `Voice::start` answers the call's number, and every
  libwebrtc event carries it (`transport`, `channel_open`, `channel_message`, `channel_closed`,
  `capturing`, `stats`, `failed`): the core drops an older call's, whose PeerConnection may still be
  closing when Reconnect or Take over starts the next call (libwebrtc reports `closed` as `close()`
  begins, which ended the new call at once). `Voice::failed(call, words)` ends a call the phone cannot
  carry on, in those words (`EndReason::Failed`, with Reconnect, its id hung up): libwebrtc's failed
  `setRemoteDescription` (which waited for the connect bound before), `createOffer` or
  `setLocalDescription`, a message the open channel refused, and Android's refusal of the microphone
  type (§8.6).
- **The link's ticket.** The trigger asks the link's actor for what the call needs (the pinned client,
  the URLs in the race's order, the URL that last answered, the Kai folder, who else holds the thread's
  voice); the voice task owns the call and gets the conversation's thread, who else answers in it and
  the hold as they change.
- **The base URL** is the one that last answered the feed, else the warm-up's race winner. The warm-up
  (`Voice::prepare`, and again at every start) is `GET /v1/realtime/calls`, which the device listener
  answers 404 without a handler: TCP, TLS and the Keystore signature, nothing read or written on lmgw.
  The POST waits for it (bounded by `lmgw.timeout_s`) so it takes that connection. *Changed
  2026-10-15 (WP3 part 1's review):* the warm-up itself is bounded as a whole by `lmgw.timeout_s` (its
  request's answer and body were each bounded, so it could take twice that), and the POST's wait by
  the same setting, with no extra second. Only connections are raced, never the POST, so no offer
  reaches lmgw twice; a POST that cannot connect (the network changed) races the other URLs once
  (§7.11).
- **No folder known** (the first call after pairing, before any lookup) looks it up by §6.1's rule
  before the POST, as a 404 `chat_folder_not_found` does before its one retry; the conversation's own
  lookup runs again beside it.
- **Every refusal** of the POST ends the call with lmgw's words and code; a key refusal (401, or
  `device_unconfirmed`) reaches the link as a request's refusal does (§5.5).

### 7.4 What the `session` part carries

The thread owns model, prompt, tools, ASR, TTS and voice (chat-voice §8.1), and with the bind by
folder it owns the call's turn mode too: lmgw reports it as `session.lmgw.turn_mode` and, for
`automatic`, applies the thread's turn detection itself, or `semantic_vad`. The `session` part
carries only what the client owns or the owner overrides, and lmgw applies it as the session's
first update. *Shared.*
- **The turn mode only as the owner's override** (`voice.mode`). At `thread`, the default, the part
  names no mode and the phone takes it from `session.lmgw.turn_mode`. Always push-to-talk sends
  `session.lmgw.turn_mode: "push_to_talk"`; always automatic sends `"automatic"`, for which lmgw
  applies the thread's own detection, or `semantic_vad` where the thread is set to push-to-talk
  (calls §2.2). The phone derives no detection rule, sends no `turn_detection` and reads no
  `voice_resolved`.
- **Half duplex:** `session.lmgw.half_duplex: true` and `echo_tail_ms` when `voice.echo` is
  `half_duplex` (§8.2).
- **During a call** the voice screen's mode switch sends a `session.update` naming
  `session.lmgw.turn_mode` (`"push_to_talk"` or `"automatic"`), which lmgw resolves as at the bind,
  so the phone keeps no detection of its own to restore. Nothing else sends one.
- *Changed after the owner's answers, 2026-10-14:* the override and the switch name
  `session.lmgw.turn_mode`, as calls v3 lets a bound client do, instead of a `turn_detection` the
  phone chose.

*Changed 2026-10-15 (WP3 part 1, as built):* the part goes only when it carries something (the
override, or half duplex with its tail); with neither the offer goes as `application/sdp`, which lmgw
takes alike (calls §2.1) and spares a first update that changes nothing. A mode switch before
`session.created` or while a bind is pending is held and sent right after `session.created`.
`voice.echo`'s `half_duplex` is part 2's (§8.2): today's setting has no such mode, so no part carries
`half_duplex` yet.

*Changed 2026-10-15 (WP3 part 2, as built):* `voice.echo` `half_duplex` sets the call's
`Facts.half_duplex` at its start: the `session` part carries `session.lmgw.half_duplex: true` and
`echo_tail_ms` when `voice.echo_tail_ms` is set (`{"lmgw":{"echo_tail_ms":250,"half_duplex":true}}`),
and the voice screen says that speech does not interrupt Kai while the tap does (`VoiceView::half_duplex`).

### 7.5 Automatic, push-to-talk, mute, K16

- **The microphone is open for the whole bound call.** libwebrtc captures from the call's audio
  start to its end, so Android's microphone indicator stays lit for the call, whatever the mode.
  The one exception is a pending bind, where the microphone is `Off` (§7.9). A disabled track does
  not stop the stream: it sends RTP with silence. So "disabled" below (`Silent`) means lmgw
  receives silence, not the room. Mute and push-to-talk stay on silence rather than `Off`: an open
  turn ends on silence in lmgw's detector, and a press must be heard at once, which a capture
  restart would clip.
- **Automatic:** the track is enabled and lmgw's detector ends turns, with the detection lmgw
  applied for the thread or the owner's override (§7.4).
- **Push-to-talk:** press and hold the ring (or the headset's button, §8.4). At the press the core
  sends `input_audio_buffer.clear` and enables the track. At the release it sends
  `input_audio_buffer.commit` and `response.create` at once: lmgw's commit waits its window
  (`webrtc_commit_window_ms`, lmgw's setting, default 120, the value measured in lmgw's WP6) and
  takes every frame stamped up to the commit plus that window, so the commit cannot overtake the
  turn's last frames (calls C8). The track stays enabled for `voice.ptt_tail_ms` after the release, so
  audio captured before the release but still in the phone's capture, echo-cancelling and Opus
  pipeline is sent rather than zeroed; then it is disabled. WP0 (g) measures that pipeline and sets
  the default; the setting shows it.
- **The tail and the window are two settings on two machines, and the tail must fit inside the
  window:** audio the tail carries that is stamped later than the commit plus the window misses the
  turn. The phone cannot read lmgw's value, so the settings page shows `voice.ptt_tail_ms` with the
  rule beside it: a tail longer than lmgw's window (120 ms unless the owner changed it) needs
  `webrtc_commit_window_ms` raised on lmgw, or the turn's last syllable is lost. WP0 (g) compares
  the measured pipeline with lmgw's window, and if the tail must be the longer, the window is raised
  before WP3. The phone never shortens its tail on its own.
- **The cut on the phone's clock, decided after WP0 and lmgw's interop rig.** The tail and the
  window stay for now. The alternative: `lmgw.input_audio_buffer.commit {captured_until_ntp_ms}`
  at the release and `lmgw.input_audio_buffer.clear {captured_from_ntp_ms}` at the press, each
  carrying the phone's wall clock at that moment, the clock its RTCP sender reports map onto the
  RTP timestamps. lmgw would cut the turn at the captured instant, however long the phone's
  pipeline is, so the tail would become a phone-only concern (keep sending until the audio captured
  up to the release is out) and the rule that the tail fit inside the window would go (Later).
- **Mute** disables the track, so an open turn ends naturally (kai §5.4). Muted, push-to-talk
  presses are ignored and the ring says so. Muted time counts toward `voice.idle_end_s` (as kai
  ruling 32 for the desktop's timeout).
- **K16:** while another principal's turn runs in the bound thread (the feed's `turn.started` by
  someone else, until its `turn.done`), automatic mode disables the track and the ring says "waiting
  for <by>"; a push-to-talk press still talks.

*Changed 2026-10-15 (WP3 part 1, as built):*
- **`voice.ptt_tail_ms` is not set** until WP0 (g) measures it (no value is guessed in its place), and
  then push-to-talk keeps no tail: the track goes silent at the release. The limit surfaces: the voice
  screen says, in push-to-talk, that no tail is kept and the last syllable may be cut, and the settings
  page shows the key with the rule beside it.
- **Before `session.created`** a press (the override is push-to-talk) turns the track on but sends no
  `input_audio_buffer.clear` (nothing may go before lmgw's first word, and lmgw's buffer holds the
  press's own start); its release's commit and `response.create` go right after `session.created`, and
  are dropped if a pending bind comes instead.
- A press in automatic mode, or while muted, is ignored (the ring says muted); muting while a press is
  held releases it (its turn is committed).
- *Changed 2026-10-15 (WP3 part 1's review):* a press always gets its release, also when its gesture
  is cancelled before the finger lifts (the call ends, the mode changes, a fold recreates the
  activity); the ring's gestures follow the call and what they do, not the phase, so the bind does not
  restart a held press.

### 7.6 Playback, stopping, tools

- **Playback** is libwebrtc's: Opus over RTP into its jitter buffer and the communication stream.
  `voice::Machine` takes its player facts from `output_audio_buffer.started` (audio in the player),
  `.stopped` (played out) and `.cleared` (cut) (P2). What lmgw takes as heard comes from the
  phone's playout time where the phone reports one, else from what lmgw sent (calls C11).
- **Playout reports** (*shared*). While Kai's audio plays (from `output_audio_buffer.started` to
  `.stopped` or `.cleared`), the core asks Kotlin for the remote track's stats every
  `voice.playout_report_ms` (default 1000) and sends
  `lmgw.output_audio.playout {playout_ntp_ms, jitter_buffer_ms}`:
  - `playout_ntp_ms` is `estimatedPlayoutTimestamp`, the NTP time of the audio playing now on
    lmgw's own clock, which libwebrtc knows from lmgw's RTCP sender reports;
  - `jitter_buffer_ms` is the growth of `jitterBufferDelay` over the growth of
    `jitterBufferEmittedCount` since the previous report (the running mean for the first);
  - a missing stat leaves its field out, and a report with neither is not sent.
  lmgw uses them for drained and for the barge-in window. When none has arrived for one
  `realtime.ping_interval_s` while audio played, lmgw considers the reports stopped and falls back
  to its formula (calls C10), so `voice.playout_report_ms` stays well below that interval (§3.4).
  WP0 (l) checks that the LiveKit build reports both stats.
- **Stopping Kai** (*shared*): a tap on the ring while a response is open (thinking, speaking, a
  tool), a push-to-talk press, End.
  - The core mutes Kai's remote track at once (`set_playback(false)`: libwebrtc sets that stream's
    output volume to zero, so the rest of the jitter buffer is never heard), reads the stats, and
    sends `lmgw.response.stop {playout_ntp_ms}` with the `estimatedPlayoutTimestamp` read right
    after the mute. Without that stat the field is left out, and lmgw takes heard = sent.
  - The event is idempotent, cancels whatever is open and never answers
    `response_cancel_not_active`, so the phone sends it without asking `Machine::current()` first:
    a response that ended a moment before costs nothing. On Kai's calls it replaces the old order
    of `response.cancel` then `output_audio_buffer.clear`.
  - A tap that stopped Kai is consumed and does not also mute (kai K6). The local mute lifts at the
    next `output_audio_buffer.started`.
- **Speech over Kai's voice** (automatic, full duplex): lmgw's detector and its gate decide, and
  the verdict is `output_audio_buffer.cleared` (calls C12), not an event of its own. lmgw sends it
  only once it has stopped sending a response's audio, whatever stopped it, so the core mutes Kai's
  remote track on every `cleared` as the stop does (lmgw has cut the reply; the phone only silences
  what is already in its jitter buffer). The cancelled `response.done` just before it says why:
  with `status_details.reason` `turn_detected` the ring shows interrupted, then listening; after
  the phone's own stop (`client_cancelled`) the track is muted already. The mute lifts at the next
  `output_audio_buffer.started`. `input_audio_buffer.speech_started` changes nothing in the phone's
  playback: lmgw also sends it where nothing is cut (a response that had played out, speech outside
  the playing window).
- **End** sends `lmgw.response.stop` as above (not on a pending bind, where nothing is open), then
  closes `oai-events`. lmgw applies every event it received before the close, the stop included,
  so the thread keeps what was heard, and then ends the call. No wait for `response.done`, and no
  hangup over HTTP. Kotlin sets the microphone `Off` at once, abandons focus, releases the
  MediaSession, restores `MODE_NORMAL`, clears the communication device and drops the service's
  `microphone` type (the service itself goes on holding the feed, M16). It closes the
  PeerConnection once the channel reports closed (its stream reset completed, so lmgw has
  everything sent before it) or lmgw has ended the call, at most `lmgw.timeout_s` later, logged
  with the setting's name when that runs out: closing it at once could drop a stop still in flight.
- **Half duplex:** speech does not interrupt; the tap does.
- **A tool turn:** `lmgw.output_audio.starved {response_id, starved}` (bound calls only) feeds
  `Machine::starved(response_id, starved)`: `true` at the first padded tick while a tool runs after
  a spoken preamble, `false` at the next real frame. The ring shows thinking instead of a silent
  speaking state (P2, WP0 (h)).
- **Captions:** the owner's words from `TranscriptDelta` and `Transcript`, Kai's from `SpokenDelta`;
  a `lmgw.chat.reply` replaces Kai's caption with what the thread kept (a cut's heard part). The
  overlay shows the last two lines, the voice screen scrolls.

*Changed 2026-10-15 (WP3 part 1, as built):*
- **Nothing overtakes the stop.** While the stop waits for its stats, every later event (a press's
  clear, its commit and `response.create`, End's close) waits behind it, so a `response.create` never
  reaches lmgw before the stop that would drop it (calls §6.5). The wait is bounded by `lmgw.timeout_s`;
  past it the stop goes without a position (heard = sent) and the log says so with the setting's name.
  *Changed 2026-10-15 (WP3 part 1's review):* the PeerConnection's wait for the channel's close (also
  `lmgw.timeout_s`) counts from the close as asked, never from End, so End sends its stop (bare when
  the stats never come) and then the close, and the PeerConnection closes at most twice
  `lmgw.timeout_s` after End; before, it closed at the stop's own bound and dropped both. The stop's
  mute, its stats read and the microphone's off go before the end is said (Kotlin drops the call's
  audio there).
- **A tap stops** while a response is open or a committed turn waits for its response (lmgw's stop
  cancels the owed response too, calls §6.2); otherwise it toggles the mute.
- **Playout reports** start one interval after `output_audio_buffer.started` and stop at a stop's tap,
  at `cleared` and at `stopped`.
- **The tool** a turn runs (the `tool` frame's `start`/`ready` to its `result` or the turn's `done`)
  is named on the state line ("Running lmgw__status") and feeds `Machine::set_tool`, so `starved` reads
  as thinking.
- **Approvals in a call** (`mcp_approval_request` on the channel) are WP5's card; part 1 logs them,
  and the feed's approval (WP2's card and notification) stands. A late task result's continuation
  (`lmgw.task.done`) is not built yet either; the event is logged.

*Changed 2026-10-16 (WP5, as built):*
- **Approvals in a call** (`voice/call/approvals.rs`; the book's side in `approvals/call.rs`): the
  session's `mcp_approval_request` goes to the book as a card (its thread the call's, no message id
  until the feed's copy names it); the first time an id comes in this call, `EventSink::asked` plays
  the ask (§10.5). While a call of the bound thread waits with no verdict sent, the state line says
  "Kai waits for your decision on <tool>" (the ring stays listening: the microphone is not held back,
  §11) and the idle clock stands still. A reply whose every call this session showed is decided
  through it: one `mcp_approval_response` each, then `response.create` with an `event_id`
  (`kai_approvals_<call>_<n>`). *Changed 2026-10-16 (WP5's review):* what lmgw answers is read
  three ways, since lmgw writes the decision inside the resumed turn and anything after the write can
  still fail:
  - **not taken:** an `error` about them (echoing that create, or with an `approval_*` code) whose
    code says lmgw refused before deciding (`key_rate`, or an `approval_*` code other than
    `approval_decided`): lmgw's words go on the card, and it asks again;
  - **unsettled:** such an `error` with any other code (`approval_decided`, `gpu_hold`, an upstream
    timeout, a model failure), the response after the create failing, cancelled (a stop) or
    completing without output (no spoken delta, no audio, no chat frame), or the call's end before
    any of these: the verdicts go again by the route, whose answer settles them (its 409
    `approval_decided` means lmgw had taken them, closing the card quietly; its success, that it
    decided them now);
  - **probably taken:** that response completed with output. The phone cannot tell it from the
    owner's turn: lmgw holds a create while the owner speaks, and a turn with words is a new message,
    which declines what waits, the verdicts going unused (`lifecycle::bound`). So the card stays
    "sent" until the feed's `approval.decided` (or its withdrawal) closes it, and when lmgw decided
    otherwise than the verdict sent the owner is told in a notice, once (§11's note).

  Before, any echo or `approval_*` code and the response's failure meant not taken, and the response
  running (a stop's cancel included) meant taken and closed the card at once: approving while still
  talking closed it as taken although the owner's words had declined it, and a stop or End between
  `response.created` and the decision's write said taken and hid the card for hours.
  `lmgw.approval.decided` closes the card. lmgw's words on an approval's `error` are never logged
  (they name the tool): the code alone is.
- **The late task result's continuation** (`voice/call/continuation.rs`): at `lmgw.task.done`, a
  turn of the owner's under way (their speech, a press, a commit in flight) or another principal's
  turn answers the result (its message goes after the result) and nothing is sent. Otherwise
  `response.create` with no new words and an `event_id` (`kai_continue_<call>_<n>`) goes at once,
  or, while a response is open (Kai thinks, speaks or runs a tool), another app holds the audio, a
  stop or a push-to-talk tail is in flight, or a call of the thread waits for a verdict (a
  continuation's reply is a new message, which would decline it), once that clears, unless a turn of
  the owner's has answered it meanwhile. An echo with `empty_turn` or `turn_running` is logged, not
  shown; any other is the call's error. *Changed 2026-10-16 (WP5's review):* a response merely open
  no longer answers it. A result enters the thread only while no turn runs, so one that ended while
  Kai spoke entered after that turn's request: lmgw says `lmgw.task.done` once the turn ended, while
  the phone may still play its audio, and that reply never answered it. Where a turn did answer it
  after all, lmgw's `empty_turn` says so, quietly. (`lmgw_client::realtime`'s module doc still says
  a `TaskDone` during a response is answered by it; lmgw's own `realtime::thread::tasks` says the
  result enters after the turn.)

### 7.7 How a call ends

| End | Trigger | Words |
|---|---|---|
| End | the ring's, the notification's, the overlay's or the headset's End | none |
| idle | `voice.idle_end_s` after the last reply played out with nothing open: no response, no press, no pending approval, no speech; a pending bind counts as quiet | "voice ended after 30 s of quiet (voice.idle_end_s)" |
| dismissed early | the overlay dismissed before the first committed turn (§9.2) | none: an accidental invocation leaves nothing open |
| declined | End or dismissal while a bind is pending (§7.9) | none |
| unanswered | lmgw ended a pending bind after one `realtime.ping_interval_s` (`thread_bound`, §7.8) | "Voice stayed on <by>" with Take over |
| rebind | New conversation or `folder.current` | the call ends at the next quiet point (no open response, nothing playing, no press) and a new one binds by the folder, with `new=true` after New conversation; an in-place rebind is decided after WP3 (Later) |
| lmgw's end | §7.8 | per reason |
| audio taken | a permanent audio focus loss (§8.5) | "voice ended: another app took the audio" |

*Changed 2026-10-15 (WP3 part 1, as built):*
- **Rebind.** A conversation that moves off the call's thread after the bind (`folder.current`, or a
  thread the conversation follows) ends the call at the next quiet point (nothing open, nothing
  playing, no press, no stop in flight) with `EndReason::Rebind`, and `CallEnd::restart` makes Kotlin
  start the next call by the folder once the old PeerConnection is closed, keeping the microphone type
  and the call's audio meanwhile. New conversation during a call asks `current {new: true}` as outside
  one, so the next bind needs no `new=true`: the folder's current thread is the new one already. The
  old call's id is hung up over HTTP before the new POST, so the new bind is neither refused nor left
  pending by the phone's own binding.
- **Idle while pending** declines with the idle's words (a pending bind counts as quiet).

*Changed 2026-10-15 (WP3 part 2, as built):* **audio taken** is `EndReason::AudioTaken`, by the
stop and the channel's close as the owner's End, with no action of its own: Talk starts the next
call (a stale id is hung up first as always). A permanent loss before lmgw's first word gives the
start up, its id hung up before the next bind.

*Changed 2026-10-15 (WP4, as built):* **dismissed early** is the core's rule, `Voice::dismissed(call)`
(`voice/call/dismiss.rs`), never the overlay's: the overlay's view can be a moment behind a commit,
and the core sees every event in order.
- **"The first committed turn"** is the owner's first turn that reached the thread with words: a
  final transcript that is not blank, or a reply Kai completed. A commit of noise or silence (a blank
  or failed transcript, lmgw's `empty_turn`) is no turn: it is what an accidental press in a pocket
  makes. `VoiceView::spoken` says whether it came.
- **Turns under way** at the dismissal decide by their words, and they are counted. A turn opens
  when lmgw sends `speech_started` (or a push-to-talk release asks for the commit), becomes a commit
  in flight at `committed`, and is resolved by its transcript (or a failed transcript or reply), or
  by `cleared` when the speech never became a commit. The ring held at the dismissal is a turn
  under way too, opened by its release. With the first words the call goes on; the call ends as
  dismissed once no turn is open and none had words, so a cough after a word-less dismissal ends it,
  and two commits in flight need both transcripts. A dismissal waiting on a turn relies on lmgw
  always resolving a committed turn (a transcript, a failed transcript or a failed reply); a turn
  lmgw never resolves keeps the call until the owner's End, the idle timeout or lmgw ends it.
- **Before the bind** a dismissal ends the start as End does (its id hung up before the next bind);
  **while a bind is pending** it declines (§7.9). The end is `EndReason::Dismissed`, without words or
  action; the ending earcon plays as at any end the owner caused.
- **Only the overlay's own call:** a call the app started, or one Expand handed to the app, is never
  ended by the overlay's dismissal (§9.2's note): the invocation did not open its microphone.

*Changed 2026-10-16 (WP5):* the idle row's "no pending approval" is a call of the bound thread
waiting for a verdict with none sent, whichever source brought it (the book's, §11's note); while one
waits the idle clock stands still.

### 7.8 Why lmgw ended a call

*Shared.* OpenAI's WebRTC carries no close reason; on Kai's calls lmgw sends one (ruling 5). The
last event on a bound call before lmgw ends it is `lmgw.session.closed {kind, by, message}`, and
the phone reads its end reason from that event alone, through `lmgw-client`'s `CloseKind` (P2),
which gains `ThreadBound`; `closed` and a kind this build does not know map onto `Other` (calls
C17). The words come from `kind` and `by`; `message` is lmgw's sentence, shown under them and
never parsed, except that a `revoked` message's leading token is the `RevokeKind`, which
`lmgw-client` reads as it reads a 4003's reason.

| `kind` | Reason | Words and action |
|---|---|---|
| `taken_over` | another client took the voice | "Voice moved to <by>" with Take over (§7.9) |
| `thread_bound` | the pending bind was left unanswered for one `realtime.ping_interval_s` (calls C21); `by` is the holder | "Voice stayed on <by>" with Take over (§7.9) |
| `revoked` | the device's key was revoked, disabled or expired, or its certificate lapsed (`message` led by the kind's token, `certificate_expired` among them) | §5.5's words by the token; for a token the core does not know, "lmgw refused this device" until the feed's `revoked` or the next request's 401 names the kind |
| `out_of_reach` | the thread was deleted or left the device's reach | "the conversation's thread is gone" with Reconnect, which binds the folder's current thread |
| `closed` | lmgw ended the call for a reason of its own (a stall, the owner on the dashboard), named in `message` | "lmgw ended the voice session" with Reconnect |
| `shutting_down` | lmgw is stopping | "lmgw is shutting down"; §5.3's state follows from the feed |
| `shutting_down`, a device call ended because the device listener changed (turned off, or its scope or port moved; calls §3.1, §8.2) | lmgw's device access changed | "lmgw's device access changed" with Reconnect; *Changed 2026-10-14 (consistency pass):* the same kind as a stop, told apart by the cause |
| any other `kind` | a newer lmgw | "lmgw ended the voice session" with Reconnect; the kind is logged |
| none: the transport failed, or the channel closed without the event | the network, or lmgw gone without a word | "connection lost (<libwebrtc's state>)" with Reconnect; §5.3's state line says the rest |

- **No wait and no feed.** The reason comes on the call, before lmgw ends it, so the phone never
  shows "voice stopped" while it waits for a word from elsewhere. The feed's `voice.ended` and
  `revoked` are not read for the phone's own call (M16).
- **Reconnect** hangs up the stale call id over HTTP first: the hangup takes no slot, so it works at
  the key's limit and frees a dead call's slot and its hold on the thread. Then it binds by the
  folder with `takeover=ask`. Only a call that ended without `lmgw.session.closed`, or a start given
  up after its POST (§7.3), leaves a stale id; lmgw ended the others itself. A 404 `call_not_found`
  to the hangup means lmgw ended it already and is as good as a 200. A takeover in between shows as
  a pending bind, a revocation as 401, a stopped lmgw as no connection.
- **No automatic reconnect** (kai K5): re-opening warms models in an lmgw that was just restarted,
  and a silent reconnect would keep the microphone open without the owner knowing why.

*Changed 2026-10-15 (WP3 part 1, as built):*
- **`shutting_down`** reads "lmgw is shutting down, or its device access changed" with Reconnect:
  lmgw as built sends the same kind and the same sentence for its stop and for a device listener that
  moved (calls §3.9, §8.2), so the phone cannot tell the two apart at the call's end; the connection's
  state line says which (the feed comes back on a listener change).
- **A refusal at the open** (an `error`, then `closed`) reads "lmgw refused the voice session: <message>
  (<code>)"; any other `closed` reads "lmgw ended the voice session", lmgw's sentence under it.
- **`revoked`** also tells the link, which stops as the feed's `revoked` would (§5.5).
- **Stale ids** are hung up before the next call's POST (Reconnect, Talk, a notice's Take over alike):
  a call lost without lmgw's word, a start given up after its POST (End before the channel opened, the
  connect bound), a rebind's old call, and a call replaced while its PeerConnection was still closing.
  *Changed 2026-10-15 (WP3 part 1's review):* also a call whose PeerConnection closed while ending
  without the channel's close or lmgw's word (the close's bound, a failed connection), which could
  leave the phone's own binding and prompt "Voice is on device '<this phone>'" at the next Talk; lmgw's
  `closed` while the phone's End is under way counts as its word. Every call lmgw may still hold is
  owed a hangup from lmgw's answer on (the live call's too), kept in `calls.json` (§3.3), so a process
  that dies mid-call hangs it up before the next POST; all owed ids are hung up, oldest first (one
  whose hangup was refused or not answered is tried again at the next call), and a clean end (lmgw's
  word, the channel's close) or an answered hangup clears one. Before, one id was kept in memory, and
  an older call's late answer replaced it.

### 7.9 Takeover

- **The phone never takes over unasked** (M11). Every call binds with `takeover=ask`. When another
  client holds the Kai thread, the call connects and lmgw sends
  `lmgw.session.bind_pending {held_by}` instead of `session.created`. The ring, or the overlay,
  shows "Voice is on <held_by>" with **Take over** and End, from `held_by` ("device 'desktop'", or
  "the dashboard" for the owner), never from a sentence.
- **While pending the phone sends nothing but `lmgw.session.take_over`, or hangs up.** lmgw answers
  every other client event with an `error` of code `bind_pending` and drops it (calls C21); the
  core holds back the mode switch, playout reports and approvals until `session.created`, and an
  `error` `bind_pending` that arrives anyway is logged as a bug, not shown.
- **Take over** sends `lmgw.session.take_over` on the same call. `session.created` follows, and the
  desktop's session closes with 4000 naming the phone (api §1.7). There is no second call setup:
  the tap costs one round trip on the data channel. When the holder ends while the prompt shows,
  `session.created` arrives without a tap and the prompt turns into listening. When a third client
  takes the thread meanwhile, a new `bind_pending` names it, and the prompt follows.
- **Declining** (End, dismissing the overlay, or `voice.idle_end_s` passing, for which a pending
  bind counts as quiet) hangs up: the phone closes `oai-events` and the PeerConnection. Nothing was
  bound, so nothing else is sent. lmgw's own bound comes first with its defaults (one
  `realtime.ping_interval_s`, 20 s, against the phone's 30): the call then ends with
  `lmgw.session.closed {kind: "thread_bound", by}`, and the phone shows "Voice stayed on <by>" with
  Take over (§7.8).
- **The microphone while the bind is pending: off.** lmgw discards inbound audio until the bind, so
  at `bind_pending` the core sets the microphone `Off` (the sender's encoding inactive: libwebrtc
  stops capturing and sends no RTP, and the microphone indicator goes out), and at
  `session.created` sets it `Live` again. The listening cue then waits for the first captured
  samples (the device module's callback, §8.1), so nobody speaks into a microphone that is not yet
  capturing.
  - *Why off, not kept running:* capture would buy nothing while lmgw discards it, except skipping
    the restart after the tap. Against that, a lit indicator under a prompt that says Kai is not
    listening is a false signal; a declined prompt (the owner meant to talk on the desktop) would
    have had the phone's microphone open for nothing; and the room's audio would cross the network
    only to be dropped.
  - *The cost:* the restart's time, from `session.created` to the first captured samples, added to
    every start that meets the prompt. With Q3's answer (the desktop takes its voice back after a
    phone call) that is nearly every phone start while the desktop listens. WP0 (j) measures it; if
    it is long enough to be felt, keeping capture on is the alternative, at the price above, and
    is decided on the number.
  - Mute and push-to-talk keep `Silent` instead (§7.5).
- **Expecting the prompt.** When the feed shows another client holding the Kai thread's voice as a
  call starts, the core sets the microphone `Off` before the answer is set and plays no cue at
  DTLS-connected; it waits for lmgw's first word on the channel. When `bind_pending` arrives after
  the cue played (the feed did not know yet, or was not up), capture stops and the prompt says that
  what was said since the cue was not heard.
- **Take over from an ended call's words** ("Voice moved to <by>", "Voice stayed on <by>", or the
  notification's action) starts a new call that sends `lmgw.session.take_over` as soon as
  `bind_pending` arrives, since the owner has already chosen (`CallStart`); if the bind succeeds
  directly, nothing is sent.
- **Afterwards the desktop takes its voice back** (Q3's answer, a change to Kai desktop's K5, none
  in lmgw): when the phone's call ends with `voice.ended {reason: closed}` (End, the idle end, or
  the phone gone) and desktop Kai was listening before the takeover, it rebinds with
  `takeover=never`; not after another takeover or a revocation. *Changed after the owner's answers,
  2026-10-14:* v3 left the desktop idle until its talk key (kai §5.6).
- **The desktop takes it back during a call** (its talk key binds with a takeover): the phone's
  call gets `lmgw.session.closed {kind: "taken_over", by}` and ends, and the phone shows "Voice
  moved to <by>" with Take over: in the overlay if it is visible; otherwise the service's
  notification goes back to the link's state and a normal notification (channel `notices`) carries
  the same words, whose Take over opens the voice screen, which starts the call (§12.3).
- **Friction, accepted:** while desktop Kai is listening (kai K11's note of 2026-10-08), every
  phone start meets the prompt, on a call that is already connected. A phone setting that takes
  over without asking would remove the prompt and the microphone's restart together; it is not in
  this design.

*Changed 2026-10-15 (WP3 part 1, as built):* the delayed cue after a pending bind (or an
expected one) plays at the device module's first `SamplesReadyCallback` after the microphone came back
from `Off`; until then the ring shows "Starting the microphone…". A new holder's `bind_pending` asks
about the new holder (the prompt's Take over is offered again). An ended call's Take over is
`CallStart { take_over: true }`. Whether the inactive encoding really stops libwebrtc's capture (the
indicator out) is WP0 (j)'s, and an acceptance check covers it.

### 7.10 Hold and errors

- **`gpu_hold`** on a turn: the amber badge; `lmgw.model.state {state: held}` names the stage and
  alias. When a fallback answered, `answered_by` in `lmgw.response.timing` names it and the caption
  line says "answered by <alias> (fallback)"; when the stage has none, the turn's error says that
  local models are paused and this alias has no fallback (ruling 13: no retry).
- **Oversized events.** On Kai's calls an event larger than the peer's `max-message-size` travels as
  `lmgw.event.part {id, index, count, data}` messages, in both directions. `lmgw-client`
  reassembles lmgw's parts before parsing and splits what the phone sends to lmgw's size (P2, P7),
  so no event is shortened. A part sequence that cannot be completed is an error shown with its
  `id` and logged; the call goes on. `event_too_large` with emptied fields applies only to unbound
  calls, which Kai never makes; one that arrives anyway is shown like any other error.
- **Any other `error`** is shown verbatim with its code; repeats are counted in the log.

*Changed 2026-10-15 (WP3 part 1, as built):* every `error` is also a caption note, the latest in
`VoiceView::error`; repeats of a code are counted in the log. A held stage (`lmgw.model.state {state:
held}`) shows as "Paused: <stage> <alias> (<cause>)", the hold as the connection's badge, and a
fallback that answered as the caption's "answered by <alias> (fallback)".

### 7.11 Network changes

*Changed after the owner's answers, 2026-10-14:* a call may now run over the LAN or the tunnel (M5).
lmgw's candidate is fixed for the call's life (no renegotiation or ICE restart on lmgw's side), so a
call survives a network change only if a new local candidate still reaches it:
- **A call over the tunnel URL.** WireGuard keeps the tunnel's inner addresses when the phone moves
  between Wi-Fi and mobile data, so the VPN candidate's pair stays the same and only WireGuard
  re-handshakes; libwebrtc's continual gathering adds candidates on the new network, and lmgw
  answers checks from a new source as a peer-reflexive pair (calls §3.6). lmgw keeps a call through
  any pause shorter than its `realtime.ping_interval_s` (calls C16).
- **A call over the LAN URL** (at home). Leaving the Wi-Fi drops its candidate; the call lives on
  only if the tunnel comes up and routes lmgw's LAN address (a profile whose allowed addresses
  include the home LAN). Otherwise it ends as "connection lost", and Reconnect POSTs to whichever
  URL answers then.
- Unverified: how libwebrtc's monitor reacts to the change, and the gap's length in each case. WP0
  (f) measures both.

*Changed 2026-10-15 (WP3 part 1, as built):* libwebrtc's ICE gathering is continual
(`GATHER_CONTINUALLY`), so a new network adds candidates to the running call; a connection that
pauses (`disconnected`) is a note on the voice screen, not an end; one that fails ends as "connection
lost (failed)" with Reconnect, which POSTs to whichever URL the link's race found since.

## 8. Audio

### 8.1 The device module

`JavaAudioDeviceModule.builder(context)`:
- `setAudioSource(VOICE_COMMUNICATION)`, `setAudioAttributes(USAGE_VOICE_COMMUNICATION,
  CONTENT_TYPE_SPEECH)`, mono in and out;
- `setUseHardwareAcousticEchoCanceler(hw)` and `setUseHardwareNoiseSuppressor(hw)` per §8.2;
- `setSamplesReadyCallback` for the microphone level (§10.3), and for the first captured samples
  after a restart, at which a delayed listening cue plays (§7.9).

It is created with the factory ahead of a call (§7.2) and rebuilt only when `voice.echo` changes.
Recording starts with a call's audio and stops at its end, and also stops while a bind is pending:
the sender's encoding is set inactive, which stops libwebrtc's capture, and active again at
`session.created` (§7.9). WP0 (j) checks that the indicator goes out and measures the restart.

*Changed 2026-10-15 (WP3 part 2, as built):* the device module also has
`setPlaybackSamplesReadyCallback`, the libwebrtc build's copy of what it plays: Kai's level for the
ring (§10.3). Both callbacks reduce each 10 ms buffer to one RMS level and keep nothing.

### 8.2 Echo cancelling

| `voice.echo` | Device module | Session |
|---|---|---|
| `hardware` (default where `AcousticEchoCanceler.isAvailable()`) | hardware AEC and NS on | full duplex |
| `software` (default otherwise) | hardware AEC and NS off, WebRTC's APM with AEC3 | full duplex |
| `half_duplex` | as `software` | `half_duplex: true`, `echo_tail_ms` |

- Some phones report a hardware canceller that does not work; the setting is the usual remedy, and
  its description says so ("Kai interrupts itself on speaker: try software echo cancelling").
- A change applies from the next call; the setting says so.
- There is no `device` or `not_needed` mode as on the dashboard (chat-voice §12.1): the phone always
  cancels on the phone, and a headset needs nothing different.
- WP0 (d) counts false barge-ins on the speaker at three volumes with `hardware` and `software`.
  *Changed 2026-10-15 (WP0 spike build):* AEC3 cancels what libwebrtc itself plays (its far-end
  reference is its own playout), so speech played through `AudioTrack` cannot test `software`. The
  spike measures `hardware` (VOICE_COMMUNICATION with AcousticEchoCanceler) against raw capture, with
  an NLMS filter over the known clip as a stand-in; the `software` column runs through (c)'s peer,
  which sends the clip as the call's remote track.

*Changed 2026-10-15 (WP3 part 2, as built):* `half_duplex` is built: the device module as
`software`, the session as §7.4's note says. The factory and its device module are rebuilt when the
canceller the setting asks for changed since they were built (`software` and `half_duplex` differ in
the session alone), at the next `prepare`, the settings page's save, or a call's start, and never
while a PeerConnection made by them lives: a change during a call waits for the next one, as the
setting's text says ("A change applies from the next call"; before, the next process).

### 8.3 Routing

- At a call's start: `MODE_IN_COMMUNICATION`; from `getAvailableCommunicationDevices()` the first
  of a Bluetooth headset (SCO, LE audio, hearing aid), a wired or USB headset, else `voice.route`
  (speaker by default; earpiece selectable); `setCommunicationDevice`.
- `addOnCommunicationDeviceChangedListener` follows changes (a headset connected mid-call takes
  over; one removed falls back to `voice.route`).
- The voice screen's route button lists the available devices and sets one for the call.
- At the end: `clearCommunicationDevice`, `MODE_NORMAL`.
- Speaker is the default because an assistant is talked to at arm's length; the earpiece with the
  proximity sensor's screen-off is Later.
- **Bluetooth headsets** use the hands-free profile: SCO takes time to set up, and its audio is
  narrowband (CVSD, or mSBC or LC3 on newer pairs), which may cost the ASR accuracy. WP0 (i)
  measures the setup time and runs a few turns through HFP. Bluetooth device names may need
  `BLUETOOTH_CONNECT`; WP3 adds it only if routing or names need it.

*Changed 2026-10-15 (WP3 part 1, as built):* part 1 sets `MODE_IN_COMMUNICATION` and the
communication device at a call's start (the first wired or USB headset, else `voice.route`) and clears
both at its end (`voice/CallAudio.kt`). Bluetooth (SCO, LE audio, hearing aids) and its setup time,
the device-change listener and the route button are part 2's, in the same class.

*Changed 2026-10-15 (WP3 part 2, as built):* `voice/CallAudio.kt` with the rules in
`voice/Routing.kt`:
- **The order:** Bluetooth headsets and hearing aids (SCO, LE audio, `TYPE_HEARING_AID`) first, then
  wired and USB headsets, then `voice.route`'s earpiece or speaker; within one rank Android's own
  order. Nothing offered: Android's choice stands, logged.
- **Changes:** the list follows `AudioDeviceCallback` (registered after the list is read, so the
  callback's first report of every device is no "connected"); a headset that connects mid-call
  takes over, the route button's choice included; a chosen device that leaves is no choice any more,
  and the rule picks again. Android's own switches follow `addOnCommunicationDeviceChangedListener`,
  and the button says "switching…" from the request until Android routes there (SCO's setup).
- **The route button** lists the communication devices (Bluetooth headsets by their own name) and
  sets one for the call; the choice is not stored. A device Android refuses (`setCommunicationDevice`
  false or throwing) is said under the button and logged, and Android's choice stands.
- **No `BLUETOOTH_CONNECT`.** `setCommunicationDevice` sets SCO and LE audio up itself, without a
  Bluetooth permission, and `AudioDeviceInfo.productName` carries a Bluetooth headset's name; a
  device without a name shows its kind ("Bluetooth headset"). WP0 (i) and item 12 say
  whether the names arrive; if they do not, the permission is added then, for the names alone.
- **The end:** the focus abandoned, the device cleared and `MODE_NORMAL`, once the ending earcon
  played on the call's route (§10.5's note).

*Changed 2026-10-15 (WP3 part 2's review):* the button's "switching…" ended only when Android
reached the requested device; when Android left it later, or never reached it, the button kept
naming it and said "switching…" for good. Now `RouteState`'s changes are pure (`voice/Routing.kt`,
JVM tests):
- **Android moves the call away** after it had switched to the requested device (SCO dropped, a
  multipoint headset busy with its other phone, the headset's own button ending HFP's virtual call):
  the request is cleared, the button names where Android put the call and says "Android moved the
  call to <X>" under it, and the rule picks again at the next device change (or the owner's choice).
  Kai does not ask again at once: that would fight Android, and a headset that dropped its link
  itself would be pulled back. A device Android reports first while SCO is still being set up is no
  move: the button keeps "switching…".
- **The requested device never arrives:** after `lmgw.timeout_s` from the request (the setting the
  phone uses for every wait on another side; Android gives no failure signal for
  `setCommunicationDevice`'s setup), or at once when the device leaves the list, the request is
  cleared and "Android did not switch the call to <X>; its own choice stands" replaces
  "switching…"; logged with the setting's name. A late arrival, the next device change or the
  owner's choice clears it.
- **A refused device** that leaves is said no more, and the rule picks again.

### 8.4 Headset buttons

- During a call Kai holds an active `MediaSession` with a playing `PlaybackState`, so the media
  buttons (`KEYCODE_HEADSETHOOK`, `KEYCODE_MEDIA_PLAY_PAUSE`) reach Kai instead of the last media
  app. Without it, play/pause starts that app's playback, whose focus request ends the call (§8.5).
- In automatic mode a press toggles mute; in push-to-talk mode a press starts a turn and the next
  one commits it (a toggle: a headset delivers no reliable hold); a long press, where the headset
  delivers one, ends the call.
- The session is released at the call's end. Which presses arrive, wired and Bluetooth, is WP0 (i).

*Changed 2026-10-15 (WP3 part 2, as built):* `voice/Headset.kt`; the core decides (`Voice::headset`):
- **Keys:** `KEYCODE_HEADSETHOOK`, `KEYCODE_MEDIA_PLAY_PAUSE`, `KEYCODE_MEDIA_PLAY` and
  `KEYCODE_MEDIA_PAUSE` (a Bluetooth headset may send play or pause by the session's state); a press
  counts at its release, a long press (a repeating or long-flagged key) once, and its release not.
- **In automatic mode a press is the ring's tap,** not only the mute: it stops Kai while a response is
  open, else toggles the mute (kai K6: a press that stopped Kai does not also mute). *Why:* the
  headset is the ring's stand-in with the phone in a pocket, and a press that muted while Kai went on
  speaking would leave no way to stop it there. Quiet, it mutes as §8.4 says.
- **The long press** ends the call where it arrives. Android's media key handling always keeps the
  long press of `KEYCODE_HEADSETHOOK` and `KEYCODE_MEDIA_PLAY_PAUSE` for voice input (*changed
  2026-10-15, WP3 part 2's review:* not "on many headsets": the session only ever gets their short
  press, delivered at the release): on One UI it opens an assistant, which takes the audio focus,
  so Kai pauses ("Paused: another app took the audio") or ends ("voice ended: another app took the
  audio") as §8.5 says. Only a headset sending `KEYCODE_MEDIA_PLAY` or `KEYCODE_MEDIA_PAUSE` long can
  end the call by its button. End stays on the screen and the notification; item 12 expects the
  assistant and records what arrives.
- *Changed 2026-10-15 (WP3 part 2's review):* a release Android flags as cancelled
  (`KeyEvent.isCanceled`) is no press.
- **The session** plays to the voice-communication stream (`setPlaybackToLocal` with the call's
  attributes), so the volume keys keep adjusting the call, not the music stream.

### 8.5 Audio focus

- `AudioFocusRequest(AUDIOFOCUS_GAIN_TRANSIENT)` with the voice-communication attributes at a call's
  start: music pauses or ducks as its app decides.
- A transient loss (a ringing phone call, another app's short focus): mute and stop Kai's speech
  (§7.6's stop); the ring says "paused: another app took the audio"; regaining focus leaves the
  mute on until the owner taps (no surprise listening).
- A permanent loss: the call ends (§7.7).
- Focus is abandoned at the call's end.

*Changed 2026-10-15 (WP3 part 2, as built):* the request and its changes are `voice/CallAudio.kt`'s, the
rules the core's (`Voice::focus` names the call, so an older call's change is dropped):
- **A loss that only asks to duck** (a navigation prompt, a notification's sound) is no loss for a
  call: the conversation goes on (Android does not duck a speech stream itself).
- **A transient loss** mutes first, which releases a held push-to-talk press (its turn committed, as
  a mute does), then stops Kai (§7.6), which also cancels the response that commit asked for: what
  the owner said is kept, and Kai does not answer into the ringing phone. The state line says
  "Paused: another app took the audio". The owner may unmute before the focus is back; that lifts
  the pause too.
- **A refused request** at the call's start (another app holds the audio, a phone call) ends the call
  as `Voice::failed` with "Another app holds the audio (a phone call?), so voice did not start. Try
  again once it is done." and Reconnect.

*Changed 2026-10-15 (WP3 part 2's review):*
- **A pause stops the idle clock.** While another app holds the audio for a while (from the
  transient loss until the focus is back) the call is not idle, so a phone call of any length does
  not end Kai's call by `voice.idle_end_s`; the clock starts again from the focus's return, also
  when the owner's unmute lifted the pause earlier, and a pending bind's clock stands still alike.
  Before, the pause counted as quiet and a phone call over 30 s ended the call by idle, with the
  ending earcon into the phone call. lmgw's word and a real failure end a paused call as before.
  Host tests in `voice/call/tests/device.rs`.
- **Nothing of Kai's sounds plays into another app's audio:** no cue, no ending earcon and no haptic
  while the focus is away, nor at an end as audio taken (`CallLife` in `voice/VoiceController.kt`).
  A rebind's next call during a pause starts paused too.
- **A refused request sets nothing up:** no microphone type, MediaSession, mode or route, no earcon
  and no haptic; the call only fails in the words above. Before, the session, the mode and the route
  were set up first and released at the end.
- **A loss for good** gives the request up at once (Android dropped it), so a later call asks again.

### 8.6 The microphone type on Kai's service, and the notification

*Changed after the owner's answers, 2026-10-14:* v3 had a microphone service of its own; the call
now runs in the service that holds the feed (M16, §12.1).
- `KaiService` declares `android:foregroundServiceType="specialUse|microphone"`, with
  `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_SPECIAL_USE`, `FOREGROUND_SERVICE_MICROPHONE` and
  `RECORD_AUDIO`.
- **At every call's start** Kotlin calls `startForeground` again with the types `SPECIAL_USE |
  MICROPHONE`: from the assistant session (the while-in-use exemption of the active
  VoiceInteractionService) or from a visible activity, never from the background. **At the call's
  end** it calls it with `SPECIAL_USE` alone. The service is the process's anchor while the screen
  is off or the overlay is gone, as it is between calls. If WP0 (b) finds that the type cannot be
  added this way from the assistant session, a separate `microphone` service carries the call, as
  in v3.
- *Changed 2026-10-15 (WP0 spike build):* `specialUse` (the type and its constant) exists from
  Android 14 (API 34), and minSdk is 31. On Android 12 and 13 the service starts with no type (below
  API 34 a foreground service needs none, and none has a timeout there), the manifest's `specialUse`
  flag is ignored, and only `MICROPHONE` is added and dropped. A LineageOS device's version (WP0 (o))
  says whether this case is real; the spike's service already runs this way.
- **The notification** (channel `status`, ongoing, low, silent) is the service's one notification.
  During a call: title "Kai", text the state and the thread ("listening · Kai: <title> · ends after
  30 s of quiet"), actions Mute/Unmute and End, tap opens the voice screen; it shows on the lock
  screen like a call's. Between calls it shows the link's state (§12.1). *Changed 2026-10-16
  (WP5's review):* "ends after 30 s of quiet" is left out while a call of the thread waits for the
  owner's verdict (`VoiceView::awaiting`), since the idle clock stands still then (§7.7's note).
- A running call continues when the screen locks: the owner started it, and the notification on the
  lock screen offers Mute and End. Starting one from the lock screen is Q7's (later).
- **No Wi-Fi lock.** The low-latency Wi-Fi mode applies only to the foreground app with the screen
  on, and a call with the screen off is the case that would need it. WP0 (b)'s screen-off runs show
  whether Wi-Fi power saving costs latency there.

*Changed 2026-10-15 (WP3 part 1, as built):* `KaiService` declares `specialUse|microphone`; a
call's start (from the visible voice screen) calls `startForeground` with `SPECIAL_USE | MICROPHONE`
(`MICROPHONE` alone below API 34), its end with `SPECIAL_USE` alone. The call's notification has Mute
or Unmute (Take over while a bind is pending) and End, category `call`; privacy by default on the lock
screen (§12.3, the owner's decision): while the phone is locked it is posted as "Kai · voice call"
with the same actions and no words or thread (§12.3's note of WP3 part 1's review), its public version
alike.
*Changed 2026-10-15 (WP3 part 1's review):* when Android refuses the microphone type (from Android 14
the while-in-use rule: the activity left the foreground before the service got there), the call ends
in "Android didn't let Kai keep the microphone in the background. Open Kai and try again." with
Reconnect (`Voice::failed`), and the service goes on with its link's notification; before, the call
ran on without the type, and a refused start of the service for the call stopped the service.
A call that ends while the app is not on screen raises its words on channel
`notices` (one at a time, tag `notice:voice`), with Take over where the end offers it, which opens the
voice screen to start the call (a push never starts voice). Replies do not notify while the phone's
own call runs (§12.3).

## 9. Assistant mode

### 9.1 What is declared

- `KaiInteractionService` (`VoiceInteractionService`), `android:permission=
  "android.permission.BIND_VOICE_INTERACTION"`, the action
  `android.service.voice.VoiceInteractionService`, meta-data `android.voice_interaction` →
  `res/xml/voice_interaction.xml`:
  ```xml
  <voice-interaction-service xmlns:android="http://schemas.android.com/apk/res/android"
      android:sessionService="dev.rootshell.kai.assist.KaiSessionService"
      android:recognitionService="dev.rootshell.kai.assist.KaiRecognitionService"
      android:settingsActivity="dev.rootshell.kai.MainActivity"
      android:supportsAssist="true"
      android:supportsLaunchVoiceAssistFromKeyguard="false"
      android:supportsLocalInteraction="false" />
  ```
- `KaiSessionService` (`VoiceInteractionSessionService`, same permission) creating `KaiSession`.
- `KaiRecognitionService` (`RecognitionService`, action `android.speech.RecognitionService`,
  meta-data `android.speech`): a stub that answers every request with an error at once.
  Consequence, said in setup: while Kai is the assistant, apps that use the system's default
  speech recogniser get no recognition from it (keyboards with their own recogniser are not
  affected). A real recogniser on lmgw's ASR is Later.
- `MainActivity` filters: `android.intent.action.ASSIST` (DEFAULT),
  `android.intent.action.VOICE_COMMAND` (unverified delivery, WP4), and the `lmgw-pair` scheme
  (§4.1).

*Changed 2026-10-15 (WP4, as built):* `assist/KaiInteractionService.kt` holds the three services
under the names above. The recogniser answers every request with `ERROR_CLIENT` and logs the
caller's uid; it is exported without a permission, as recognisers are (apps bind it through
`SpeechRecognizer`), and reads and records nothing. The filters' meaning is `app/Entry.kt`'s, pure:
`ASSIST` and `VOICE_COMMAND` open the voice screen with Talk ready and **start no call**,
`ASSIST` with `EXTRA_ASSIST_INPUT_HINT_KEYBOARD` opens the conversation, and an assistant intent
never pairs, whatever data it carries. *Why the filters start no call (decided conservatively,
review 2026-10-16):* `MainActivity` is exported, so a headset's voice button in a pocket on a locked
phone, or any foreground app's explicit `ASSIST`, would make Kai listen (the first version set the
call pending, and it started hours later at the unlock). No call starts without the owner's own tap
on Talk or the system's assistant gesture, which reaches the session (§9.2), not these filters. The
cost: a headset button opens the voice screen and the owner taps Talk. `MainActivity` also drops a
call still pending when it stops. The tile and the shortcut go through `TalkActivity`, which is not
exported and asks for the unlock at once (§9.4's note).
`setDisabledShowContext` is one constant, `KaiInteractionService.NO_SCREEN_CONTEXT` (**WP0 (a)'s
seam**); the session drops and logs any screen content or screenshot that comes anyway.

### 9.2 The session overlay

- **Process.** The core and its pairing come up at process start, and Kai's service keeps the
  process alive while paired (M16). `KaiInteractionService.onReady()` creates the WebRTC factory and
  device module (§7.2); it opens no network connection and no microphone. Whether the system also
  keeps the assistant's process alive and at what priority matters less with the service; WP4
  records it.
- **`onPrepareShow`** makes sure the factory exists (a process the system restarted since
  `onReady`) and calls `Voice::prepare()`, which warms a connection to lmgw (§7.2).
- **`onCreateContentView()`** returns a `ComposeView`. A ComposeView outside an activity needs its
  view-tree owners set by hand: `KaiSession` implements `LifecycleOwner` and
  `SavedStateRegistryOwner` and sets them on the view, or composition fails at attach.
- **`onShow(args, flags)`:** the overlay draws the ring in "connecting" and starts a call on the Kai
  thread at once (§7.3). `onShow` can repeat while a call runs: it shows the running call, starting
  nothing.
- **Layout:** a bottom sheet, the ring, the last two caption lines, and a row of Mute, Keyboard
  (opens the conversation in the app), Expand and End. "Voice is on <by>" with Take over, and the
  approval card (§11), appear above the row. On a foldable's cover screen the sheet takes the width;
  on the inner screen it keeps Material's maximum sheet width, centred, so the controls stay within
  reach of one thumb (M23, §10.6). Folding or unfolding while the overlay shows moves it to the
  other screen; the call goes on in the service.
- **Dismissal** (back, a swipe down, a tap outside): `hide()`. Before the first committed turn this
  also ends the call (§7.7), after it the call goes on in the background with its notification and
  ends by the usual rules. *Why:* an accidental long press must leave no open microphone, and a
  conversation the owner started must survive glancing at another app.
- **Expand** starts `MainActivity` at the voice screen (`startAssistantActivity` where it works,
  else `startActivity` with `FLAG_ACTIVITY_NEW_TASK`; WP0 (a)), then `hide()`. The call continues;
  it is the app's, not the overlay's.
- **No screen content is collected in v1:** `setDisabledShowContext(SHOW_WITH_ASSIST |
  SHOW_WITH_SCREENSHOT)` on the service (API 23; WP0 (a) confirms it takes effect), so the system
  neither reads the screen for Kai nor spends time on it. Q8 decides what comes later.

*Changed 2026-10-15 (WP4, as built):* `assist/KaiSession.kt` carries out `OverlayModel`'s decisions
(`assist/OverlayRules.kt`, pure, one instance in the application, so a session the system destroys
and makes again knows which call the overlay started); the sheet is `assist/OverlaySheet.kt`.
- **`onShow`** starts a call by the folder at once; a running call is shown, starting nothing; a
  start still on its way is not repeated; locked, nothing starts (§9.6's note); without
  `RECORD_AUDIO` the sheet says so with Open Kai, which opens the voice screen to ask and then start
  the call (the overlay has no activity to ask from).
- **Every hide that is no handover is a dismissal** (back, a swipe down, a tap outside, Home, the
  screen going off), and the core hears it for the overlay's own call (§7.7's note). A dismissal
  before the core answered the start reaches the call at the answer: `CallEngine.start` now answers
  as soon as the core took the call, and the PeerConnection, the offer and lmgw's answer follow on
  the voice thread, so the window is short.
- **Handovers** (Expand, Keyboard, Unlock, Open Kai) are no dismissal, start no call, and make the
  call the app's: a later show of the overlay does not end it. The overlay lets go of the call only
  **after** a path opened the app. Expand tries `EXPAND_PATHS` in order, the next when one throws or
  the app does not show (**WP0 (a)'s seam**): `startActivity` first (one Kai in Recents, item 23),
  `startAssistantActivity` the fallback; WP0 (a) may still reorder them. A path counts as opening
  the app when the start did not throw and the app became visible within `lmgw.timeout_s` (Android
  14 and later refuse a start from the background without throwing, so the app showing is the one
  signal; no constant of ours). If no path opened it, the sheet stays with an error line, the call
  stays the overlay's, and a later dismissal decides as usual. *Limit:* an activity that starts
  later than `lmgw.timeout_s` after the sheet gave up is not undone; the call is then the overlay's
  while the app shows it.
- **Screen off** hides the overlay (`hide()`), so it goes through `onHide` and the core's rule
  decides. **A fold or an unfold** while the sheet shows re-reads the lock state (`onConfigurationChanged`).
- **The sheet:** the ring (a tap or hold acts as on the voice screen), the state line and the call's
  facts, the last two caption lines, Mute or Unmute and End, then Keyboard and Expand as text buttons
  in a row of their own (four labelled buttons do not fit the cover screen at a larger font), a
  pending bind's Take over and End, and a line that says what closing the sheet does ("ends the
  call" before the first turn, "goes on in the background" after it and for the app's call). An end
  while the sheet shows is shown in it with Talk or what the end offers, and raises no notice.
- **The window:** a scrim over the screen, the sheet at the bottom, swiped down past 96 dp to
  dismiss; its own lifecycle, saved-state and view-model owners, as WP0's spike.
- **The first frame** after each invocation is logged (tag `kai`) from the window's frame metrics:
  its time after `onShow` and after `onPrepareShow`, after the press where the system's arguments
  carry `invocation_time_ms`, warm or cold (a process under 5 s old), the width class and the dp, the
  show flags and the process's importance (the process's priority is recorded, not relied on).
- **The process:** the overlay does not count as the app being visible for the hub (replies and
  approvals keep their notifications; the approval card in the overlay is WP5's).

*Changed 2026-10-16 (WP5, as built):* the sheet shows the first call that still asks as a card (its
place among them, its arguments scrolling inside it) in every unlocked face, above the controls.
While it shows unlocked it tells the core it sees the cards (`Core::set_overlay`), so no notification
asks for the same call; when it hides or locks, the calls still waiting are notified (§12.3's note).
Back goes through the session's own `OnBackPressedDispatcher`, wired to the window's
`OnBackInvokedDispatcher` (API 33+), so the sheet follows the back gesture (predictive back), letting
go brings it back and completing it dismisses it (§7.7's rule); the session's `onBackPressed` (the
key, Android before 13) asks the same dispatcher, whose fallback hides. The sheet scrolls when it is
taller than the window (a large font), and only then, so a swipe down still drags it. The process
note's "the approval card in the overlay is WP5's" is done.

### 9.3 The voice controller is the app's

The call, the service's microphone type, the MediaSession and the audio routing belong to one
app-level `VoiceController` (Kotlin, a singleton beside the core). The overlay, the voice screen,
the tile and the notification are views of it, so a call moves between them without a reconnect,
and an activity recreated by a fold or an unfold (M23) finds the call where it was.

*Changed 2026-10-15 (WP3 part 2, as built):* `voice/VoiceController.kt` is the controller for the
call's audio: the microphone type on Kai's service, the focus, the MediaSession, the route and the
earcons, around `CallEngine` (the PeerConnection) and the core's `Voice` (the rules); the
application holds both for the process's life. A rebind's next call keeps the audio as it is.

*Changed 2026-10-15 (WP3 part 2's review):* the controller's decisions are pure (`CallLife`: set up
once the focus is granted, kept for a rebind's call or one started while the last end's earcon
plays, released once the ending earcon played, each end's earcon numbered so an older one releases
nothing), and so is the order the engine reports a call in (`CallOrder` in `voice/CallEngine.kt`):
the start reaches the controller on the voice thread, in the order of the core's decisions, and an
end the core decided before the start was recorded waits and follows it. Before, the start went from
another thread after the call was made current, and such an end was dropped, which left the focus,
the mode, the microphone type and the MediaSession held. JVM tests for both.

*Changed 2026-10-15 (WP4, as built):* the overlay is one more view of the same call: it starts and
ends calls through `CallEngine` and the core's `Voice` as the voice screen does, and the controller
sets the call's audio up the same way (the microphone type from the assistant session is WP0 (b)'s;
a refusal ends the call in WP3's words, "Android didn't let Kai keep the microphone in the
background. Open Kai and try again.").

### 9.4 Invocation paths

| Path | Arrives as | Notes |
|---|---|---|
| side button press and hold (Samsung, One UI 8) | `onShow` | Settings → Advanced features → Side button → Press and hold → a digital assistant app instead of the power-off menu; whether One UI 8 lists Kai there is WP0 (a) |
| power button long press (LineageOS) | `onShow` | `config_longPressOnPowerBehavior` at 5 (the assistant); LineageOS's default is the global actions menu, and whether it offers a user setting is WP0 (a); `settings put global power_button_long_press 5` over adb overrides it |
| power button long press (Pixel) | `onShow` | the "digital assistant" setting |
| corner swipe, home pill long press | `onShow` with `SHOW_SOURCE_ASSIST_GESTURE` | with the role held |
| headset or Bluetooth button outside a call | `SHOW_SOURCE_PUSH_TO_TALK` or `ACTION_VOICE_COMMAND` | unverified, WP4 |
| Quick Settings tile "Talk to Kai" | `TileService.onClick` → `startActivityAndCollapse(PendingIntent)` to the voice screen, which starts a call | the `Intent` overload throws from targetSdk 34; works without the role, and where the OEM binds the power button to its own assistant |
| launcher shortcut "Talk" | a static shortcut to the voice screen with a start flag | |
| the app's ring | a tap | |

*Changed 2026-10-15 (WP4, as built):* the tile (`assist/TalkTileService.kt`) and the shortcut
(`res/xml/shortcuts.xml`, one per build type, since the debug build's application id ends in `.dev`)
open `TalkActivity`, which is not exported (the launcher starts a shortcut as Kai, and the tile's
`PendingIntent` is Kai's), so no other app can make Kai listen through them. It draws nothing,
shows over the keyguard only to ask for the unlock (`requestDismissKeyguard`; the tile asks through
`unlockAndRun` first), and then hands over to the voice screen, which asks for the microphone if
needed and starts the call (a running call is shown instead). Below API 34 the tile uses the
`Intent` overload. Every invocation path that reaches `onShow` is handled alike; the show flags are
logged with the first frame, so the acceptance checks say what a headset's button sends.

### 9.5 Setup

*Changed after the owner's answers, 2026-10-14:* the background card is new (rulings 6 and 7), and
the side button's steps are Samsung's (Q1).

Once the pairing is confirmed, setup shows two cards; each re-checks its steps on resume and is
done when they pass. The settings page shows both cards' state afterwards.

**"Kai in the background"** (M16), first:
1. `POST_NOTIFICATIONS`: without it the service still runs, but no notification shows, the
   approvals' and replies' included; the card says so.
2. **The battery-optimisation exemption:** `ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS` for Kai's
   package (the permission `REQUEST_IGNORE_BATTERY_OPTIMIZATIONS`), checked with
   `PowerManager.isIgnoringBatteryOptimizations`.
3. **On Samsung, "Never sleeping apps":** the steps in One UI 8's words (Settings → Battery →
   Background usage limits → Never sleeping apps → +, the path WP0 (m) confirms), with a button to
   Kai's app settings. One UI offers no intent to the list and no way to read it, so the owner ticks
   the step done.
4. When Android's local network protection is enforced (§5.2), its permission, for the LAN URL.

*Changed 2026-10-15 (WP2, as built):* the card is the settings page "Kai in the background", and the
main screen offers "Finish setup" once connected while a step is open. Notifications are checked with
`NotificationManager.areNotificationsEnabled()` (`POST_NOTIFICATIONS` exists from API 33; on 12 the
step opens the app's notification settings). Step 4 waits for WP0 (k)'s finding; nothing is declared
for it yet.

**"Kai as your digital assistant"**:
1. What changes: the side button or the gesture opens Kai's overlay; the system's voice input
   becomes Kai's stub (§9.1).
2. **Set Kai** opens `Settings.ACTION_VOICE_INPUT_SETTINGS`; on resume
   `RoleManager.isRoleHeld(ROLE_ASSISTANT)` decides whether the step is done. Whether
   `createRequestRoleIntent(ROLE_ASSISTANT)` shows a dialog on One UI 8 is WP0 (a)'s to find; if it
   does, it replaces the deep link.
3. **The side button's steps** for the phone's maker (§9.4): on Samsung, Settings → Advanced
   features → Side button → Press and hold → Kai; on LineageOS, the power button's setting where
   the build offers one; on a Pixel, the "digital assistant" setting. WP0 (a) confirms the first
   two.
4. `RECORD_AUDIO` is asked here, because the overlay is a poor place to ask (an invocation should
   listen, not show a permission dialog). Without the role, it is asked at the first call from the
   app.

*Changed 2026-10-15 (WP4, as built):* the card is a settings page, "Kai as your digital assistant"
(`ui/screens/AssistantPage.kt`, `assist/AssistantSetup.kt`), as WP2's background card is; the main
screen does not prompt for it (assistant mode is an option: the tile and the shortcut start a call
without the role). Set Kai opens `ACTION_VOICE_INPUT_SETTINGS` (`AssistantSetup.SET_KAI`, **WP0
(a)'s seam**: the role request where WP0 (a) finds that it shows a dialog), "Open default apps"
beside it. The button's step is told by the phone's maker (`Maker`: LineageOS by its product or
display name, then Samsung, a Pixel, others) and ticked by the owner; on LineageOS the page reads
`power_button_long_press` and the step is done at 5 (`AssistantSetup.POWER_ASSISTANT`). The
wording of each maker's steps is in the string resources `assistant_button_*` (WP0 (a) confirms One
UI 8's and LineageOS's). `RECORD_AUDIO` is asked on the page.

### 9.6 The lock screen

Not in the first usable version (M22). `supportsLaunchVoiceAssistFromKeyguard` stays false; an
invocation on the lock screen asks to unlock first. Q7's answer decides how it comes later: a call
started on the lock screen binds a temporary thread (a negative `chat_thread`, kept out of the
history and the feed) with the Kai folder's model, approvals and anything needing a decision
require an unlock, and the conversation and settings are not reachable from it. The Kai thread
itself stays out of reach while locked, so whoever holds the phone cannot ask about its history.

*Changed 2026-10-15 (WP4, as built; privacy by default, decided conservatively):* whether the
system shows the overlay over the keyguard is the system's; what Kai shows there is Kai's. While the
keyguard is locked or the screen is off (the notifications' rule, §12.3), a show starts nothing and
the sheet shows Kai's name and "Unlock to talk to Kai" with Unlock, nothing of the conversation; a
running call shows "Kai · voice call" with Mute or Unmute and End, as its lock-screen notification
does. Unlock opens `TalkActivity`, which asks for the unlock and then has the voice screen start the
call; an unlock while the sheet shows lets the invocation go on in the sheet. *Why:* the invocation
is the owner's wish to talk, but the phone may be in another's hands; the unlock is the proof that
lets the Kai thread be heard (Q7's temporary thread is Later).

*Changed 2026-10-16 (WP5):* while a call waits for a decision, the locked face adds "Kai needs your
approval", the notification's content-free words: no tool, no argument, and deciding waits for the
unlock, after which the card is in the sheet.

### 9.7 Screen context

Not in v1 (§9.2), and never automatic (Q8's answer). A later "Ask about this screen" chip in the
overlay sends a screenshot only when the owner taps it, once lmgw's realtime takes `input_image`
content (realtime §19).

## 10. UI and feel

### 10.1 Screens

- **Voice** (the start destination once paired): the ring in the upper half, captions under it,
  the state line, controls (Mute, automatic or push-to-talk, route, End). The Kai conversation is a
  sheet one swipe up. Opening the app does not start a call: a tap on the ring does. *Why:* opening
  an app is no request to listen, and the microphone indicator would light at every glance.
- **Kai conversation:** the messages (§6.2), streaming replies, the text field with IME insets,
  New conversation, and banners: the folder's steps, "Voice is on <by>", the hold.
- **Settings:** gateway (device name, the URLs and the one that last answered, lmgw's CA
  fingerprint shortened, the certificate's dates and notice, pair again, forget), connection
  (timeouts, the URL stagger, the backoffs, the feed's keep-alive), notifications (a link to each
  channel's system settings), voice (§3.4's voice keys), setup (§9.5's two cards), appearance,
  About (app version, core version, lmgw pin, lmgw's epoch).
- **Pairing and setup** (§4, §9.5).
- On a foldable's inner screen the voice and conversation screens sit side by side (§10.6).
- The threads browser and search are Later (M22): the first version is the Kai conversation.

### 10.2 Platform behaviour

- Navigation Compose, one activity, predictive back on (`android:enableOnBackInvokedCallback`),
  including the conversation sheet's back (`PredictiveBackHandler`).
- Edge to edge (`enableEdgeToEdge`), every screen padded by its insets; the text field follows the
  IME with `imePadding` and the IME's own animation.
- Material 3; the Expressive components where the Compose Material 3 release chosen at WP1 has them
  stable (no experimental opt-ins in release code). Colours per Q6's answer: Kai's graphite and blue
  by default, the system's dynamic colour as a switch (`appearance.colours`); the ring keeps Kai's
  state colours either way.
  *Changed 2026-10-14 (WP1 review):* `TopAppBar` and `ModalBottomSheet` are still `@ExperimentalMaterial3Api` in Material 3 1.4.0
  (Compose BOM 2026.09.00). WP1 keeps them with an `@OptIn` on the two composables that use them:
  the one exception to "no experimental opt-ins in release code", until a stable release of either.
- Typography and shapes are Material's defaults with the system font, so the app reads like the
  phone's other apps.

*Changed 2026-10-16 (WP5's audit, as built):*
- **Predictive back:** `enableOnBackInvokedCallback` is on; Navigation Compose 2.10 and
  `ModalBottomSheet` animate it (the conversation's sheet, settings, pairing, the scanner); settings'
  compact page keeps `BackHandler` (back returns to the list, without a preview), unchanged. Found:
  the overlay had none, back reaching it only through the session's `onBackPressed`; fixed as §9.2's
  note says.
- **Edge to edge:** found: the main screen and pairing added `imePadding` to a Scaffold padding that
  held the navigation bar, so a text field sat that much above the keyboard, and every screen padded
  by the system bars alone, so the cover screen's camera covered content in landscape. Fixed: every
  Scaffold pads by `safeDrawing` (`ui/Layout.kt`'s `ScreenInsets`: the bars, the cutout, the IME),
  its top bar by the top and the sides, and the main screen consumes it before the conversation's
  own IME padding; the overlay's sheet pads by `safeDrawing`'s sides and bottom.
- **TalkBack:** found and fixed: push-to-talk could only be held (now TalkBack's click starts the
  turn and the next one sends it, labelled so, and while muted it is refused with the reject
  haptic; *changed 2026-10-16, WP5's review:* a turn a click started is released, with the end
  haptic, when its control leaves or the call or its gesture changes, as the held press's `finally`
  releases one, so a fold or a mode switch never leaves a turn held open:
  `rememberPushToTalkClicks`' `DisposableEffect`); the push-to-talk switch and settings' switches,
  radio buttons and checkboxes were
  controls apart from their labels (now one item and one touch target, `ui/Controls.kt`); setup's
  steps read their ✓ and • aloud (now "done" or "to do"); a caption line was two items (now one); a
  message did not say who wrote it (now "You" or "Kai" first). The approval card's title is a
  heading; on the voice surfaces it is a polite live region unless the ring's live region already
  says it ("Kai waits for your decision on …" for the bound thread's calls), so TalkBack hears it
  once. The order is the layout's: the top bar, the ring, the words, the facts, the
  card, the controls.

### 10.3 The ring

A native Compose ring (a `Canvas` with `drawWithCache`), echoing Kai's ring and icon (kai K9, §9):
graphite at rest, amber while listening and interrupted, blue while thinking and speaking, grey and
struck through when muted or unreachable, an amber badge for the hold.
- **Levels:** the microphone's from the device module's samples (RMS per 10 ms, fed at 30 Hz as Kai
  feeds its ring); Kai's speech from the remote track, through the libwebrtc build's audio sink if it
  has one, else `getStats`' `audioLevel` at 30 Hz (WP3 picks).
- **Cost:** it animates only while a call is active (connecting, listening with sound, thinking,
  speaking); at rest and muted it draws one static frame, as Kai's standby draws none (kai K11).
- **Reduced motion:** with the animator duration scale at 0, the ring shows its state's colour
  without motion.
- **Accessibility:** the ring has a content description of the state ("Kai is listening") and
  announces changes as a polite live region; captions are text.

*Changed 2026-10-15 (WP3 part 2, as built):* `ui/voice/Ring.kt`:
- **Levels (WP3's pick):** both from the device module, Kai's through its playback callback (§8.1's
  note), not `getStats`' `audioLevel` at 30 Hz: it is what the speaker plays, so a stop's local mute
  reads as silence at once, and nothing is polled. Each 10 ms buffer's RMS on a dB scale from −60
  dBFS; the ring reads the latest at its frame rate in the draw phase (no recomposition per frame),
  up fast and down slowly.
- **Motion:** a halo following the microphone while listening and interrupted, Kai's level while it
  speaks, an arc running while it thinks, a breath while connecting; idle, muted, a pending prompt
  and rest draw one static frame. **Colours:** amber listening and interrupted, Kai's blue thinking
  and speaking (fixed, under the system's dynamic colours too), grey and struck through muted or
  unreachable, graphite otherwise (connecting, idle, pending, rest), the amber badge for the hold.
- **Accessibility:** the content description is the state and its words, a polite live region; the
  ring's tap is TalkBack's click (Talk, or stop and mute).

*Changed 2026-10-15 (WP3 part 2's review):*
- **Cost:** the ring redrew every frame while listening, silence included. Now the clock runs only
  for the thinking arc and the connecting breath, the halo's level is written only when it differs
  from its target by more than half a hundredth (under a pixel), and the draw phase reads only what
  its motion uses: a steady level draws nothing, so a variable-refresh display can drop in
  silence. The levels are still sampled at the frame rate (a volatile read, no draw).
- **Colours:** Kai's in every state, graphite and grey too, fixed under the system's dynamic colour
  (§10.2); in a dark set (Breeze's amber #F67400, Kai's blue #3DAEE9, as before) and a light one
  readable on #EFF0F1, where the dark set's amber (2.5:1) and blue (2.2:1) were too faint: amber
  #A84F00 (4.9:1, also for the amber notes, small text) and Kai's deep blue #1B6EA8 (4.8:1),
  graphite #4D545B, grey #7A8189 (3.5:1); white text on the amber hold button there.
- **Accessibility:** the ring's description is the screen's one polite live region (the state line
  was a second one, so TalkBack said every change twice); between calls it is the link's words, the
  click label saying it starts a call. Idle in automatic mode while another person's turn runs
  (K16) says so ("Kai is waiting while another person's turn runs"), not "hold to talk".
- **Reduced motion** is followed while the ring shows, so a change of the animator duration scale
  applies without reopening the screen.

### 10.4 Performance budget

Targets measured in the named WP; a miss is recorded with its numbers and decided on, not hidden:

| Measure | Target | Measured in |
|---|---|---|
| invocation → the overlay's first frame, warm process | < 300 ms | WP0 (a), WP4 |
| invocation → the overlay's first frame, cold process | recorded | WP0 (a) |
| invocation → listening (Wi-Fi, the LAN URL and the tunnel URL, warm, the primary test device) | go/no-go: median ≤ 1.0 s over 20 runs, none over 1.5 s (§7.2) | WP0 (c), WP3 |
| the same over mobile data through the tunnel, and cold | recorded | WP0 (c) |
| a new connection's TLS handshake with its Keystore signature | recorded; HTTP/2 on lmgw's device listener is weighed against it | WP0 (e) |
| process start → the Kai conversation re-read and drawn | recorded; the message cache (M10) is decided on it | WP2 |
| the held feed's battery cost per day, lmgw up and lmgw stopped for hours | recorded | WP0 (m), WP2 |
| a fold or unfold mid-call | no reconnect, nothing lost | WP0 (n), WP3 |
| scrolling the conversation | no frame over the display's budget in Perfetto, on both screens | WP5 |

A Baseline Profile module is Later (M22); R8 full mode is on from the start.

*Changed 2026-10-15 (WP4):* the overlay's first frame is logged at each invocation (§9.2's note), so
the first two rows are read from logcat on the phone; WP4's own row is item 19 of the acceptance checks.

*Changed 2026-10-16 (WP5):* the repository has no macrobenchmark module (the Baseline Profile
module is Later, M22), so the last row is read from the app itself: each scroll of the conversation
and of the captions logs its frames against `FrameMetrics.DEADLINE`, the display's interval at its
refresh rate for that frame ("conversation: scrolled N frames (compact 3xx dp), K over the display's
deadline (8.3 ms), the worst … ms", tag `kai`; `ui/FrameWatch.kt`), and a Perfetto trace gives the
detail (item 38). Found in the code: the conversation animated a scroll per streamed delta and pulled
the owner back to the end while reading; now it follows the end only while the owner is there, jumps
without an animation, and marks its items' content types. The captions the same. *Changed
2026-10-16 (WP5's review):* following starts again when another thread or call shows and at the
owner's own send (the captions: another call's lines), so a list scrolled up once does not stay
there for the next conversation.

### 10.5 Haptics and sound

A confirm haptic when listening starts, gesture-start and gesture-end haptics for push-to-talk, a
reject haptic on a refusal (`View.performHapticFeedback`, which honours the system's haptics
setting). An earcon when listening starts and when a call ends, played on the communication stream
by Kotlin, useful with the screen off; `voice.earcon` and `voice.haptics` switch them off.

*Changed 2026-10-15 (WP3 part 2, as built):* `voice/Earcons.kt`. The earcons are synthesised, two
80 ms sine notes with 12 ms fades and a 30 ms gap at a third of full scale: E5 to A5 rising when
listening starts, A5 to D5 falling at every end but a rebind's; on the communication stream, so they
go where the call's audio goes (a headset too) at the call's volume, and the route is kept until the
ending one played. Haptics: the tick at listening, gesture start and end for a push-to-talk press,
the reject haptic for a start refused at once or a call that could not start or go on.

*Changed 2026-10-15 (WP3 part 2's review):* the route was kept for the earcon's nominal 190 ms, which
a Bluetooth headset's SCO link plays later; now the call's audio goes once the track reports its
last frame played (its notification marker), bounded by its length and `lmgw.timeout_s` in case the
track never reports (logged), and a track whose write or play throws is released at once. No cue,
ending earcon or haptic plays while another app holds the audio or at an end as audio taken, nor for
a call whose focus was refused (§8.5's note).

*Changed 2026-10-16 (WP5, as built; decided here):* a third earcon, the ask (B5 twice, `Earcon.ASK`),
with a heavy-click haptic, when a call waits for the owner's verdict in a call (`EventSink::asked`,
once per call id, by the cue's rules: never while another app holds the audio), so an owner listening
with the screen off knows to look; the approval's notification keeps its channel's alert. The confirm
haptic when the core took a verdict given on a card; the reject haptic when it refused it, and again
when lmgw's refusal of a verdict given there reaches its card; and the reject haptic instead of the
gesture's start for a push-to-talk press while muted, which the core ignores.

### 10.6 Foldable screens

*Added after the owner's answers, 2026-10-14* (Q1, M23). The primary test device is a foldable:
a cover screen of about 904×2316 px, narrower than 600 dp, and an inner screen of 1812×2176 px, 600
dp or wider. WP0 (n) reads the real dp of both from `WindowMetrics` and records them.
- **Size classes, not devices.** Every screen reads `currentWindowAdaptiveInfo()` and lays out by
  its width class; nothing checks for a foldable. A Pixel, a LineageOS device, split screen and a
  tablet get the same rules.
- **Compact width (the cover screen):** as §10.1 describes, the voice screen with the conversation
  as a sheet one swipe up.
- **Medium and expanded width (the inner screen):** voice and conversation side by side, the ring,
  captions and controls in one pane and the conversation in the other, so a call and its text are
  both in view; the conversation's column keeps a readable measure instead of running the full
  width. Settings and setup become list and detail.
- **Fully resizable.** No `screenOrientation`, no `resizableActivity="false"`, no aspect-ratio
  limits: from targetSdk 36 Android ignores them on screens 600 dp and wider anyway, and the
  temporary opt-out ends at 37. Landscape on the cover screen works too.
- **Folding and unfolding** change the window's size, and the activity is recreated (the default;
  no `configChanges` override). That is safe because nothing lives in the activity: the call is in
  the service and the `VoiceController` (§9.3), and every screen draws from the core's state, so
  the recreated screen shows the same call, captions and conversation, and nothing reconnects.
  Whether One UI keeps the app open when the phone folds is its "Continue apps on cover screen"
  setting; the call does not depend on it. WP0 (n) and WP3 test a fold and an unfold mid-call,
  mid-sentence, with the overlay and with the app.
- **The assistant overlay** works on both screens (§9.2): full width on the cover screen, centred at
  Material's maximum sheet width on the inner one.
- **The tabletop posture** (half-folded, Jetpack WindowManager's `FoldingFeature`) is Later: the
  ring above the hinge and the controls below would suit a call on a table, and nothing in the
  layouts above forecloses it.

*Changed 2026-10-15 (WP3 part 2, as built):* `ui/voice/VoicePane.kt` is the voice screen on every
width: the ring in the upper part (60 % of the width, at most 30 % of the height, 112 to 240 dp), the
state line and the call's facts under it, the captions filling the middle, the controls at the
bottom; a window under 440 dp high puts the ring beside the rest when it is wider than tall (the
cover screen in landscape) and scrolls as one column otherwise (a split screen), so the controls
stay reachable. On the inner screen it is the left pane beside the
conversation, as before. A fold or an unfold recreates only the drawing: the call, its captions and
its route live in the application, a call waiting for the microphone's permission survives the
recreation, and the screen's warm-up (`prepare`) does not run while a call is up, so nothing
reconnects.

*Changed 2026-10-16 (WP5's size-class and font-scale audit, as built):* the voice pane's arrangement
follows the font scale (Android's up to 200 % grows the text, not the ring) and an approval card: tall
while the window holds 440 dp, plus 220 dp with a card, times the font scale (at least 1); else the
ring beside the rest when wider than tall; else one scrolling column (`voiceArrangement`). Rows of
buttons wrap (`ButtonRow`, a FlowRow) instead of running off the cover screen, and Hold to talk grows
with its text. Pairing keeps a readable measure (640 dp) on a wide window. Kept: voice and
conversation side by side on a wide window with the conversation at 640 dp, the overlay at Material's
sheet width, settings as list and detail. No screen checks for a foldable.

## 11. Approvals

- **Where they come from:** in a call, `mcp_approval_request` items (api §6.4); otherwise the feed's
  open set for threads the phone may see: `hello.open_approvals` and the set on every `state`,
  then `approval.requested` and `approval.decided` after `caught_up` (§5.4; api §6.5).
- **The card:** the server label and tool, the arguments in compact lines (kai K22's form), Approve
  and Decline. It shows in the overlay, on the voice screen and in the conversation at the call's
  message.
- **Deciding:** in a call, `conversation.item.create` of `mcp_approval_response` then
  `response.create` with an `event_id`, whose echo in an `error` means the verdict was not taken
  (kai §11.1's review note); otherwise `POST /chat/api/threads/{id}/approvals`. A 409
  `approval_decided` closes the card with "decided on <by>"; `approval.decided` and
  `lmgw.approval.decided` close it too.
- **Talking moves on:** speaking or typing while a card is open is a new user message, which lmgw
  answers by declining every pending call (api §6.3). The card says so ("say something else to
  decline"); the phone does not hold the microphone back for it.
- **Not visible, call or no call** (ruling 6): a heads-up notification (channel `approvals`) per
  open approval, built from the event alone ("Kai wants to run desktop__open_file (path: …) in
  '<thread_title>'"), with Approve and Decline actions; Approve carries
  `setAuthenticationRequired(true)` (API 31; unverified on One UI 8), so it needs an unlocked
  phone, Decline does not. An action goes to a receiver that calls `Core::decide`, which POSTs from
  the service's process; the tap itself opens the conversation at the approval's message.
  `approval.decided` withdraws it (§12.3). *Changed after the owner's answers, 2026-10-14:* v3
  notified only during a call. *Changed 2026-10-15:* a dismissed approval stays dismissed across
  reconnects (§12.3).
  *(changed 2026-10-15: WP2's security review: a decision from a notification may start the
  process before the open set is known. The core holds it until the first `caught_up` since start,
  at most `lmgw.timeout_s`. After that it says "not connected to lmgw, the decision was not sent"
  and leaves the call open; it never guesses "decided elsewhere". The action's receiver starts
  Kai's service first, which a notification's action may do from the background, so the process
  lives that long. A verdict held for a reply's other calls is never asked again, neither when the
  app leaves the screen nor at a reconnect's `caught_up`. While the app is not visible, the
  reply's calls that still wait are raised again, dismissed or not, saying "your verdict on X
  waits for this one".)*
- **By voice:** Later, with Kai's phrase matcher moved into a shared crate.

*Changed 2026-10-16 (WP5, as built):*
- **One card per call** (`approvals.rs`, `approvals/call.rs`): the feed's open set and the running
  call's items are merged by id, the feed's facts winning. Each id is asked once, whichever source
  brought it: by a card while one is seen (the app visible, or the overlay showing unlocked), else by
  one notification, which a decision anywhere withdraws. A call only the session showed goes with the
  call's end (the feed brings it back while it waits). `lmgw.approval.decided`, or the route's 409
  `approval_decided`, keeps the call out of a later feed set until the feed withdraws it or the next
  `caught_up`. *Changed 2026-10-16 (WP5's review):* the session's items name no message, so within a
  thread a call the session showed is of one reply with the other calls the same session showed and
  with the feed's calls of their message (lmgw keeps at most one reply waiting per thread); before,
  a reply the feed had named in part was split in two, and its verdicts went as two halves.
- **Where it shows** (`ui/approvals/`): in the conversation at its message (a call whose message the
  list lacks, at the end); the compact voice screen and the overlay show the first that still asks,
  with its place ("Decision 2 of 3"); a wide voice pane shows none, the conversation beside it does;
  never on the lock screen (§9.6's note).
- **The card:** the tool, the arguments in compact lines (on the voice surfaces scrolling inside a
  bounded box, never cut), the thread, "In this call: Kai speaks the answer once you decide" for a
  call whose whole reply the running call showed (*changed 2026-10-16, WP5's review:* the reply's,
  not the call's alone, since only then does the verdict go through the call), Approve and Decline,
  or the verdict held (it goes to lmgw with the reply's other verdicts) or sent, lmgw's words when it
  did not take the last one, and that saying or typing something else declines it. *Changed
  2026-10-16 (WP5's review):* an Approve tapped while the phone is locked is refused on the card
  ("Unlock the phone to approve. Declining works while locked.", the reject haptic): the locked
  faces show no card, so this is defence in depth, as the notification's Approve needs the unlock.
- **Deciding:** through the call when its session showed every call of the reply (§7.6's note), else
  by the route. lmgw's refusals put its words on the card and ask again; `approval_out_of_scope`
  says "lmgw does not let this phone approve it (…). Decline it here, or approve it on lmgw's
  dashboard."; a notice says them only while no card is seen. A 409 `approval_decided` closes the card
  with "Already decided: <lmgw's words>" (nothing for verdicts the route sent again after the call,
  which lmgw had most likely taken there). A verdict on its way is not given a second time.
  *Changed 2026-10-16 (WP5's review):* only lmgw's refusals before deciding (an `approval_*` code but
  `approval_decided`, or `key_rate`) say "lmgw did not take the verdict"; any other code says "The
  decision may not have reached lmgw: …", since lmgw may have written it before failing, and asks
  again (the feed closes the card if it had).
- **What lmgw decided** (*added 2026-10-16, WP5's review*): a verdict sent stays "sent" until the
  feed's `approval.decided` (or its withdrawal) closes the card; the phone never closes it on its own
  guess (§7.6's note). The feed's event carries `approve`, `by` and `not_run`, and a decline because
  a new message came reads as `approve: false` by the message's writer (lmgw's `decline_waiting`,
  `MOVED_ON`; no separate flag reaches the feed). When it differs from the verdict the phone sent,
  the owner is told once in a notice (the snackbar in the app, else a notice notification, content
  free while locked): the phone approved and its own new message declined it ("desktop__open_file
  did not run: what you said or typed reached lmgw before your approval, and a new message declines
  what waits. Ask Kai again if you still want it."), another client declined it first ("… did not
  run: the dashboard declined it before your approval reached lmgw."), another approved it after a
  decline here, or it was approved and its turn could not start ("… was approved, but it never ran:
  lmgw could not start the turn that was to run it.", lmgw's second `approval.decided` with
  `not_run`). The verdicts sent are kept past their cards for this until the next `caught_up` finds
  the call no longer waiting; a 409 `approval_decided` that said "Already decided", or a refusal
  before deciding, ends them.
- *Decided (WP5):* the card's state is the core's alone, so the overlay, the voice screen, the
  conversation and the notification never disagree; a notification raised while away stays in the
  shade when the app shows (no second alert; a decision either way withdraws it).

## 12. Background and notifications

*Changed after the owner's answers, 2026-10-14:* v3 had no background work and no push; rulings 6
and 7 put the held feed into the first usable version (device link §6.4 items 1 to 3).

### 12.1 Kai's service

- **`KaiService`** (§8.6 for its types and permissions) carries `PROPERTY_SPECIAL_USE_FGS_SUBTYPE`:
  "Holds the connection to the owner's own lmgw gateway, which delivers its notifications (replies,
  approvals)".
- **Started** when a pairing is confirmed (from the visible app), whenever the app becomes visible
  with a confirmed pairing and the service is not running, from a `BOOT_COMPLETED` receiver (which
  Android 15 allows for `specialUse`), and from `MY_PACKAGE_REPLACED` after an update.
  `START_STICKY`, so the system restarts it after the process dies, and the core resumes the feed
  from its cursor (§5.4). Forget gateway stops it. Whether One UI 8 restarts it reliably after a
  kill, and how the battery exemption and the never-sleeping list change that, is WP0 (m).
- **Its notification** (channel `status`, low, silent, ongoing) shows "Kai · connected to lmgw" or
  §5.3's words for the state ("lmgw is not running · next try 07:42"), updated on a change of state
  only; during a call it shows the call (§8.6). Its tap opens the app. The owner may silence the
  channel in Android's settings; the service runs on.
- *Changed 2026-10-15 (WP2, as built):* in WP2 the service declares `specialUse` alone (the
  microphone type joins with WP3's calls); started once enrolled from the visible app (§4.2's note),
  whenever the app becomes visible with a pairing and the service not running, at boot and after an
  update; stopped by Forget gateway. A refused start (Android's background rule) is logged.
- **What it costs:** one held TLS connection, lmgw's keep-alive comment every `feed.keepalive_s`
  (each one wakes the radio), the reconnects. No wake lock is held: an incoming packet wakes the
  CPU for its handling, and WP0 (m) checks that a notification follows an event within seconds
  after an hour with the screen off. The prepared WebRTC factory is memory, not activity.

### 12.2 Wake-ups, the dead link and retries

- **The core's deadlines count sleep.** The dead-link verdict and the next retry are kept on
  elapsed real time (`CLOCK_BOOTTIME`, Android's `elapsedRealtime`), not on tokio's monotonic clock,
  which stops while the phone sleeps. At every wake (an incoming packet, an alarm, a network
  callback, the app becoming visible) the core compares them with that clock and acts on what is
  due.
- **One alarm.** The core publishes `Update::WakeAt(t)`, the earliest deadline it needs the CPU
  for, and Kotlin keeps one inexact `setAndAllowWhileIdle` alarm on `ELAPSED_REALTIME_WAKEUP` for
  it (no exact-alarm permission), whose receiver calls `Core::wake()`. Doze may defer it to a
  maintenance window; the settings text says so.
- **Connected:** the deadline is the dead-link verdict, the last byte plus two keep-alives. lmgw's
  comment arrives every `feed.keepalive_s` and moves it, so the alarm fires only for a link that
  went silent. lmgw notices a vanished phone by `TCP_USER_TIMEOUT` (device link D16).
- **Not connected:** the next retry, 1 s doubling to `lmgw.retry_max_s` (30) while the app is
  visible and to `lmgw.retry_max_background_s` (300) while it is not. A new default network and
  the app becoming visible retry at once. With lmgw stopped for hours, the phone therefore wakes at
  most every five minutes, or at Doze's spacing, for one connect that fails.
- **The app becoming visible** asks `current` (§6.1), which doubles as a probe of the path: if it
  fails, the core reopens the feed at once instead of waiting for the verdict.
- *Rejected:* a short keep-alive while the app is visible (a reopen, with its TLS handshake and
  Keystore signature, at every change of visibility, where the probe above costs one request on the
  pool); exact alarms (a permission, and wake-ups the platform would otherwise batch); WorkManager
  (minutes of delay, and no held connection).

### 12.3 Notifications

- **Built from events alone** (device link D18): `approval.requested` carries `thread_title`,
  `server_label`, `name` and `arguments`; `message.added` and `message.updated` carry the messages
  whole and `thread_title`. No notification waits for a fetch.
- **Channels:** `status` (low, silent; the service's notification, §12.1), `approvals` (high;
  §11), `messages` (default; Kai's replies), `notices` (default: voice moved or stayed elsewhere,
  connection lost, voice ended by focus, the certificate's coming lapse).
- **Approvals** come from the open set (§5.4): one notification per `approval_request_id`, raised
  at `caught_up` for the calls the core does not know (`Added`, or `Open` filtered the same way) and
  for each addition after it, withdrawn on `approval.decided`, a 409 `approval_decided`, or a
  `Withdrawn` (a decision made while the link was down, a reply or thread gone, a reach lost, found
  in the set at `caught_up` or in a later `state`). The core keeps the ids it notified for and the
  ids the user dismissed across reconnects and gives them to each connection's reader
  (`FeedReader::knowing`), so a dismissed approval is not raised again and a reconnect leaves no
  stale notification; every `Added` and `Open` is filtered by the dismissed ids. Their actions are
  §11's; the tap opens the conversation at the approval's message. *Changed 2026-10-15:* the section
  raised the open set at every `caught_up` and withdrew on `approval.decided` alone.
- **Replies:** an assistant message added in the Kai folder's threads while the app is not visible
  and no call of the phone's runs (the owner hears that one) is a notification per thread: title
  `thread_title`, text the reply's start, `message.updated` replacing the text while it shows,
  `message.deleted` of that message withdrawing it. During a catch-up, replies are collected, and at
  `caught_up` each thread gets one notification with its latest reply and the count. The tap opens
  the conversation; a **Talk** action opens the voice screen. Opening that thread withdraws it.
- **No double alerts:** ids are stable (the approval's id, the thread's id) and every notification
  sets `setOnlyAlertOnce`, so a resumed feed or a restarted process never alerts twice for one
  thing.
- **A push never starts voice** (M16). Every voice action in a notification (Talk, Take over) is an
  activity `PendingIntent` to the voice screen, which starts the call as a visible activity may;
  the microphone type is while-in-use, and no notification action starts a call from the
  background.
- **Not notified:** others' turns (`turn.*` carries no text; the message follows), `voice.*`,
  `folder.*`, `thread.*`, and anything while the app is visible, where the screen shows it.
- *Changed 2026-10-15 (WP2, as built):* "the Kai folder's threads" are known from one
  `GET /chat/api/threads` in each connection's folder lookup, then kept by `thread.*` and
  `folder.current`; the replies of a catch-up wait for that list before `caught_up` collapses them.
  Notification tags carry the stable ids (`approval:<id>`, `reply:<thread>`), so the shell reads the
  active ones back at start. A swiped approval reaches `dismiss_approval`; a tapped or swiped reply
  reaches `notification_gone`, so a later `message.updated` does not bring it back. An approval
  approved or declined from its notification is withdrawn at once, and asked again (a notice says
  why) when lmgw does not take the verdicts.
- *(changed 2026-10-15: WP2's security review, the owner's decision: privacy by default on the
  lock screen. Every notification is `VISIBILITY_PRIVATE` with a public version that holds no
  content: "Kai needs your approval", "New reply from Kai", a notice's "A notice from Kai", the
  status's "Kai is running". The content shows only once the phone is unlocked. Also from the
  review: taps and Talk go through a trampoline activity that is not exported, and Talk withdraws
  its reply. Every PendingIntent has an identity of its own (`Intent.setIdentifier`: the tag, with
  the action), so no collision can rewrite another approval's extras. A swiped approval's receiver
  finishes once `approvals.json` holds the dismissal, within 5 s, and logs when it did not. Each
  notice has a tag of its own (its text's SHA-256), so a second notice no longer replaces the
  first silently. A folder lookup that fails, or whose thread list does not read, leaves the Kai
  folder's threads unknown. The replies keep collecting and the lookup is asked again on its own
  backoff, 1 s doubling to the ceiling, until the list reads.)*
- *(changed 2026-10-15: WP3 part 1's review. The public version alone did not hold the owner's
  decision: under Android's default lock-screen setting, "Show all notification content", the lock
  screen shows a private notification's content. Now, while the keyguard is locked or the screen is
  off (`KeyguardManager.isKeyguardLocked`, `PowerManager.isInteractive`; the keyguard may lock after
  Android's lock delay while the screen is off, and no broadcast an app may receive says when), every
  notification Kai posts has the content-free text of its kind as its main content: "Kai needs your
  approval", "New reply from Kai", "A notice from Kai", "Kai is running", and the call's "Kai · voice
  call" with Mute (or Unmute, Take over) and End. No other action shows there: an approval's Approve
  needs the call it decides, Talk and Take over open the app. The notifier keeps in memory how each
  notification is built and posts it again with the other face at `ACTION_SCREEN_OFF`,
  `ACTION_SCREEN_ON` and `ACTION_USER_PRESENT` (registered at run time; Kai's service keeps the
  process). A notification that was withdrawn, cleared or tapped away is never posted again. The public
  version stays as the second line. A notification posted while locked by a process that died since
  keeps its content-free text after unlocking until it changes, since its content is never put into
  the notification; its tap opens the app, which shows it.)*
- *Changed 2026-10-16 (WP5):* for approvals, "while the app is visible" is "while a card is seen":
  the app visible, or the assistant overlay showing unlocked (`Core::set_overlay`); when the overlay
  hides or locks, the calls still waiting are notified. An approval the running call showed is
  notified once like any other, and the call's ask plays too (§10.5's note).

## 13. Phone tools later: what v1 must not foreclose

- **Their own design** decides the tools (alarms and timers, calendar, notifications, location,
  intents, …), their permissions, and what runs while locked.
- **The host link lives in the core,** a task like Kai's `src/host/` (kai PD2): `GET /mcp/host`
  over tokio-tungstenite, authenticated by the device's certificate through the one constructor
  (M2), rmcp's server role at lmgw's rmcp version, and
  `lmgw-client`'s `mcp_host` module for refusals, closes and `_meta`. The tools are implemented in
  Kotlin behind a callback interface (`ToolHost.call(name, arguments_json) -> result_json`),
  declared in Rust under kai K22's rules and checked by a test as kai PD6 does; whether they share
  `kai-desktop`'s contract crate is that design's call. This answers api §5.1's "an Android SDK will
  need a custom transport": no Android MCP SDK and no Android transport are needed.
- **v1 keeps the seams:** the core's link has room for a second long-lived task beside the feed, the
  settings a place for a hosting label, and nothing in the UniFFI surface assumes the core only
  consumes.
- **Offline is normal:** lmgw skips an offline device's label rather than refusing a turn (api
  §5.3). With the held feed the process now lives while the phone is paired (M16), so the host
  link can be held by the same service, and the phone's tools are reachable whenever its feed is;
  whether they should be, locked or not, is that design's question.

## 14. iOS later: what Android must not foreclose

- The core has no Android types: Android-only facts (the default network, the Keystore) enter
  through `PlatformFacts`, `NetworkFacts` and the `Keystore` callback, which iOS fills differently
  (`NWPathMonitor`, a Secure Enclave P-256 key signing through the same callback; device link
  Later).
- UniFFI's Swift bindings come from the same library; nothing in the core assumes JNA.
- WebRTC on iOS: the same libwebrtc family (LiveKit's iOS build), audio through `AVAudioSession`
  in `.voiceChat` mode; the same signalling through the core.
- No assistant role on iOS: App Intents, Shortcuts and the Action button stand in; that is the iOS
  design's.
- The device key is the Secure Enclave's; `link.json` and the settings are the core's on both.
- **The held feed has no iOS counterpart:** iOS does not keep an app's socket open in the
  background, so ruling 7's push channel is Android's. How an iOS client learns of a reply while
  closed is the iOS design's question.

## 15. Build, test, install

### 15.1 CI

- The CI configuration, with every job tagged for the build runner.
- `ci/Containerfile`: a Fedora image with JDK 21, the Android command-line tools, the platform and
  build tools of the chosen API level, the NDK (r28+), rustup with both Android targets, cargo-ndk
  and just; versions pinned in the file. Built once and kept by tag; rebuilt only
  when the file changes.
- Jobs: **check** (the core's `cargo fmt --check`, `cargo clippy`, `cargo nextest run`,
  `cargo test --doc` on the host; Gradle `lint` and `testDebugUnitTest`) and **build** (the release
  APK, `arm64-v8a`, signed, kept as the pipeline's artifact). That artifact is installable as it
  is, and it is how a tester without a build setup gets the app until Obtainium follows the project's releases
  (Q1, Q10).
- lmgw is cloned at `LMGW_PIN` by the CI job.
- **Signing:** (WP1: the variables are `KAI_KEYSTORE_FILE` (type File), `KAI_KEYSTORE_PASSWORD`,
  `KAI_KEY_ALIAS`, `KAI_KEY_PASSWORD`; `KAI_REQUIRE_SIGNING=1` in the build job makes a missing keystore
  fail the build; locally the release APK is unsigned.) The release keystore is a protected CI file variable with a masked password, never in
  the repository. Losing it means a reinstall and a new pairing on every phone, so the owner keeps an
  offline backup. Debug builds use the SDK's debug key.
  *Changed 2026-10-14:* the keystore travels base64-encoded in `KAI_KEYSTORE_B64`, because CI variables hold text.

### 15.2 Local

`just` is the entry point: `just core-test` (nextest and doc tests), `just core-android`,
`just apk` (a debug APK, in the CI image under Podman with the repository mounted `:Z`, or with an
installed SDK when one is found), `just install` (adb over wireless debugging; the first usable
version's way onto the phone), `just check` (what CI's check runs, plus the pin warning).
*Changed 2026-10-14 (WP1 review):* `just apk` builds in the image only; the installed-SDK path is not built. `just install` takes
`apk=<path>` (default: the debug APK) so that a CI-signed release can go onto the phone, installs for the
current user only (`--user current`: never into a work profile) and picks one adb serial
(`scripts/adb-serial.sh`).

### 15.3 Tests

- **The core** on the host, with a loopback fake of lmgw's device listener whose certificates rcgen
  makes in the test (as lmgw's own tests do) and a software P-256 key behind a test `Keystore`:
  - **the device link:** enrolment's answer and each of its refusals; a `ca_certificate` that does
    not hash to `fp` refused; `EnrolVerifier` against a server that holds no certificate hashing to
    `fp`; the steady-state verifier against a server under another CA, an IP-address host, a host
    the leaf does not name, a lapsed leaf; the client certificate presented through the `Keystore`
    callback; the one-constructor test (every lmgw-facing client refuses a server under another
    CA); `http` URLs dropped from a refreshed list; the confirmation's wait (403
    `device_unconfirmed`, then 200); the certificate's states from `notice_after` and `not_after`;
  - **the link:** the URL race (the last good URL first, the stagger, the first verified
    handshake wins, the whole race bounded), the reconnect on a new default network, the feed's
    waits, the refusals and revocation kinds (`certificate_expired` among them), the backoff's two
    ceilings, the deadlines on a clock that jumps across a sleep, `WakeAt`;
  - **the held feed:** the cursor written after each stored event and resumed with
    `Last-Event-ID`; `caught_up` after a paged catch-up and right after `hello`;
    `hello.open_approvals` with a catch-up that holds a request and its decision; a reconnect after
    a decision made while the link was down (`Withdrawn`), after a dismissal (no second alert) and
    after a call that left the reach and came back (`Added`, filtered); a `state`'s set before and
    after `caught_up`, and one that is `null`; which events
    notify, collapsed per thread at `caught_up`, with stable ids, and none while visible or during
    the phone's own call;
  - the folder rule;
  - **the call's signalling:** `chat_folder` with and without `new`, `takeover=ask`, the `session`
    part with and without an override, the folder's refusals (404 `chat_folder_not_found` runs
    the lookup again), a refusal at the open, Reconnect's hangup of a stale id (a 404 counts as
    done);
  - **the events channel's rules:** the bind's outcome (`session.created` then `session.updated`
    with `resolved.chat_thread` and `turn_mode`; `bind_pending` with its prompt, Take over, the
    holder ending first, a new holder, a decline, nothing but `take_over` sent while pending, the
    microphone `Off` and back with the delayed cue, a `bind_pending` after the cue); push-to-talk
    with its tail; mute; K16; the stop with and without a playout time, the local mute and its lift
    at `output_audio_buffer.started`; the mute on `output_audio_buffer.cleared` after
    `turn_detected` and after `client_cancelled`, and no change on `speech_started`; playout
    reports at the setting's interval, with a missing stat; part reassembly and splitting, and a
    part sequence that cannot be completed; End as the stop and then the channel's close; every
    `lmgw.session.closed` kind (`thread_bound` after an unanswered prompt, `revoked` with
    `certificate_expired`), an unknown kind, and a call that dies without the event.

  Live checks of the HTTP side run against a dev lmgw started by lmgw's dev-instance script, never
  the owner's running gateway, paired like any device (device link §13.2's recipe shows the steps).
- **Kotlin** JVM unit tests: view models against a fake of the core (the generated bindings sit
  behind a Kotlin interface so they can be faked).
- *Changed 2026-10-15 (WP2, as built):* the core's host tests run against a scripted lmgw
  (`tests/it/mock.rs`) behind a mutual-TLS device listener whose certificates rcgen makes
  (`tests/it/pki.rs`, as Kai desktop's `test_pki`), with a squatter answering ClientHellos with alerts,
  a software P-256 key behind the `Keystore` callback, and a clock whose boot time jumps as across a
  sleep (`clock::ManualClock`): the device link (enrolment and each refusal, the pinned verifiers,
  one key per token, `Unproven`, the confirmation's poll on the kept-alive connection, the dates), the
  link (the race and the last good URL, a new network, the dead-link verdict after a sleep with
  `WakeAt`, the backoff's two ceilings, refusals and revocation kinds, alerts, a key gone from the
  Keystore), the held feed (§15.3's list) and the conversation (the folder rule, `current`,
  `message.*`, a send streaming in, New conversation). The Kotlin tests cover the hub, the message
  merge, the notification tags, the pairing links and the background card.
  *(changed 2026-10-15: WP2's security review added host tests for: a 401 of an unknown code, both
  shapes of a failed folder lookup, a decision held until `caught_up` and its bound, a held verdict
  across backgrounding and a reconnect, the refused link schemes, a 5xx/429/codeless 4xx keeping
  the enrolment's key, a re-pair whose `link.json` cannot be written, the orphan key sweep, a
  Keystore failure short of a lost key, the acknowledged dismissal, and the source test against a
  default reqwest client. The Kotlin tests add the notices' tags.)*
- *Changed 2026-10-16 (WP5):* host tests for the book's merge of the call's and the feed's calls,
  the routing of a reply's verdicts, lmgw's word on the call, the call's end, a refused verdict's
  note and what waits (`approvals/tests.rs`); for the call's items, the waiting words and the idle
  clock, the tagged verdicts and each outcome, an end with verdicts on their way, and the
  continuation's timing (`voice/call/tests/approvals.rs`); through the integration harness, an
  approval decided in a call and then the continuation, a verdict after the call's end by the route,
  and the overlay's cards counting as seen (`tests/it/voice.rs`, `tests/it/held.rs`). JVM tests for
  the card's rules, the voice arrangement by font scale and the frame tally. *Changed 2026-10-16
  (WP5's review):* host tests for a reply the feed and the session both show, lmgw's decision other
  than the verdict sent told once, and the codes that mean refused before deciding
  (`approvals/tests.rs`); for approving while the owner talks, a cancelled or silent resume, each
  `error` code's outcome and a failed resume, and a task that ends while Kai speaks
  (`voice/call/tests/approvals.rs`); through the integration harness, the card staying sent until
  the feed's `approval.decided`, approving while still talking (the feed's decline by the phone and
  the notice), and Approve then End at once (one route request, its 409 closing the card quietly).
- **On the phone** (wireless adb): instrumented UI tests for pairing by paste with the
  confirmation, the states of §5.3 and the conversation against the dev lmgw's device listener,
  through its LAN URL and its tunnel URL, on both screens of a foldable.
- **Audio and WebRTC** only on real hardware: TTS clips played from a speaker beside the phone, or
  the owner's voice; recordings are never committed. The emulator proves nothing about audio.
- **Acceptance checks** for what only a person at the phone can do: invocation paths on the foldable
  test device, a headset, folding mid-call, a day with the held feed, real conversations.
- **A LineageOS device's checks:** the phone without Google's services, the power button's
  assistant, and, once P12's release exists, a day against a separate lmgw.

### 15.4 Installing and updating

The first usable version goes onto the test phone with `just install`; a tester without a build
setup installs the signed APK from CI's build job (Q1). Once the app has proven usable, CI publishes
signed APKs to the project's GitHub releases and Obtainium follows them (Q10); an in-app updater on
lmgw's `lmgw-update` crate comes only if Obtainium proves awkward.

## 16. lmgw-side prerequisites

Built in lmgw's repository under lmgw's own designs and reviews; kai-mobile bumps `LMGW_PIN` after
each lands. Kai's desktop moves with P4, P5 and P9 at its next pin bump. *Changed after the owner's
answers, 2026-10-14:* P1 to P4 and P8 follow the calls design and the device link; P9 to P12 are
new (device link §6.4 item 13).

**P1. The calls route as designed** (calls WP1 to WP5 and calls WP7, the device socket group, which lands with or right after device link WP2; lmgw 2328902a, with the device link's changes to it,
device link §8): everything under "lmgw, being designed: calls" above. The multipart `session` part
applied as the first update right after `session.created`; `Location` and the slot-free hangup;
one candidate, the local address the POST arrived on, which on the device listener is the LAN or
tunnel address the phone used; the device socket group (calls C4, §3.1, WP7): one socket per device-listener address, following it live; the SDP inside the device link's mutual TLS; `takeover=ask` (calls only) with
`lmgw.session.bind_pending {held_by}`, inbound audio discarded and every other client event
answered with an `error` of code `bind_pending` while pending, `lmgw.session.take_over`,
`session.created` by itself when the holder ends, and `thread_bound` after one
`realtime.ping_interval_s`; RTP kept from the first frame on the session's timeline when the bind
succeeds directly; `output_audio_buffer.*`, with `cleared` as the barge-in verdict;
`lmgw.response.stop {playout_ntp_ms}`, idempotent and never answered with
`response_cancel_not_active`; `lmgw.output_audio.playout {playout_ntp_ms, jitter_buffer_ms}` used
for drained and the barge-in window and considered stopped after one `realtime.ping_interval_s`
without a report; `webrtc_commit_window_ms`; `lmgw.event.part` in both directions;
`lmgw.output_audio.starved`; `lmgw.session.closed {kind, by, message}` as the last event before
lmgw ends a bound call; and End by the client's close of `oai-events`, after lmgw has applied
every event received before it. The bind by folder is P8. The phone relies on none of the feed's
events for a call's end (§7.8). Needed by WP3, and by WP0 (c), (h), (j), (l) unless the stand-in
serves.

**P2. `lmgw-client` for calls** (calls §8.3, calls WP5):
- readers for every `lmgw.*` event a Kai call receives: `lmgw.session.bind_pending`,
  `lmgw.session.closed` with `CloseKind` from its `kind` (`ThreadBound` new; `closed` and a kind
  the build does not know as `Other`, whose code becomes optional; `revoked`'s `RevokeKind` from
  the message's leading token, `certificate_expired` among them), `lmgw.output_audio.starved`, and
  `session.lmgw.resolved.chat_thread` and `session.lmgw.turn_mode` in `session.created` and
  `session.updated`;
- `lmgw.event.part`: reassembly before parsing, and splitting of what the client sends to the
  peer's `max-message-size`;
- builders for `lmgw.response.stop`, `lmgw.session.take_over`, `lmgw.output_audio.playout` (both fields optional, as in the calls design; §7.6), and
  `session.update` with `session.lmgw.turn_mode`;
- the `output_audio_buffer.*` events typed; `voice::Machine` taking its player facts from
  `started`, `stopped` and `cleared`, `starved` from `lmgw.output_audio.starved {response_id,
  starved}`, interrupted from a `cleared` after a `turn_detected` cancel, and the remote track's
  mute from a stop's tap or a `cleared` until the next `started`;
- the call's request builder (multipart with the `session` part; `chat_folder`, `new`,
  `takeover=ask`) and reader (the id, the answer, refusals through `read_refusal`, which reads
  `error.held_by`), and the hangup's;
- `revoked`'s `kind` typed in the feed's items (§5.5).
Needed by WP3.

**P3. api WP10:** `message.*` records in every message write and their catch-up, carrying the
messages whole and `thread_title` (device link D18); the typed `Message`, `Attachment`,
`SendRequest` and send-stream frames with their DocRoutes; `GET /chat/api/threads/{id}`
documented. Needed by WP2, for the conversation and the reply notifications. The pairing link's QR
code has moved to the device link (P10). The phone no longer needs `voice_resolved`: lmgw reports
the call's mode as `session.lmgw.turn_mode` and applies the detection itself (§7.4).

**P4. The pairing link v2, enrolment and the pinned client in `lmgw-client`** (device link WP6,
§6.3):
- `lmgw_client::pairing`: `PairingLink::parse` for v=2 (strict, with the v1 link's message),
  `Fingerprint`, `PairingToken` (redacted `Debug`, no `Display`), `LinkError`, `normalize_base_url`
  (`https` only), `PairFacts` and `display_name`, moved from `kai/src/pairing.rs` with its tests;
  `url` 2 joins the crate's dependencies;
- the enrolment builder and readers: `requests::enrol`, `read_enrolment` (the `fp` check on
  `ca_certificate`, `https`-only URLs), `Credentials` with `state(now_ms)` (valid, notice, lapsed),
  `refresh_urls`, `Urls::order`; `key_refused` gaining `CertificateRequired`,
  `CredentialOnDeviceListener`, `Unconfirmed {code}`, `CertificateExpired` and
  `TokenUsedElsewhere`, and the enrolment's refusal codes through `read_refusal`;
  *Changed 2026-10-14 (device link WP6):* the refusal is a plain `Unconfirmed`, with no code: the app shows the confirm code it computed
  itself (`Credentials.confirm_code`, derived from its own key) and never parses lmgw's 403 message;
- the non-default `tls` feature: `EnrolVerifier { fp }` and `client_config(provider, credentials,
  signing_key)`, the pinned configuration that the client's one constructor takes, with the
  provider left to the app (ring here);
- the feed's `keepalive_s`, `CaughtUp`, and `Hello`'s `urls`, `cert` and `open_approvals`, with
  `FeedReader` keeping the open set.
Kai switches to it. Needed by WP2.

**P5. The ongoing-folder rule in `lmgw-client`:** kai §4's five-step lookup and K2-22's convergence
as a pure function over the folder list, the remembered id and the name, returning the choice and
the namesakes to log. Kai switches to it. Needed by WP2. *Changed 2026-10-14 (consistency pass):* no owning WP exists in the lmgw designs; this is an lmgw-client change done alongside device link WP6 (lmgw-client's additions).

**P6. Push-to-talk's commit under WebRTC.** *Answered by the calls design:* a commit waits
`webrtc_commit_window_ms` (default 120, measured in lmgw's WP6) and takes the frames stamped up to
the commit plus the window. The phone's part is the release tail, which must fit inside that window
(§7.5). After WP0 and lmgw's interop rig, the cut on the phone's clock
(`lmgw.input_audio_buffer.commit {captured_until_ntp_ms}`, `.clear {captured_from_ntp_ms}`) may
replace it, making the tail a phone-only concern and removing the fit rule (§7.5, Later).

**P7. The data channel's message size.** *Answered by the owner in the calls design:* on bound
calls an event larger than the peer's `max-message-size` travels as
`lmgw.event.part {id, index, count, data}` messages, in both directions, and `lmgw-client`
reassembles them before parsing (P2, §7.10). `event_too_large` with emptied fields remains only for
unbound calls, which Kai never makes.

**P8. `chat_folder=` on the bind** (calls WP2). *Answered by the calls design (C23)*, and a firm prerequisite of
WP3. `?chat_folder=<id>` (with `&new=true` for New conversation): lmgw binds the folder's current
thread in the same step, rolling it over as `current` does, reports it in
`session.lmgw.resolved.chat_thread`, and reports `session.lmgw.turn_mode`, applying for
`automatic` the thread's turn detection, or `semantic_vad`. Its refusals are 404
`chat_folder_not_found`, 409 `not_ongoing` and 409 `folder_no_model`. It takes the `current`
round trip, the retry after a thread that went stale, and the race with a rollover off a call's
critical path (§7.3). It is in the shared `RealtimeQuery`, so the WebSocket takes it too.

**P9. The device link's trust, listener and pairing** (device link WP1 to WP3): lmgw's device CA
and server certificates that name every URL host; the device listener (TLS 1.3 with client
certificates, its own port and stop, loopback until the first remote device); the principal read
from the certificate per request, with `client_certificate_required`,
`credential_on_device_listener`, `device_certificate_expired`, `device_unconfirmed` and the RevokeKind `certificate_expired` (carried under kind `revoked`); pairing tokens, `POST /devices/enrol` and the owner's Confirm; the
pairing form's URL list; the end of bearer device keys. Needed by WP2, and by WP3 (calls reach the
device listener, device link WP2's `Conn`).

**P10. The Devices card for the device link** (device link WP4): the QR card (the QR code, the
link, the token's expiry, the URLs, `fp`), the confirmation with the code and Confirm, the
certificate's dates and notice, the contested notice. Needed by WP2: confirming needs the card's
Confirm, and scanning needs the QR code; pasting works without it.

**P11. The feed as push channel** (device link WP5): `?keepalive_s=` with its maximum and
`TCP_USER_TIMEOUT`; `hello.open_approvals`, `state.open_approvals` and `caught_up`; `hello.urls` and
`hello.cert`; `thread_title` on `approval.requested`; the RevokeKind `certificate_expired` (carried under kind `revoked`). Needed by WP2's
held feed; its `message.*` side is P3. *Changed 2026-10-15:* built in lmgw are `?keepalive_s=` with
its maximum, the open set (`hello`, `state`, `caught_up`) with its reader in `lmgw-client`
(`ApprovalsChange`, `FeedReader::knowing`) and `thread_title`; **not built yet** (device link WP2
and WP3): `TCP_USER_TIMEOUT`, `hello.urls`, `hello.cert`, `certificate_expired` and the per-link
details. Kai mobile's code that needs them waits for them.

**P12. A published lmgw release that carries P1 to P11** (the calls and the device link). Tests
against a separate lmgw beyond WP0's LineageOS points wait for
it, together with an installable APK (§15.4).

Later on lmgw's side, not needed for the first version: silent certificate renewal over mutual TLS
(device link WP8); HTTP/2 on the device listener, so a call's POST could ride the feed's connection,
weighed against WP0 (e)'s handshake cost; Android key attestation at enrolment; `input_image`
content in realtime items (Q8); `lmgw.input_audio.preroll`, if Q5's measurement says so after WP0;
the cut on the phone's clock (P6), after WP0 and lmgw's interop rig; the in-place rebind of a
running call (Later), after WP3. An Android MCP transport is not needed (§13).

## 17. Work packages

| WP | Scope | Done when | Needs |
|---|---|---|---|
| 0 | **Device spike on the foldable test device** (One UI 8, Android 16), with (a) and (o) also on a LineageOS device; one verdict per point in `spikes/wp0/RESULTS.md`, only what could overturn the architecture: (a) **invocation:** whether One UI 8 lists Kai under Side button → Press and hold and the press reaches `onShow`; whether `createRequestRoleIntent(ROLE_ASSISTANT)` shows a dialog; on LineageOS the power button's long press reaching Kai (`config_longPressOnPowerBehavior`, whether LineageOS offers the choice as a user setting, else `power_button_long_press` over adb); the overlay's first frame after invocation, warm and cold process, on the cover and the inner screen, with the factory prepared in `onReady`; Expand's way into the app; `setDisabledShowContext`; (b) **the microphone type** added to Kai's `specialUse` service from the assistant session and from a visible activity, and dropped at the end; the call surviving dismissal and 10 minutes of screen-off; the indicator, the notification; Wi-Fi latency with the screen off; (c) **the go/no-go of §7.2**: libwebrtc (the LiveKit build, standalone) against lmgw's calls route over the device link, or a stand-in (a str0m ICE-lite peer in `spikes/wp0/` behind TLS that answers the POST and echoes audio and channel); no-gathering offer, one candidate, time to listening (the cue at DTLS-connected) through the LAN URL and the tunnel URL on Wi-Fi and through the tunnel on mobile data, warm and cold, with and without a warm connection; speech from the cue on reaching the turn; the press-to-DTLS gap split into its steps, the TLS handshake and its signature among them; the gap when an `AudioRecord` capture hands the microphone over to libwebrtc (both for Q5); (d) hardware against software echo cancelling on the speaker: false barge-ins at three volumes, folded and unfolded; (e) **UniFFI, JNA and TLS:** APK size, first-call time, an `EventSink` call from a tokio thread through JNA with its thread attach and detach cost, 16 KB alignment of every `.so`; rustls with ring built by cargo-ndk; a Keystore `SHA256withECDSA` signature through the `Keystore` callback, cold and warm, and a full TLS 1.3 handshake with client authentication; (f) **networks:** libwebrtc gathering on every network against lmgw's one candidate: which local candidate wins on a LAN-URL call and on a tunnel-URL call; a Wi-Fi to mobile-data handover mid-call over the tunnel; leaving the Wi-Fi mid-call on a LAN-URL call, with and without a tunnel that routes lmgw's LAN address; the core's URL race and its reconnect on a change of the default network; (g) push-to-talk's tail: clipped endings against the release tail, which sets `voice.ptt_tail_ms`, and whether that tail fits inside lmgw's `webrtc_commit_window_ms` (else the window is raised before WP3), with the press and release times kept for P6's alternative; (h) `starved` from `lmgw.output_audio.starved` in a tool turn with a spoken preamble; (i) headset buttons through the MediaSession, wired and Bluetooth; SCO setup time; a few turns over HFP's narrowband audio; (j) the takeover against a listening desktop Kai: `bind_pending` with `held_by`, Take over by `lmgw.session.take_over`, the desktop's 4000, the desktop ending while the prompt shows, a decline, the wait running out (`thread_bound`), End, and the desktop's state after it; the microphone `Off` while pending (the indicator out, the call alive without RTP until lmgw's bound) and the restart from `session.created` to the first captured samples; (k) **on the LAN:** with a WireGuard profile up on the home network, which URL answers first and whether a call connects through it; a full tunnel's "Exclude private IPs"; whether One UI 8 enforces Android 16's local network protection on the LAN URL; (l) playout facts on the LiveKit build: `estimatedPlayoutTimestamp`, `jitterBufferDelay` and `jitterBufferEmittedCount` in the remote track's `inbound-rtp` stats, how often they update, whether the playout time lands on lmgw's sent timeline, how long a `getStats` read takes after the mute (the error it adds to heard), and that disabling the remote track silences it at once; (m) **the held feed:** the `specialUse` service holding a feed over mutual TLS at `keepalive_s=120` for a day: the battery drain per day on the test device, with lmgw reachable for part of the day and stopped for several hours (the background backoff), from Android's battery usage and `dumpsys batterystats`; whether an event raises its notification within seconds after an hour of screen-off and Doze; the battery exemption and Samsung's "Never sleeping apps" (its path on One UI 8) and what each changes; the restart after a process kill (`START_STICKY`) and after a reboot (`BOOT_COMPLETED`), resumed by the cursor with `caught_up` and no double alert; how soon a silent drop (lmgw's machine suspended) is noticed with the screen off; the inexact alarm's spacing under Doze; (n) **the two screens:** the real dp and window size classes of the cover and the inner screen from `WindowMetrics`; folding and unfolding mid-call and mid-sentence (the activity recreated, the call alive in the service, nothing reconnecting, nothing lost); the overlay on both screens; One UI's "Continue apps on cover screen" for Kai; (o) **a LineageOS device without Google services:** the APK installs and starts, the core loads, libwebrtc initialises, the Keystore key is generated and signs, the overlay and notifications work; its Android version recorded for M15 | every point has a verdict; (c) is go, or its miss is decided on; Q5 and (m)'s drain are decided on their numbers; a failed point changes this document first | WP1's core build for (e); P1 for (c), (h), (j), (l), and P9 and P11 for (m), against lmgw, or the stand-in where it can serve |
| 1 | **Scaffold:** the repository of M1, Gradle with the catalog, the core with a first `Core` (version, pin, settings), rustls with ring in the build and no aws-lc-sys in `cargo tree`, bindings by `just core-android` from `preBuild`, the app shell (theme, navigation, the size-class skeleton of §10.6, empty screens), the pin check, the CI image with check and build jobs, signing | CI builds a signed APK that `just install` puts on the foldable test device and that installs on a LineageOS device, showing the core's version and the lmgw pin on both screens; host tests green; the targetSdk recorded | — |
| 2 | **Pairing over the device link, the held feed, the Kai conversation by text:** scan, tap and paste of the link v2 with the confirmation; the Keystore key, enrolment, the confirmation's wait and the pinned client from one constructor (§4); the URL race and the reconnects (§5.2); Kai's service holding the feed with its cursor, `caught_up`, the open approvals, wake-ups and backoffs (§12); notifications for replies and approvals with their actions; setup's background card; every state of §5.3 and the kinds of §5.5; the folder and `current`; messages with Markdown, text sends, settings; the layouts on both screens | paired by QR from a dev lmgw's card and confirmed there; each state shown on cue (lmgw stopped, the computer unreachable, a used and an expired pairing link, a lapsed certificate, the hold, disabled, expired, deleted); a text turn streams; a dashboard turn appears through `message.*`, and with the app closed as a notification within seconds; an approval raised on the desktop shows as a notification with the app closed and is decided from it; a killed process resumes without a double alert; the start time recorded for M10 | P3, P4, P5, P9, P10, P11 |
| 3 | **Voice over WebRTC:** the call (§7.3) bound by folder with the `session` part, the warm connection, the cue at DTLS-connected, the pending bind and Take over (`bind_pending`, `take_over`, the microphone off while pending, nothing else sent), the events channel through the core with parts, states, ring, captions, mute, push-to-talk with its tail, the stop (the local mute and `lmgw.response.stop` with the playout time), the mute on `output_audio_buffer.cleared`, playout reports, `starved`, End by the stop and the channel's close, the end reasons from `lmgw.session.closed`, Reconnect, the microphone type on Kai's service and its notification, routing, the MediaSession, focus, the echo setting, the voice screen on both screens | round trips on the phone through the LAN URL and the tunnel URL in both modes; words spoken right after the cue reach the turn; a stop by speech and by tap leaves the heard part in the thread; a held thread prompts on a connected call, Take over moves the voice without a second setup, and the desktop taking it back shows its name; End leaves no open response; 10 minutes with the screen off; a fold and an unfold mid-call keep the call; a Bluetooth headset routes and its button mutes; a dev hold shows the badge and the fallback's name; a dev lmgw stopped mid-call says so | P1, P2, P8, P9 |
| 4 | **Assistant mode:** the interaction service, the session overlay with the prepared factory on both screens, the stub recogniser, the filters, the tile and shortcut, setup's assistant card; the role on One UI 8 and LineageOS, their invocation paths | the side button opens the overlay within the budget on both screens and listens; the LineageOS power button does the same; dismissal before a turn closes the call, after a turn the call goes on; Expand moves the call into the app without a reconnect | WP3 |
| 5 | **Approvals in a call and polish:** the card in the overlay, on the voice screen and in the conversation; haptics and earcons; the predictive-back and insets audit; accessibility (TalkBack, font scale); the size-class audit on both screens; scrolling measured | an approval from the desktop's tools decided on the phone in a call (the card) and outside one (WP2's notification); acceptance on the foldable test device: the app has proven usable; a second device's against a separate lmgw once P12 exists | lmgw's approvals (built) |

*Changed 2026-10-15 (WP2):* built against lmgw f6917516: pairing by tap, paste and scan, the
Keystore key and enrolment, the pinned client from one constructor, the URL race and the reconnects,
Kai's service holding the feed with its cursor, `caught_up`, the open approvals and their
notifications (raised, withdrawn, dismissed, decided from the notification), reply notifications, the
alarm and the backoffs, §5.3's states and §5.5's kinds, the Kai conversation by text with Markdown,
the settings pages and the background card, on both screens' layouts; host tests against a scripted
lmgw. Not built: images in messages (§6.2's note) and the local network permission (§9.5's step 4).
Its "done when" is the acceptance: on the foldable test device against a dev lmgw (pairing by paste or tap until
lmgw's QR card exists, the Confirm, each state on cue, a dashboard turn and an approval with the app
closed and the screen off, a killed process resuming without a double alert, the start time for
M10).

*Changed 2026-10-15 (WP3):* built in two parts, against lmgw 34d25cba: part 1 the call (§7) and its
notification, part 2 the call's audio (§8.2 to §8.5), the ring, the earcons and the voice screen on
both screens (§10.3, §10.5, §10.6). Its "done when" is the acceptance, on the foldable test device: the checklists at
the top of this document (items 1 to 11 for part 1, 12 to 18 for part 2).

*Changed 2026-10-15 (WP4):* built against lmgw 34d25cba: the core's dismissal rule, the interaction
service, the session overlay on both screens, the stub recogniser, the filters, the tile and the
shortcut, and setup's assistant card (§9's notes). Its "done when" is the acceptance: items 19 to 26 at the top of this document, after WP0 (a). The points WP0 (a)'s verdict can
change are `EXPAND_PATHS` (Expand's way into the app; `startActivity` is first since 2026-10-16), `AssistantSetup.SET_KAI` (the role's
dialog), `KaiInteractionService.NO_SCREEN_CONTEXT` (whether the flag takes effect) and the
`assistant_button_*` strings with `Maker` (where One UI 8 and LineageOS put the button's setting).

*Changed 2026-10-16 (WP5):* built against the same pin: approvals in a call and outside one through
one card per call, the ask's earcon and the haptics, the audits' fixes, and scrolling measured by the
app's own log (§7.6's, §9.2's, §10.2's to §10.6's and §11's notes). Its "done when" is the acceptance:
items 27 to 40 at the top of this document. Not built: images in messages
(§6.2's note; with the attachments, Later) and a verdict by voice (Later, as §11 says).

The first implementation step is WP0 on the foldable test device, with WP1's scaffold beside it (WP0 (e) uses
WP1's core build). The critical path is P1 (with calls WP7), P8 and P9 → WP3 → WP4. WP2 runs while lmgw builds the
device link and the calls route; WP0's (c) and (m) can start on stand-ins.

## 18. Questions, in priority order (all answered)

The owner answered every question on 2026-10-14; each keeps its v3 text, with the answer after it.
Q2 and Q11 are restated, because the device link and ruling 7 removed what their v3 text assumed.

1. **Target phone: model, OEM, Android version** (and a second Android phone, if any). They
   decide the invocation paths (WP4), minSdk's margin and the spike's target. *Recommendation:* name
   the primary phone; WP0 runs on it, and minSdk 31 stands unless it is older.
   *Answered 2026-10-14:* a Samsung foldable on Android 16 / One UI 8.0 is the primary target,
   and a LineageOS device (against a separate lmgw) checks the build without Google services, so
   there are no shared-lmgw or folder-scoped keys. WP0 runs on the foldable; the LineageOS device (a second test device) is the check without Google's
   services and of AOSP's power-button assistant (WP0 (a), (o)). On Samsung the side button's Press
   and hold → Digital assistant is the path, and Kai goes on "Never sleeping apps" during setup
   (§9.4, §9.5). Further testing needs a published lmgw release with the calls and the
   device link (P12) and installable APKs (§15.4). The two screens are M23 and §10.6.
2. **The tunnel at home.** v3 made the pairing URL lmgw's tunnel address, so the phone used the
   tunnel at home too, which needed NAT hairpinning or a LAN-endpoint profile, and release builds
   had no switch that sent the key outside the tunnel. *Recommendation (v3):* always-on WireGuard
   everywhere, home included, and no exception in Kai.
   *Answered 2026-10-14:* approved as recommended, and superseded by the device link (ruling 8).
   WireGuard is routing only; the link carries several `https` URLs (LAN, tunnel) and the phone
   uses whichever answers; no hairpinning is required, and there is no tunnel gate (M5, §5.1,
   §5.2). The phone does not restrict libwebrtc to the VPN network: lmgw's candidate is the address
   the POST arrived on, so libwebrtc gathers on every network (M5).
3. **Should desktop Kai take its voice back when the phone's call ends?** Today a takeover leaves it
   idle until its talk key (kai §5.6), so after a phone call the desktop's all-day listening stays
   off. *Recommendation:* yes, when the phone's call ends with `voice.ended {reason: closed}` (End,
   the idle end, or the phone gone) and listening was on before the takeover: desktop Kai rebinds
   with `takeover=never`; not after another takeover or a revocation. A Kai desktop change to K5,
   none in lmgw. Related: while the desktop is listening, every phone start asks Take over once;
   a phone setting that takes over without asking would remove that, and is not in this design.
   *Note: the phone's microphone while the prompt shows* (§7.9): the phone turns its
   microphone off while a bind is pending and on again at `session.created`, so the indicator never
   lights while lmgw discards what it hears, and a declined prompt never had the microphone open.
   The price is the capture's restart, added to every start that meets the prompt (WP0 (j) measures
   it). Today that is the first phone start after the desktop listened; with this question's yes it
   is every phone start, because the desktop then always holds the thread when the phone begins.
   Keeping capture on during the prompt removes the restart, at the cost of a lit indicator while
   nothing is heard; a phone setting that takes over without asking removes the prompt and the
   restart together.
   *Answered 2026-10-14:* approved as recommended (§7.9). WP0 (j)'s restart time is decided on the number
   if it is long enough to be felt.
4. **What the phone's key may reach** (set on the Devices card at pairing). *Recommendation:* the
   aliases the Kai folder uses; a budget when any of them has a cloud fallback (kai §16 Q2's rule); a
   tool scope naming Kai's label (`kai__*`), so the Kai conversation behaves the same from the phone,
   with the folder's approvals deciding what needs a yes; `self_admin` off; and a
   `concurrency_limit` of at least 2, since a call holds one slot for its life and a text send's
   turn needs another (the feed holds none).
   *Answered 2026-10-14:* approved as recommended, with `concurrency_limit` at 2 or more (§4.1,
   §6.3).
   *(changed 2026-10-15: "a tool scope naming Kai's label (`kai__*`)" is now the device-tools grant `kai`, with the tool scope `all`. lmgw's reach rule changed (client-apps design L16): a scope pattern alone reaches no device's tools; §4.1.)*
5. **A pre-roll: `lmgw.input_audio.preroll`, decided after WP0** (§7.2). lmgw keeps the call's RTP
   from the first frame, so speech from DTLS-connected on is heard; words spoken between the press
   and DTLS-connected are lost. The phone could capture PCM with a plain `AudioRecord` from the
   invocation (the side button's press), before the PeerConnection exists, send it as
   `lmgw.input_audio.preroll` once `oai-events` opens (in parts when large), placed ahead of the
   first RTP frame on the session's timeline, and hand the microphone to libwebrtc. Under ruling 5
   the lmgw addition is no cost in itself; the question is what it buys. WP0 (c) measures the
   press-to-DTLS gap and the handover's gap (the audio neither capture records while the
   microphone changes hands). *Recommendation:* decide with those numbers: build it if the
   press-to-DTLS gap is regularly long enough to swallow the start of what the owner says, and the
   handover's gap is short enough not to clip the words that follow; otherwise not built, whatever
   the go/no-go's verdict.
   *Answered 2026-10-14:* approved as recommended: decided after WP0 on (c)'s numbers.
6. **Colours.** *Recommendation:* Kai's graphite and blue by default, with the system's dynamic
   colour as a switch. Pastel looks are not wanted, and Material's dynamic colour is a tonal
   palette drawn from the wallpaper, which mostly comes out pastel. The ring keeps Kai's state
   colours (amber listening, blue speaking) either way, so the states read as on the desktop.
   *Answered 2026-10-14:* approved as recommended (§10.2).
7. **The lock screen** (after the first usable version). Voice from the lock screen speaks for the
   Kai thread to whoever holds the phone: its model sees the thread's history, so "what did I ask
   earlier?" is answered aloud. *Recommendation:* when it comes, a call started on the lock screen
   binds a temporary thread (a negative `chat_thread`, kept out of the history and the feed) with
   the Kai folder's model; approvals and anything needing a decision require an unlock; the
   conversation and settings are not reachable from it. The alternative, the Kai thread itself while
   locked, matches Gemini's use without unlocking but exposes the history.
   *Answered 2026-10-14:* approved as recommended: later, on a temporary thread (§9.6).
8. **Screen context when invoked as the assistant.** *Recommendation:* never sent automatically; v1
   collects nothing (§9.2). Later, an "Ask about this screen" chip in the overlay sends a screenshot
   only when the owner taps it, once lmgw's realtime takes `input_image`. A fallback alias may carry
   it to a cloud model; that is the alias choice (ruling 10), not a guard of the phone's.
   *Answered 2026-10-14:* approved as recommended: never automatic (§9.7).
9. **A wake word.** *Recommendation:* none in v1; invocation by the side button, gesture, headset,
   tile and the app. The system's hotword APIs are privileged, and an own engine costs a permanent
   notification, a permanently lit microphone indicator and battery, and cannot start at boot.
   *Answered 2026-10-14:* approved as recommended: no wake word.
10. **Installing and updating.** The first usable version is installed with `just install` over adb.
    *Recommendation:* once it has proven usable, CI publishes signed APKs to the project's GitHub
    releases and Obtainium follows them; an
    in-app updater on lmgw's `lmgw-update` crate (as Kai's, kai §13.1) only if Obtainium proves
    awkward.
    *Answered 2026-10-14:* approved as recommended: adb first, then Obtainium (§15.4).
11. **The feed in the background (push) in v1, or later.** v3 recommended later, with the cost that
    a dashboard reply or an approval reached the phone only while the app was open.
    *Answered 2026-10-14:* push in the first version, recorded as ruling 6. The
    follow-up, approved: the Chat feed is the push channel (ruling 7). Kai's own foreground service
    holds the feed over the device link from the first version (M16, §12).

### Answers (the owner, 2026-10-14)

On draft v2:
- **Custom APIs for Kai clients:** recorded as ruling 5. v3 rewrote every place where v2 had kept
  lmgw's additions minimal: M9, M11, M16 and M21; §7.2 to §7.10; P1, P2, P3, P6, P7 and P8; WP0 and
  WP3; Q5.
- **The calls design's question on close reasons:** answered there. The last event on a bound call
  is `lmgw.session.closed {kind, by, message}`, and the phone reads its end reason from it alone
  (§7.8); the feed is no longer read for a call's end, and the wait for its word is gone.
- **The calls design's question on oversized events:** answered there. On bound calls they travel
  as `lmgw.event.part` messages in both directions (§7.10, P7).

On draft v3 (recorded in this v4, which is approved for implementation):
- **Q1:** a Samsung foldable on Android 16 / One UI 8.0, and a LineageOS device against a
  separate lmgw. Recorded in M15, M17, M18, M23, §9.4, §9.5, §10.6, §15, P12 and WP0 (a), (n),
  (o).
- **Q2:** approved as recommended and superseded by the device link: WireGuard is routing only,
  the link carries several URLs, no hairpinning, no tunnel gate, libwebrtc on every network.
  Recorded in ruling 8, M5, §5.1, §5.2, §7.11 and WP0 (f), (k).
- **Q3 to Q10:** approved as recommended. Recorded in §7.9 (Q3), §4.1 and §6.3 (Q4), §7.2 (Q5,
  after WP0), §10.2 (Q6), §9.6 (Q7), §9.7 (Q8), §9.4 (Q9) and §15.4 (Q10).
- **Q11:** push in the first version, ruling 6; the follow-up, approved: the Chat feed
  is the push channel, ruling 7. Recorded in M10, M16, M20, M22, §5.4, §8.6, §11, §12, WP0 (m) and
  WP2.
- **The device link** (lmgw's design, approved the same day): ruling 8, and every conflict its §6.4
  lists is resolved (the appendix).
- **The calls design v3's final names:** the barge-in verdict is `output_audio_buffer.cleared`; the
  folder's 404 is `chat_folder_not_found`; during `bind_pending` other client events get an `error`
  with code `bind_pending`, so the phone sends nothing but `lmgw.session.take_over`, or hangs up;
  `takeover=ask` exists on calls only; `CloseKind` gains `ThreadBound`; playout reports count as
  stopped after one `realtime.ping_interval_s`. Recorded in M11, "What exists today", §7.3, §7.6,
  §7.8, §7.9, P1, P2 and P8.
- **Foldable screens:** adaptive layouts by window size class, a fully resizable app,
  continuity across a fold mid-call, the overlay on both screens, the tabletop posture Later.
  Recorded in M23 and §10.6.

## Later

Decided during the build, each on the measurement it names:
- **The in-place rebind:** on `folder.current` or New conversation, the running call moves to the
  new thread instead of ending at its next quiet point and binding again (§7.7). Decided after
  WP3, with what the rebind costs there in hand. If lmgw then also tells a bound call about another
  principal's turn (K16), a call would need nothing from the feed.
- **The cut on the phone's clock** (P6): `lmgw.input_audio_buffer.commit {captured_until_ntp_ms}`
  and `.clear {captured_from_ntp_ms}` in place of the release tail's fit inside lmgw's window
  (§7.5). Decided after WP0 and lmgw's interop rig.
- **A pre-roll** (`lmgw.input_audio.preroll`, Q5), decided after WP0 on its merits.
- **HTTP/2 to lmgw's device listener,** so a call's POST rides the held feed's connection and no
  handshake sits on the invocation's path (§7.2). lmgw's Later item, weighed against WP0 (e)'s
  handshake cost.

After the first usable version (M22), each waiting for a reason:
- **The local message cache** (rusqlite, an instant start without the re-read): decided on WP2's
  measured start time. The feed's cursor is persisted from the first version already (M10).
- **The threads browser and search**, and voice in other threads: the Kai conversation comes first.
- **Markdown tables** rendered as tables: rare in spoken replies; source text until then.
- **A Baseline Profile module** (Macrobenchmark): once there are screens worth profiling.
- **Publishing for Obtainium** (Q10).
- **The lock screen,** on a temporary thread (Q7).
- **Silent certificate renewal** (lmgw's device link WP8): the phone renews at its first request
  after `notice_after`, with no user and the same key; until then re-pairing renews (§4.4).
- **Bringing the tunnel up from Kai** when no URL answers away from home, through the WireGuard
  app's remote-control intents (behind its "Allow remote control apps" switch); unverified.

Later still:
- **Telecom (Core-Telecom):** car integration (Android Auto), a call-style notification.
- **Phone tools** (§13), with their own design.
- **Voice approvals** with Kai's phrase matcher (`kai/src/approvals/words.rs`) moved into a shared
  crate; the dictation route keeps the answer out of the conversation (api §6.4).
- **A real RecognitionService** on lmgw's ASR, so the system's speech recognition goes through lmgw
  while Kai is the assistant.
- **Screen context** (Q8), on the owner's tap, once lmgw's realtime takes `input_image`.
- **The tabletop posture** on foldables (M23, §10.6), with Jetpack WindowManager's `FoldingFeature`.
- **Attachments** from the camera and files into a thread.
- **The earpiece route** with the proximity sensor turning the screen off.
- **UniFFI's JNI generator** in place of JNA, once released and measured against WP0 (e).
- **iOS** (ruling 3), in `ios/`, on the same core (§14).

## Appendix: the device link's conflicts with v3, resolved (2026-10-14)

lmgw's device link design lists in its §6.4 where kai-mobile's v3 conflicted with it. Each is
resolved in this version:

| # | The conflict | Resolved in |
|---|---|---|
| 1 | M16, §5.4 and §12 ran the feed only while the app was visible or a call ran, with no push | rulings 6 and 7; M16; §5.4; §12 (Kai's `specialUse` service holds the feed) |
| 2 | Q11, the Later list, §16's lmgw-side Later list and "Not built" planned push through a separate push service, later | Q11 answered; ruling 7 rules that service out, and nothing else names it |
| 3 | M10 and §5.4 started the feed at "now" on every start | M10; §5.4 (the persisted cursor, `caught_up`, `hello.open_approvals`) |
| 4 | M5 and §5.2: the VPN-only `networkIgnoreMask` breaks a LAN-URL call; the tunnel gate, its identification, the process binding and `dev.allow_untunnelled` lose their reason | M5 (every network, argued, with the rejected options); §5.2; §3.4 |
| 5 | M7 and §4.4 refused every link with `fp` and defined `fp` as the leaf's SHA-256 | M7 and §4.4: TLS from the first version, `fp` the CA's SubjectPublicKeyInfo hash |
| 6 | P4 moved `DeviceKey`, `verify_fingerprint` and `plain_http_remote` with a v=1 parser | P4: v=2, `PairingToken`, the enrolment verifier, `https` only, the enrolment builder and readers, the `tls` feature |
| 7 | M6, §3.3 and §4.3: the bearer encrypted with a Keystore AES-GCM key into `credentials.bin` | M6; §3.3 (`link.json`, all public); §4.3 (a non-extractable signing key) |
| 8 | M8 and §5.3: "WireGuard off" and "another VPN" as gates | M8; §5.3 (waiting for confirmation, the code used by someone else, the certificate's lapse) |
| 9 | Q2 and §5.1: hairpinning, no release switch, WP0 (k) | Q2 answered; §5.1; WP0 (k) now checks the URLs at home |
| 10 | §4.2: pairing's probe through the tunnel | §4.2: enrolment, the confirmation, the first mTLS feed |
| 11 | §5.5's refusal kinds | §5.5 and §7.8: `client_certificate_required`, `credential_on_device_listener`, `device_unconfirmed`, `device_certificate_expired`, `certificate_expired` |
| 12 | §13: the host link by "the device key"; offline until push or a background service | M19 and §13: the certificate; the held service |
| 13 | §16: P3's QR code, P4, and TLS, `fp` and push on lmgw's Later list | P3 (no QR code), P4, P9 to P12; §16's Later list |
