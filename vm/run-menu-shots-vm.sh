#!/bin/zsh
# run-menu-shots-vm.sh: screenshot PhotoCraft's native macOS menus with XCUITest, inside a
# disposable clone of craft-uitest-golden (artcraft-hub/docs/ui-test-vm.md). Never on the host.
#
# Usage: run-menu-shots-vm.sh <PhotoCraft.app> <menushots project dir> <fixtures dir> <results dir> [--graphics]
set -euo pipefail

APP_SRC="$1"; PROJ_SRC="$2"; FIX_SRC="$3"; RESULTS_DIR="$4"; GRAPHICS="${5:-}"
GOLDEN="craft-uitest-golden"
GUEST_RESULTS="/Users/admin/results"
RUN_ID="$(date +%Y%m%d-%H%M%S)-$$"
CLONE="craft-uitest-photocraft-$RUN_ID"
EXPORT="$(mktemp -d "${TMPDIR:-/tmp}/craft-uitest-export.XXXXXX")"
BOOT_TIMEOUT_SECS=120
LEASE_ID=""
T0=$SECONDS

log()  { print -r -- "==> $*"; }
fail() { print -r -- "!! $*" >&2; exit 1; }

cleanup() {
  local rc=$?
  trap - EXIT
  log "Cleaning up (exit $rc)"
  if [[ -n "$LEASE_ID" ]]; then tart-lease release --id "$LEASE_ID" 2>/dev/null || true; fi
  tart stop "$CLONE" >/dev/null 2>&1 || true
  tart delete "$CLONE" >/dev/null 2>&1 || true
  rm -rf "$EXPORT"
  log "Wall clock: $(( SECONDS - T0 )) s"
}
trap cleanup EXIT

command -v tart >/dev/null || fail "Tart is not installed"
tart list | awk '{print $2}' | grep -qx "$GOLDEN" || fail "Golden image '$GOLDEN' not found"
free_kb=$(df -k / | awk 'NR == 2 { print $4 }')
(( free_kb / 1024 / 1024 >= 20 )) || fail "Only $((free_kb / 1024 / 1024)) GiB free on /; need 20 GiB"
# --- Memory (generic memory-signal protocol) -------------------------------
# The guest needs its RAM plus host build headroom. This script states the
# need through the memory-signal protocol and waits for this machine's
# observer to free memory; what the observer does (unload cached AI models,
# drop caches, ask the user) is configured on the machine, never here.
# See PROTOCOL.md in the protocol's home for the wire format.

# Keep in sync with the golden image's memory (`tart set --memory`).
typeset -g GUEST_MEM_MB=12288

memory_available_bytes() {
  local page free inactive purgeable
  page=$(sysctl -n vm.pagesize)
  free=$(vm_stat | awk '/Pages free/ {gsub("\\.","",$3); print $3}')
  inactive=$(vm_stat | awk '/Pages inactive/ {gsub("\\.","",$3); print $3}')
  purgeable=$(vm_stat | awk '/Pages purgeable/ {gsub("\\.","",$3); print $3}')
  print -r -- $(( (free + inactive + purgeable) * page ))
}

# 0 = a live observer owns the spool (heartbeat fresh), 1 = none.
memory_observer_live() {
  local dir hb now mtime
  dir="${MEMORY_COORDINATION_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/memory-coordination}"
  hb="$dir/heartbeat"
  [[ -f "$hb" ]] || return 1
  now=$(date +%s)
  mtime=$(stat -f %m "$hb" 2>/dev/null) || return 1
  (( now - mtime <= 15 ))
}

