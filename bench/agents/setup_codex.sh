#!/bin/sh
# Prepare WORKDIR (made by setup.sh) for run_codex.sh:
#   - each workspace's AGENTS.md for Codex: the lancet skill, or the
#     shell + rg + apply_patch guide of about the same size;
#   - WORKDIR/codex-home: a CODEX_HOME with only the login linked in;
#   - WORKDIR/codex-user: an empty HOME whose .zprofile puts AB_FRONT first on PATH.
#
#   bench/agents/setup_codex.sh WORKDIR
set -eu
WORK=${1:?usage: setup_codex.sh WORKDIR}
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)

awk 'f>=2{print} /^---$/{f++}' "$REPO/skills/lancet/SKILL.md" > "$WORK/lancet-ws/AGENTS.md"
printf '\n## In this workspace\n\nUse `lancet` through the shell for every file operation: reading, searching and changing files (not `apply_patch`, `sed` or `cat`).\n' >> "$WORK/lancet-ws/AGENTS.md"
cp "$HERE/guide-codex-builtin.md" "$WORK/builtin-ws/AGENTS.md"
for s in lancet builtin; do
  rm -f "$WORK/$s-ws/opencode.json"
  git -C "$WORK/$s-ws" add -A
  git -C "$WORK/$s-ws" -c user.name=ab -c user.email=ab@example.invalid commit -qm "codex instructions" || true
done

mkdir -p "$WORK/codex-home" "$WORK/codex-user"
ln -sf "${CODEX_HOME:-$HOME/.codex}/auth.json" "$WORK/codex-home/auth.json"
: > "$WORK/codex-home/config.toml"
printf '[ -n "$AB_FRONT" ] && export PATH="$AB_FRONT:$PATH"\n' > "$WORK/codex-user/.zprofile"
echo "codex workspaces ready (guide sizes: $(wc -c < "$WORK/lancet-ws/AGENTS.md") / $(wc -c < "$WORK/builtin-ws/AGENTS.md") bytes)"
