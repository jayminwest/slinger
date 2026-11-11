#!/usr/bin/env bash
# Three slings against the real Claude Code (SPEC section 14, Spike C pass line).
#   scripts/e2e-sling.sh [--model NAME] [--timeout SECS] [--keep]
#
# NOT part of `cargo xtask check-all`: it needs a logged-in Claude Code (`claude`
# on PATH), spends real tokens, and adds three-sling conversation transcripts
# to Claude Code's own session store. Run it by hand.
#
# It builds slinger and slingerd, starts slingerd on a temporary socket and data
# directory, opens a scratch workspace, starts its session, makes a canvas with
# three text notes (and one ink stroke under note 1, so sling 1 carries an
# image), and sends three slings: sling i selects note i. Sling 1 gives a code
# word, sling 3 asks for it. Then it checks:
#   - every sling went into one session id;
#   - each reply landed as an item beside its selection (nearest that note);
#   - the record has sling_sent, reply_landed and agent_done 3 times each and
#     3 packet blobs;
#   - the third answer has the code word.
# It prints the time from each sling to its reply and to the agent's done, and
# the tokens of each turn read from the session's transcript.
#
#   --model NAME    the Claude Code model (default: haiku, to keep the cost
#                   down; `default` runs Claude Code's own default)
#   --timeout SECS  how long one sling may take to reach done (default: 240)
#   --keep          keep the scratch directory (its path is printed)
#
# Nothing touches the default socket, data directory or Claude Code config. On
# exit the session, slingerd and Claude Code are stopped.
set -euo pipefail
cd "$(dirname "$0")/.."

model=haiku
timeout=240
keep=0
while [ $# -gt 0 ]; do
  case "$1" in
    --model) model="${2:?--model needs a name}"; shift 2 ;;
    --timeout) timeout="${2:?--timeout needs seconds}"; shift 2 ;;
    --keep) keep=1; shift ;;
    -h | --help) sed -n '2,28p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "✗ unknown argument: $1" >&2; echo "  see scripts/e2e-sling.sh --help" >&2; exit 2 ;;
  esac
done

claude=$(command -v claude) || { echo "✗ claude is not on PATH" >&2; echo "  install Claude Code and log in" >&2; exit 2; }
command -v jq >/dev/null || { echo "✗ jq is not on PATH" >&2; exit 2; }

cargo build --quiet --locked --bin slinger --bin slingerd
bin="$(pwd)/${CARGO_TARGET_DIR:-target}/debug"

# A short path: a Unix socket path has a length limit.
tmp=$(mktemp -d /tmp/sl-e2e.XXXXXX)
sock="$tmp/s.sock"
daemon=""
agent_pid=""
cleanup() {
  if [ -n "$daemon" ]; then
    "$bin/slinger" --socket "$sock" session stop e2e >/dev/null 2>&1 || true
    kill "$daemon" 2>/dev/null || true
    wait "$daemon" 2>/dev/null || true
  fi
  if [ -n "$agent_pid" ] && kill -0 "$agent_pid" 2>/dev/null; then kill "$agent_pid" 2>/dev/null || true; fi
  if [ "$keep" = 1 ]; then echo "kept: $tmp" >&2; else rm -rf "$tmp"; fi
}
trap cleanup EXIT

# The program slingerd runs as the agent: Claude Code with the chosen model.
if [ "$model" = default ]; then
  printf '#!/bin/sh\nexec %q "$@"\n' "$claude" >"$tmp/claude"
else
  printf '#!/bin/sh\nexec %q --model %q "$@"\n' "$claude" "$model" >"$tmp/claude"
fi
chmod +x "$tmp/claude"

mkdir "$tmp/ws"
printf 'version = 1\nname = "e2e"\n[agent]\nname = "e2e"\n' >"$tmp/ws/slinger-workspace.toml"

"$bin/slingerd" --socket "$sock" --data-dir "$tmp/data" --claude "$tmp/claude" --slinger "$bin/slinger" \
  2>"$tmp/slingerd.log" &