# Emits a request and polls for ready/failed. 0 ready, 3 failed, 4 timeout
# or no observer. Sets MEMORY_REQUEST_ID for the release in cleanup.
memory_request_and_wait() {
  local need_bytes=$1 reason=$2 timeout=${3:-120}
  local dir id req deadline
  dir="${MEMORY_COORDINATION_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/memory-coordination}"
  id="run-ui-tests-vm-$$-$(date +%s)"
  req="$dir/requests/$id.json"
  MEMORY_REQUEST_ID="$id"
  MEMORY_COORD_DIR="$dir"

  memory_observer_live || { MEMORY_REQUEST_ID=""; return 4; }

  mkdir -p "$dir/requests" "$dir/ready" "$dir/failed" "$dir/release"
  /usr/bin/python3 - "$req" "$id" "$need_bytes" "$reason" "$$" <<'PY'
import json, os, sys, datetime
path, rid, need, reason, pid = sys.argv[1:6]
doc = {
    "id": rid,
    "resource": "memory",
    "bytes": int(need),
    "requester": "run-ui-tests-vm",
    "reason": reason,
    "pid": int(pid),
    "created": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
}
with open(path, "w") as f:
    json.dump(doc, f, indent=2)
    f.write("\n")
PY

  deadline=$(( SECONDS + timeout ))
  while (( SECONDS < deadline )); do
    [[ -f "$dir/ready/$id.json" ]] && return 0
    if [[ -f "$dir/failed/$id.json" ]]; then
      /usr/bin/python3 -c 'import json,sys; print("memory-signal failed:", json.load(open(sys.argv[1])).get("reason","?"))' "$dir/failed/$id.json" >&2
      return 3
    fi
    sleep 2
  done
  return 4
}

need_bytes=$(( (GUEST_MEM_MB + 4096) * 1024 * 1024 ))
avail_bytes=$(memory_available_bytes)
if (( avail_bytes < need_bytes )); then
  log "Host memory short ($(( avail_bytes / 1073741824 )) GiB of $(( need_bytes / 1073741824 )) GiB) — requesting via the memory-signal protocol"
  set +e
  memory_request_and_wait "$need_bytes" "Tart VM UI-test run needs guest RAM plus build headroom" 120
  mem_rc=$?
  set -e
  (( mem_rc == 0 )) || fail "Memory request not fulfilled (rc=$mem_rc). Free memory, or set up a memory-signal observer (MEMORY_COORDINATION_DIR=${MEMORY_COORDINATION_DIR:-$HOME/.local/state/memory-coordination})"
  avail_bytes=$(memory_available_bytes)
  (( avail_bytes >= need_bytes )) || fail "Observer signaled ready but memory is still short ($(( avail_bytes / 1073741824 )) GiB of $(( need_bytes / 1073741824 )) GiB)"
fi

# Sweep only this script's own run-ID shape; never the golden.
for old in $(tart list | awk '$2 ~ /^craft-uitest-photocraft-[0-9]{8}-[0-9]{6}-[0-9]+$/ { print $2 }'); do
  [[ "$old" == "$GOLDEN" ]] && { print -r -- "!! refusing to sweep the golden" >&2; continue; }
  log "Sweeping stale clone: $old"
  tart stop "$old" >/dev/null 2>&1 || true
  tart delete "$old" >/dev/null 2>&1 || true
done

