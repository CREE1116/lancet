#!/bin/sh
# Codex CLI variant of run.sh: every task in tasks.txt, resetting the workspace
# before each one.
#
#   bench/agents/run_codex.sh WORKDIR lancet|builtin MODEL TAG
#   e.g. bench/agents/run_codex.sh /tmp/lancet-ab lancet gpt-5.6-sol sol1
#
# Run bench/agents/setup.sh WORKDIR first, then bench/agents/setup_codex.sh WORKDIR.
# Codex runs with an empty HOME and its own CODEX_HOME (WORKDIR/codex-home, only
# the login linked in), so neither side sees the user's skills, MCP servers or
# global instructions; WORKDIR/codex-user/.zprofile puts this side's tool
# directory first on PATH (macOS's /etc/zprofile reorders it in login shells).
# LANCET_BIN_DIR overrides which lancet the lancet side gets.
# Writes WORKDIR/out/<task>-<setup>-<tag>.{jsonl,err,diff}; the .err ends with
# the wall time (secs=N) and the exit code.
set -u
WORK=${1:?usage: run_codex.sh WORKDIR SETUP MODEL TAG}
SETUP=${2:?}
MODEL=${3:?}
TAG=${4:?}
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)
ws="$WORK/$SETUP-ws"
out="$WORK/out"
mkdir -p "$out"
if [ "$SETUP" = lancet ]; then FRONT="${LANCET_BIN_DIR:-$REPO/target/release}"; else FRONT="$WORK/nolancet"; fi

while IFS='|' read -r id task; do
  [ -n "$id" ] || continue
  git -C "$ws" checkout -q . && git -C "$ws" clean -qfdx -e target
  f="$out/$id-$SETUP-$TAG"
  t0=$(date +%s)
  AB_FRONT="$FRONT" HOME="$WORK/codex-user" CODEX_HOME="$WORK/codex-home" \
    CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
    PATH="$FRONT:$PATH" timeout 900 codex exec --json --skip-git-repo-check --ephemeral \
    -s workspace-write -m "$MODEL" -C "$ws" "$task" </dev/null >"$f.jsonl" 2>"$f.err"
  code=$?
  echo "secs=$(( $(date +%s) - t0 ))" >>"$f.err"
  echo "exit=$code" >>"$f.err"
  git -C "$ws" add -A -- . ':!target' ':!Cargo.lock' >/dev/null
  git -C "$ws" diff --cached >"$f.diff"
  git -C "$ws" reset -q
done <"$HERE/tasks.txt"
