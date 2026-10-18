#!/usr/bin/env python3
"""Fill spikes/wp0/RESULTS.md's measured blocks from the spike's pulled logs.

Usage: python3 -I spikes/wp0/fill-results.py [--results spikes/wp0/results] [--doc spikes/wp0/RESULTS.md] [--print]
  (just spike-log runs it after each pull.)

Every wp0.jsonl under --results is one phone's log (the phone's file only grows, so each pull holds
the earlier ones; identical lines are counted once). The blocks between <!-- auto:X --> and
<!-- /auto:X --> are rewritten from all of them, grouped by phone; nothing else in the document is
touched. --print writes the blocks to stdout instead.
"""

import argparse
import json
import re
import statistics
import sys
from collections import defaultdict
from pathlib import Path


def load(results: Path):
    """{phone: [event, ...]} with each distinct line once, in time order."""
    phones = defaultdict(dict)
    for path in sorted(results.rglob("wp0.jsonl")):
        lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
        events = []
        for line in lines:
            line = line.strip()
            if not line:
                continue
            try:
                events.append((line, json.loads(line)))
            except json.JSONDecodeError:
                continue
        starts = [e for _, e in events if e.get("point") == "app" and e.get("event") == "start"]
        phone = (
            f"{starts[-1].get('manufacturer', '?')} {starts[-1].get('model', '?')}" if starts else path.parent.name
        )
        for line, e in events:
            phones[phone][line] = e
    return {p: sorted(d.values(), key=lambda e: (e.get("ts", ""), e.get("elapsed_ms", 0))) for p, d in phones.items()}


def pick(events, point, event=None):
    return [e for e in events if e.get("point") == point and (event is None or e.get("event") == event)]