mkdir -p "$EXPORT/app" "$EXPORT/fixtures"
ditto "$APP_SRC" "$EXPORT/app/PhotoCraft.app"
ditto "$PROJ_SRC" "$EXPORT/menushots"
rm -rf "$EXPORT/menushots/build"
cp "$FIX_SRC"/* "$EXPORT/fixtures/"

if command -v tart-lease >/dev/null; then
  LEASE_ID=$(tart-lease acquire --label craft-photocraft --pid $$)
else
  log "WARNING: tart-lease not on PATH"
fi

log "Cloning $GOLDEN -> $CLONE"
tart clone "$GOLDEN" "$CLONE"
if [[ "$GRAPHICS" == "--graphics" ]]; then
  log "Booting with graphics (a Tart window opens; nothing runs on the host desktop)"
  tart run "$CLONE" --dir=run:"$EXPORT":ro >"$EXPORT/tart-run.log" 2>&1 &
else
  log "Booting headless"
  tart run "$CLONE" --no-graphics --dir=run:"$EXPORT":ro >"$EXPORT/tart-run.log" 2>&1 &
fi
boot_deadline=$(( SECONDS + BOOT_TIMEOUT_SECS ))
until tart exec "$CLONE" true >/dev/null 2>&1; do
  (( SECONDS < boot_deadline )) || fail "Guest not reachable within ${BOOT_TIMEOUT_SECS}s"
  sleep 5
done
log "Guest reachable after $(( SECONDS - T0 )) s"

R="/Volumes/My Shared Files/run"
tart exec "$CLONE" /bin/zsh -lc "sw_vers; rm -rf ~/menushots ~/fixtures $GUEST_RESULTS && mkdir -p $GUEST_RESULTS/shots \
  && ditto '$R/app/PhotoCraft.app' /Applications/PhotoCraft.app \
  && /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f /Applications/PhotoCraft.app \
  && ditto '$R/menushots' ~/menushots && ditto '$R/fixtures' ~/fixtures \
  && xattr -l /Applications/PhotoCraft.app | head -3; echo copied"

set +e
if [[ -z "${ONLY_TESTS:-}" ]]; then
  log "Running XCUITest in the guest"
  tart exec "$CLONE" /bin/zsh -lc "set -o pipefail; cd ~/menushots && xcodebuild -project MenuShots.xcodeproj -scheme MenuShots \
    -destination 'platform=macOS' -only-testing:MenuShotsUITests/MenuShotsUITests \
    -resultBundlePath $GUEST_RESULTS/MenuShots.xcresult test 2>&1 | tee $GUEST_RESULTS/xcodebuild.log"
  test_rc=$?
else
  # One test per xcodebuild run (ONLY_TESTS="Class/testA Class/testB"), and after each: is
  # PhotoCraft still running, any new crash report, AppKit's exception log lines, and the saved
  # window layout's modification time.
  tart exec "$CLONE" /bin/zsh -lc "cd ~/menushots && xcodebuild -project MenuShots.xcodeproj -scheme MenuShots \
    -destination 'platform=macOS' -derivedDataPath ~/dd build-for-testing > $GUEST_RESULTS/build.log 2>&1; tail -1 $GUEST_RESULTS/build.log"
  test_rc=0
  for t in ${=ONLY_TESTS}; do
    name="${t##*/}"
    log "Running $name in the guest"
    tart exec "$CLONE" /bin/zsh -lc "set -o pipefail; cd ~/menushots && start=\$(date '+%Y-%m-%d %H:%M:%S') \
      && xcodebuild -project MenuShots.xcodeproj -scheme MenuShots -destination 'platform=macOS' -derivedDataPath ~/dd \
         -only-testing:MenuShotsUITests/$t -resultBundlePath $GUEST_RESULTS/$name.xcresult test-without-building \
         > $GUEST_RESULTS/$name.log 2>&1; rc=\$?; sleep 3
      { echo \"test rc: \$rc\"
        echo \"still running: \$(pgrep -x PhotoCraft || echo no)\"
        echo \"crash reports:\"; ls ~/Library/Logs/DiagnosticReports 2>/dev/null | grep -i photocraft
        echo \"ui.ron:\"; find ~/Library -name ui.ron -path '*hoto*' -exec stat -f '%Sm %N' {} + 2>/dev/null
        echo \"AppKit log:\"; log show --style compact --start \"\$start\" --predicate 'process == \"PhotoCraft\" AND (eventMessage CONTAINS[c] \"observer\" OR eventMessage CONTAINS[c] \"exception\")' 2>/dev/null | tail -5
      } > $GUEST_RESULTS/$name.txt
      pkill -9 -x PhotoCraft; exit \$rc"
    (( $? == 0 )) || test_rc=1
  done
fi
tart exec "$CLONE" /bin/zsh -lc "ls ~/Library/Logs/DiagnosticReports 2>/dev/null | tail -5 > $GUEST_RESULTS/guest-crashes.txt; \
  mkdir -p $GUEST_RESULTS/crashes && cp ~/Library/Logs/DiagnosticReports/PhotoCraft*.ips $GUEST_RESULTS/crashes/ 2>/dev/null; true"
set -e

mkdir -p "$RESULTS_DIR"
tart exec "$CLONE" /bin/zsh -lc "tar -C $GUEST_RESULTS -cf - ." | tar -x -C "$RESULTS_DIR"
grep -hE 'error:|Test Case .* (passed|failed)|TEST (SUCCEEDED|FAILED)' "$RESULTS_DIR"/*.log 2>/dev/null | tail -20 || true
log "Results: $RESULTS_DIR"
exit "$test_rc"
