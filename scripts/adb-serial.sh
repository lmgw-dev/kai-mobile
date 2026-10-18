#!/usr/bin/env bash
# Print the one adb serial to use (host adb, not the container).
# Usage: scripts/adb-serial.sh        (just install; `ANDROID_SERIAL=… ` is honoured as it is)
#   One phone may be listed several times: over USB, over wireless debugging (`adb-<serial>-…`) and
#   after `adb connect ip:port`. Entries are grouped by the phone's own serial (ro.serialno, asked
#   through each entry; `adb get-serialno` only echoes the transport's name), never by name pattern.
#   The USB entry is preferred, then any other. Only `device` states count. Two distinct phones fail
#   with a message naming them.
set -euo pipefail
if [[ -n "${ANDROID_SERIAL:-}" ]]; then
  echo "$ANDROID_SERIAL"
  exit 0
fi
mapfile -t entries < <(adb devices | awk 'NR > 1 && $2 == "device" {print $1}')
((${#entries[@]} > 0)) || { echo "no adb device in state 'device' (adb devices; pair the phone first)" >&2; exit 1; }
declare -A best      # phone serial -> chosen entry
declare -A is_usb    # phone serial -> 1 once the chosen entry is a USB one
for entry in "${entries[@]}"; do
  phone=$(adb -s "$entry" shell getprop ro.serialno | tr -d '\r\n')
  [[ -n "$phone" ]] || phone=$entry
  usb=0
  [[ "$entry" != *:* && "$entry" != adb-*_adb-tls-* ]] && usb=1
  if [[ -z "${best[$phone]:-}" ]] || { ((usb)) && [[ -z "${is_usb[$phone]:-}" ]]; }; then
    best[$phone]=$entry
    ((usb)) && is_usb[$phone]=1
  fi
done
if ((${#best[@]} > 1)); then
  echo "more than one phone is attached: ${!best[*]}; set ANDROID_SERIAL" >&2
  exit 1
fi
echo "${best[${!best[*]}]}"