daemon=$!
for _ in $(seq 50); do [ -S "$sock" ] && break; sleep 0.1; done
[ -S "$sock" ] || { echo "✗ slingerd did not start" >&2; cat "$tmp/slingerd.log" >&2; exit 1; }

s() { "$bin/slinger" --socket "$sock" "$@"; }

s workspace open "$tmp/ws" >/dev/null
s session start e2e
agent_pid=$(s --json session show e2e | jq -r '.session.pid // empty')

words=(heron quartz lantern juniper cobalt marigold tundra saffron)
word="${words[RANDOM % ${#words[@]}]}-$((RANDOM % 900 + 100))"
notes=(
  "Names for the garden shed project"
  "What is 17 times 23?"
  "Recall check"
)
ops=$(jq -n --arg a "${notes[0]}" --arg b "${notes[1]}" --arg c "${notes[2]}" '[
  {op: "add", item: {page: "p1", kind: "text", x: 0, y: 0, w: 3000, h: 400, text: $a}},
  {op: "add", item: {page: "p1", kind: "stroke", width: 20, pts: [0, 600, 1500, 0, 1500, 150]}},
  {op: "add", item: {page: "p1", kind: "text", x: 0, y: 6000, w: 3000, h: 400, text: $b}},
  {op: "add", item: {page: "p1", kind: "text", x: 0, y: 12000, w: 3000, h: 400, text: $c}}
]')
made=$(s --json canvas apply main "$ops" | jq -c '.made')
note_ids=("$(jq -r '.[0]' <<<"$made")" "$(jq -r '.[2]' <<<"$made")" "$(jq -r '.[3]' <<<"$made")")

prompts=(
  "The code word is $word. Remember it: I will ask for it later. Put a short note beside this one that says you have it."
  "Answer the question in this note, beside it."
  "What was the code word I gave you in my first sling? Put it beside this note."
)

count() { s --json record list --kind "$1" | jq '.events | length'; }

sends=()
for i in 0 1 2; do
  # The packet names a text item i<n> as t<n>.
  sel="t${note_ids[$i]#i}"
  sends+=("$(s --json sling send main --select "$sel" --prompt "${prompts[$i]}")")
  deadline=$((SECONDS + timeout))
  until [ "$(count agent_done)" -ge $((i + 1)) ] &&
    [ "$(s --json session show e2e | jq '.session.pending // 0')" = 0 ]; do
    if [ "$(s --json session show e2e | jq -r '.session.process // ""')" != running ]; then
      echo "✗ sling $((i + 1)): the agent's process is not running" >&2; tail -20 "$tmp/slingerd.log" >&2; exit 1
    fi
    if [ $SECONDS -ge $deadline ]; then
      echo "✗ sling $((i + 1)): no agent_done within ${timeout}s" >&2; tail -20 "$tmp/slingerd.log" >&2
      s session show e2e >&2 || true; exit 1
    fi
    sleep 1
  done
done
# A put after the turn's done still counts; give a late one a moment.
sleep 2

fail=0
check() { # check <ok?> <what>
  if [ "$1" = true ]; then echo "ok    $2"; else echo "FAIL  $2"; fail=1; fi
}

events=$(s --json record list | jq -c '.events')
items=$(s --json canvas show main | jq -c '.items')
ids=$(printf '%s\n' "${sends[@]}" | jq -s -c '[.[].session_id]')
sid=$(jq -r '.[0] // ""' <<<"$ids")

check "$(jq --arg s "$sid" '$s != "" and all(. == $s)' <<<"$ids")" "one session id: $sid"
for k in sling_sent reply_landed agent_done; do
  n=$(jq --arg k "$k" '[.[] | select(.kind == $k)] | length' <<<"$events")
  check "$([ "$n" = 3 ] && echo true || echo false)" "$k events: $n (want 3)"
