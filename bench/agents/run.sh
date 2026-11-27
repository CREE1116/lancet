#!/bin/sh
# Run every task in tasks.txt in one workspace, resetting it before each task.
#
#   bench/agents/run.sh WORKDIR lancet|builtin MODEL TAG
#   e.g. bench/agents/run.sh /tmp/lancet-ab lancet opencode/big-pickle bp
#
# Writes WORKDIR/out/<task>-<setup>-<tag>.{jsonl,err,diff}. The lancet setup puts
# this checkout's target/release first on PATH; the builtin setup shadows `lancet`.
set -u
WORK=${1:?usage: run.sh WORKDIR SETUP MODEL TAG}
SETUP=${2:?}
MODEL=${3:?}
TAG=${4:?}
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)
ws="$WORK/$SETUP-ws"
out="$WORK/out"
mkdir -p "$out"
if [ "$SETUP" = lancet ]; then P="$REPO/target/release:$PATH"; else P="$WORK/nolancet:$PATH"; fi

while IFS='|' read -r id task; do
  [ -n "$id" ] || continue
  git -C "$ws" checkout -q . && git -C "$ws" clean -qfdx -e target
  f="$out/$id-$SETUP-$TAG"
  (cd "$ws" && PATH="$P" timeout 900 opencode run --standalone -m "$MODEL" --auto --format json \
      --title "ab-$SETUP-$TAG-$id" "$task" </dev/null >"$f.jsonl" 2>"$f.err")
  echo "exit=$?" >>"$f.err"
  git -C "$ws" add -A -- . ':!target' ':!Cargo.lock' >/dev/null
  git -C "$ws" diff --cached >"$f.diff"
  git -C "$ws" reset -q
done <"$HERE/tasks.txt"