def stats(values):
    v = sorted(x for x in values if isinstance(x, (int, float)))
    if not v:
        return "none"
    p90 = v[min(len(v) - 1, max(0, -(-9 * len(v) // 10) - 1))]
    return f"n={len(v)} median {statistics.median(v):.1f} p90 {p90:.1f} max {v[-1]:.1f}"


def phones_block(data):
    out = []
    for phone, events in data.items():
        s = pick(events, "app", "start")
        last = s[-1] if s else {}
        out.append(
            f"- **{phone}**: Android {last.get('android_release', '?')} (SDK {last.get('sdk_int', '?')}), "
            f"build {last.get('build_display', '?')}"
            + (f", One UI {last['oneui']}" if last.get("oneui") else "")
            + (f", LineageOS {last['lineage']}" if last.get("lineage") else "")
            + f", page size {last.get('page_size', '?')}; {len(events)} log lines"
        )
    return out


def block_a(events):
    out = []
    frames = pick(events, "a", "first_frame")
    groups = defaultdict(list)
    for f in frames:
        kind = "cold" if f.get("cold") else "warm"
        src = "system" if f.get("source") == "system" else f"app ({f.get('source')})"
        groups[(src, kind, f.get("posture") or "?")].append(f)
    if frames:
        out.append("")
        out.append("| Source | Process | Screen | onPrepareShow → frame (ms) | onShow → frame (ms) | invocation → frame (ms) |")
        out.append("|---|---|---|---|---|---|")
        for (src, kind, posture), fs in sorted(groups.items()):
            ref = [f.get("ref_to_frame_ms") for f in fs if f.get("ref_to_frame_ms") is not None]
            keys = sorted({f.get("ref_key") for f in fs if f.get("ref_key")})
            out.append(
                f"| {src} | {kind} | {posture} | {stats(f.get('prepare_to_frame_ms') for f in fs)} | "
                f"{stats(f.get('show_to_frame_ms') for f in fs)} | {stats(ref)}{' (' + ', '.join(keys) + ')' if keys else ''} |"
            )
        out.append("")
        ages = [f.get("process_age_at_prepare_ms") for f in frames if f.get("cold")]
        if ages:
            out.append(f"- cold shows: process age at onPrepareShow {stats(ages)} ms")
        flags = sorted({f.get("show_flags") for f in frames})
        out.append(f"- show_flags seen: {flags}; argument keys: {sorted({k for f in frames for k in (f.get('args') or {})})}")
    else:
        out.append("- no first frame logged yet")
    for d in pick(events, "a", "role_dialog"):
        out.append(f"- role request: a dialog showed: {d.get('shown')} ({d.get('ts', '')[:16]})")
    for r in pick(events, "a", "role_request_result"):
        out.append(f"- role request result {r.get('result_code')} after {r.get('after_ms')} ms, held after: {r.get('role_held')}")
    st = pick(events, "a", "status")
    if st:
        s = st[-1]
        out.append(
            "- setup: role held {role_held}, active service {active_service}, config_longPressOnPowerBehavior "
            "{config_longPressOnPowerBehavior}, power_button_long_press {global_power_button_long_press}".format(
                **{k: s.get(k) for k in ("role_held", "active_service", "config_longPressOnPowerBehavior",
                                          "global_power_button_long_press")}
            )
        )
    for x in pick(events, "a", "expand_call"):
        out.append(f"- Expand by {x.get('how')}: call ok {x.get('ok')}" + (f", {x.get('error')}" if x.get("error") else ""))
    for x in pick(events, "a", "expand_arrived"):
        out.append(f"- Expand by {x.get('how')}: the app's first frame {x.get('first_frame_ms')} ms after the tap ({x.get('posture')})")
    ready = pick(events, "a", "vis_ready")
    if ready:
        out.append(
            f"- onReady: {len(ready)} times; process age at onReady {stats(r.get('process_age_ms') for r in ready)} ms; "
            f"libwebrtc prepared in {stats((r.get('webrtc') or {}).get('initialize_ms', 0) + (r.get('webrtc') or {}).get('adm_ms', 0) + (r.get('webrtc') or {}).get('factory_ms', 0) for r in ready)} ms"
        )
    disabled = None
    assist = defaultdict(int)
    for e in events:
        if e.get("point") == "a" and e.get("event") == "vis_ready":
            disabled = e.get("disabled_show_context")
        if e.get("point") == "a" and e.get("event") in ("handle_assist", "handle_screenshot"):
            assist[(disabled, e["event"])] += 1
    for (dis, ev), n in sorted(assist.items(), key=str):
        out.append(f"- {ev}: {n} times while disabledShowContext was {dis}")
    return out


def block_b(events):
    out = []
    fg = pick(events, "b", "foreground")
    if fg:
        out.append("")
        out.append("| Origin | Step | Types | OK | Activity visible | Overlay showing | Error |")
        out.append("|---|---|---|---|---|---|---|")
        for f in fg:
            out.append(
                f"| {f.get('origin')} | {f.get('step')} | {f.get('types')} | {f.get('ok')} | {f.get('activity_visible')} | "
                f"{f.get('overlay_showing')} | {f.get('error') or ''} |"
            )
        out.append("")
    for s in pick(events, "b", "service_start"):
        if not s.get("ok"):
            out.append(f"- service start refused from {s.get('origin')}: {s.get('error')} {s.get('message')}")
    for s in pick(events, "b", "capture_summary"):
        ping = s.get("ping") or {}
        on = ping.get("screen_on") or {}
        off = ping.get("screen_off") or {}
        out.append(
            f"- capture from {s.get('origin')} ({s.get('ts', '')[:16]}): {s.get('duration_s')} s, screen off {s.get('screen_off_s')} s, "
            f"lost {s.get('lost_ms')} ms, longest gap {s.get('max_gap_ms')} ms, gaps over 100 ms {s.get('gaps_over_100ms')}, "
            f"silenced {s.get('silenced_ever')}, stopped by {s.get('stopped_by')}"
            + (
                f"; ping {ping.get('target')}: screen on median {on.get('median_ms')} p95 {on.get('p95_ms')} ms (n={on.get('count')}), "
                f"screen off median {off.get('median_ms')} p95 {off.get('p95_ms')} ms (n={off.get('count')}), lost {ping.get('lost')}/{ping.get('sent')}"
                if ping else ""
            )
        )
    if not out:
        out.append("- nothing logged yet")
    return out


def block_d(events):
    runs = pick(events, "d", "run")
    if not runs:
        return ["- nothing logged yet"]
    out = [
        "| Screen | Mode | Volume | False barge-ins | After NLMS | Echo median / max (dBFS) | Noise floor | AEC on | Recording |",
        "|---|---|---|---|---|---|---|---|---|",
    ]
    for r in runs:
        out.append(
            f"| {r.get('posture')} | {r.get('mode')} | {r.get('volume_index')}/{r.get('volume_max')} | {r.get('barge_ins')} | "
            f"{'' if r.get('software_barge_ins') is None else r.get('software_barge_ins')} | {r.get('echo_median_db')} / {r.get('echo_max_db')} | "
            f"{r.get('noise_floor_db')} | {r.get('aec_enabled')} | {r.get('wav')} |"
        )
    return out


def block_e(events):
    results = [r.get("result") or {} for r in pick(events, "e", "result")]
    if not results:
        return ["- nothing logged yet"]
    out = []
    last = results[-1]
    out.append(f"- APK {last.get('apk_bytes', 0) / 1048576:.1f} MiB, page size {last.get('page_size')}, every library 16 KB aligned: {last.get('libs_all_16k')}")
    for lib in last.get("libs") or []:
        out.append(f"  - {lib.get('name')}: {lib.get('size', 0) / 1048576:.2f} MiB, stored {lib.get('stored')}, zip 16 KB {lib.get('zip_16k')}, p_align {lib.get('min_load_align')}")
    out.append("")
    out.append("| Run | Keep attached | JNA (ms) | Core first call (ms) | Sink: calling / worker / fresh (µs, median) | Attach+detach (µs) | Sign cold / warm (ms) | Handshake first / warm median (ms) | Its signature (ms, median) |")
    out.append("|---|---|---|---|---|---|---|---|---|")
    for r in results:
        if not r.get("ok"):
            out.append(f"| {r.get('where')} | | failed: {r.get('error')} | | | | | | |")
            continue
        sink = r.get("sink") or {}
        ks = r.get("keystore") or {}
        tls = r.get("tls") or {}
        med = lambda o, k="median_us": (o or {}).get(k)
        out.append(
            f"| {r.get('where')} | {r.get('keep_attached')} | {r.get('jna_init_ms')} | {r.get('core_first_call_ms')} | "
            f"{med(sink.get('calling_thread'))} / {med(sink.get('worker_repeat_call'))} / {med(sink.get('fresh_thread'))} | "
            f"{sink.get('attach_detach_us')} | {(med(ks.get('sign'), 'first_us') or 0) / 1000:.2f} / {(med(ks.get('sign_warm')) or 0) / 1000:.2f} | "
            f"{(med(tls.get('handshake'), 'first_us') or 0) / 1000:.2f} / {(med(tls.get('handshake_warm')) or 0) / 1000:.2f} | "
            f"{(med(tls.get('sign')) or 0) / 1000:.2f} |"
        )
    out.append("")
    tls = last.get("tls") or {}
    if tls:
        out.append(f"- TLS: {tls.get('protocol')} {tls.get('cipher_suite')} {tls.get('key_exchange')}, client certificate verified {tls.get('client_cert_verified')}; Keystore key in {(last.get('keystore') or {}).get('security_level')}")
    return out


def block_i(events):
    out = []
    keys = defaultdict(int)
    for b in pick(events, "i", "button"):
        keys[(b.get("key"), b.get("action"), b.get("long_press"), b.get("input_device"), b.get("usage"), b.get("mode_in_communication"))] += 1
    if keys:
        out.append("")
        out.append("| Key | Action | Long press | Input device | Usage | In communication | Count |")
        out.append("|---|---|---|---|---|---|---|")
        for (k, a, lp, dev, usage, comm), n in sorted(keys.items(), key=str):
            out.append(f"| {k} | {a} | {lp} | {dev} | {usage} | {comm} | {n} |")
        out.append("")
    for h in pick(events, "i", "hfp"):
        out.append(
            f"- HFP {h.get('device')}: accepted {h.get('accepted')}, device changed after {h.get('sco_changed_ms')} ms, "
            f"recording routed after {h.get('routed_ms')} ms; rates client {h.get('client_rate')} device {h.get('device_format_rate')} "
            f"(offers {h.get('device_rates')}); {h.get('bandwidth')}" + (f"; {h.get('error')}" if h.get("error") else "")
        )
    names = {(d.get("device") or {}).get("type"): (d.get("device") or {}).get("product") for d in pick(events, "i", "device_added")}
    if names:
        out.append(f"- devices seen: {names}")
    return out or ["- nothing logged yet"]


def block_n(events):
    out = []
    by = {}
    for m in pick(events, "n", "metrics"):
        w = m.get("window") or {}
        by[w.get("posture")] = m
    for posture, m in by.items():
        w = m.get("window") or {}
        c = m.get("configuration") or {}
        out.append(
            f"- {posture}: {w.get('width_px')}×{w.get('height_px')} px, {w.get('width_dp')}×{w.get('height_dp')} dp "
            f"(density {w.get('density')}), {w.get('width_class')} × {w.get('height_class')}; smallest width {c.get('smallest_width_dp')} dp"
        )
    caps = [c for c in pick(events, "n", "activity_created") if c.get("capture")]
    if caps:
        out.append("- recreations during a capture:")
        for c in caps:
            cap = c["capture"]
            out.append(f"  - {c.get('posture')} {c.get('width_dp')} dp: capture {cap.get('duration_s')} s, lost {cap.get('lost_ms')} ms, longest gap {cap.get('max_gap_ms')} ms")
    layouts = pick(events, "a", "overlay_layout")
    if layouts:
        out.append("- overlay sizes: " + "; ".join(sorted({f"{l.get('posture')} {l.get('width_dp')}×{l.get('height_dp')} dp" for l in layouts})))
    gone = [d for d in pick(events, "n", "activity_destroyed") if not d.get("changing_configurations")]
    if gone:
        out.append(f"- destroyed without a configuration change: {len(gone)} times (finishing: {sum(1 for d in gone if d.get('finishing'))})")
    return out or ["- nothing logged yet"]


def block_o(events):
    checks = pick(events, "o", "selfcheck")
    if not checks:
        return ["- nothing logged yet"]
    c = checks[-1]
    out = [f"- {c.get('ts', '')[:16]}: {c.get('passed')} passed, {c.get('failed')} failed, {c.get('skipped')} skipped"]
    for item in c.get("items") or []:
        verdict = {True: "pass", False: "FAIL", None: "skip"}[item.get("ok")]
        out.append(f"  - {verdict} · {item.get('name')} · {item.get('ms')} ms · {item.get('detail')}")
    return out


BLOCKS = {"a": block_a, "b": block_b, "d": block_d, "e": block_e, "i": block_i, "n": block_n, "o": block_o}


def render(data):
    blocks = {"phones": phones_block(data) or ["*(no log pulled yet)*"]}
    for key, fn in BLOCKS.items():
        lines = []
        for phone, events in data.items():
            body = fn(events)
            if body and body != ["- nothing logged yet"]:
                lines += [f"**{phone}**", ""] + body + [""]
        blocks[key] = lines or ["*(nothing logged yet)*"]
    return blocks


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    here = Path(__file__).resolve().parent
    ap.add_argument("--results", type=Path, default=here / "results")
    ap.add_argument("--doc", type=Path, default=here / "RESULTS.md")
    ap.add_argument("--print", action="store_true", help="write the blocks to stdout, leave the document alone")
    args = ap.parse_args()
    data = load(args.results) if args.results.is_dir() else {}
    blocks = render(data)
    if args.print:
        for k, v in blocks.items():
            print(f"== {k}\n" + "\n".join(v))
        return 0
    text = args.doc.read_text(encoding="utf-8")
    for key, lines in blocks.items():
        pattern = re.compile(rf"(<!-- auto:{re.escape(key)} -->\n).*?(\n<!-- /auto:{re.escape(key)} -->)", re.S)
        if not pattern.search(text):
            print(f"warning: {args.doc} has no auto:{key} block", file=sys.stderr)
            continue
        body = "\n".join(lines).rstrip("\n")
        text = pattern.sub(lambda m: m.group(1) + body + m.group(2), text)
    args.doc.write_text(text, encoding="utf-8")
    print(f"{args.doc}: measured blocks rewritten from {sum(len(v) for v in data.values())} log lines of {len(data)} phone(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