done
packets=$(jq -r '[.[] | select(.kind == "sling_sent") | .payload.packet] | unique | .[]' <<<"$events")
np=0
for p in $packets; do s blob get "$p" --out "$tmp/packet-$np.txt" && np=$((np + 1)); done
check "$([ "$np" = 3 ] && echo true || echo false)" "packet blobs: $np (want 3)"
nimg=$(jq '[.[] | select(.kind == "sling_sent") | .payload.images // [] | length] | add' <<<"$events")
echo "info  images sent: $nimg (sling 1 carries the stroke under note 1)"

notes_json=$(printf '%s\n' "${note_ids[@]}" | jq -R . | jq -s -c .)
for i in 0 1 2; do
  ev=$(jq '.event' <<<"${sends[$i]}")
  landed=$(jq -c --argjson ev "$ev" '[.[] | select(.kind == "reply_landed" and .payload.sling == $ev) | .payload.items[]]' <<<"$events")
  # Each landed item must be nearer this sling's note than any other note.
  beside=$(jq -n --argjson items "$items" --argjson landed "$landed" --argjson notes "$notes_json" --arg want "${note_ids[$i]}" '
    def centre: [(.x // 0) + (.w // 0) / 2, (.y // 0) + (.h // 0) / 2];
    def dist(a; b): ((a[0] - b[0]) * (a[0] - b[0]) + (a[1] - b[1]) * (a[1] - b[1]));
    ($items | map({key: .id, value: .}) | from_entries) as $by
    | ($landed | length) > 0 and all($landed[]; . as $id | $by[$id] as $it
        | $it != null and $it.x != null
        and ($notes | min_by(dist($by[.] | centre; $it | centre))) == $want)')
  text=$(jq -r --argjson items "$items" '. as $l | [$items[] | select(.id as $id | $l | index($id)) | .text // empty] | join(" / ")' <<<"$landed")
  check "$beside" "sling $((i + 1)): reply $landed beside ${note_ids[$i]}: \"$text\""
  if [ "$i" = 2 ]; then
    check "$(jq -n --arg t "$text" --arg w "$word" '$t | ascii_downcase | contains($w)')" "third answer has the code word $word"
  fi
done

# Time from each sling to its first reply and to the agent's done, from the record.
jq -r '
  [.[] | select(.kind == "sling_sent")] as $sl
  | range(0; $sl | length) as $i | $sl[$i] as $s
  | ([.[] | select(.kind == "reply_landed" and .payload.sling == $s.number)] | first) as $r
  | ([.[] | select(.kind == "agent_done" and .number > $s.number)] | first) as $d
  | "time  sling \($i + 1): reply \(if $r then "\(($r.time - $s.time) / 1000) s" else "-" end), done \(if $d then "\(($d.time - $s.time) / 1000) s" else "-" end)"
' <<<"$events"

# Tokens per sling, from the session's transcript in Claude Code's own store.
transcript=$(find "${CLAUDE_CONFIG_DIR:-$HOME/.claude}/projects" -name "$sid.jsonl" 2>/dev/null | head -1 || true)
if [ -n "$sid" ] && [ -n "$transcript" ]; then
  jq -s -r '
    reduce .[] as $e ({n: 0, t: {}};
      if $e.type == "user" and ($e.message.content | tostring | contains("slinger-sling 1")) then .n += 1
      elif $e.type == "assistant" and .n > 0 and $e.message.usage != null
      then .t[(.n | tostring)][$e.message.id] = $e.message.usage
      else . end)
    | .t | to_entries[] | .key as $k | [.value[]] as $u
    | "tokens sling \($k): turns \($u | length), input \($u | map(.input_tokens // 0) | add), cache write \($u | map(.cache_creation_input_tokens // 0) | add), cache read \($u | map(.cache_read_input_tokens // 0) | add), output \($u | map(.output_tokens // 0) | add)"
  ' "$transcript"
  echo "info  transcript: $transcript"
else
  echo "info  tokens: no transcript found for session $sid"
fi
echo "info  model: $model"

if [ "$fail" = 0 ]; then echo "pass"; else echo "✗ the check failed" >&2; exit 1; fi
