#!/bin/sh
# Build the two workspaces an A/B run compares (see README.md):
#   WORKDIR/lancet-ws   built-in file tools off, the lancet skill as AGENTS.md
#   WORKDIR/builtin-ws  built-in tools on, `lancet` hidden, an equal-size guide for them
# Both hold this repository's source at REV (default a043e7f, the revision the
# tasks and grader are written against) plus docs/design-notes.pdf.
#
#   bench/agents/setup.sh /tmp/lancet-ab [REV]
set -eu
WORK=${1:?usage: setup.sh WORKDIR [REV]}
REV=${2:-a043e7f}
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)
mkdir -p "$WORK/nolancet" "$WORK/out"
printf '#!/bin/sh\necho "lancet: command not found" >&2\nexit 127\n' > "$WORK/nolancet/lancet"
chmod +x "$WORK/nolancet/lancet"

for setup in lancet builtin; do
  ws="$WORK/$setup-ws"
  rm -rf "$ws"
  mkdir -p "$ws"
  git -C "$REPO" archive "$REV" src docs Cargo.toml Cargo.lock README.md | tar -x -C "$ws"
  rm -rf "$ws/docs/site" "$ws/docs/assets"
  cp "$HERE/fixtures/design-notes.pdf" "$ws/docs/"
  if [ "$setup" = lancet ]; then
    # The skill body (without its front matter), as the agent instructions.
    awk 'f>=2{print} /^---$/{f++}' "$REPO/skills/lancet/SKILL.md" > "$ws/AGENTS.md"
    printf '\n## In this workspace\n\nThe built-in read/edit/write/grep/glob tools are turned off. Use `lancet` through the shell for every file operation.\n' >> "$ws/AGENTS.md"
    tools='"read":false,"edit":false,"write":false,"grep":false,"glob":false,"list":false,"patch":false,"multiedit":false,"skill":false'
  else
    cp "$HERE/guide-builtin.md" "$ws/AGENTS.md"
    tools='"skill":false'
  fi
  printf '{"$schema":"https://opencode.ai/config.json","tools":{%s},"permission":{"bash":"allow"}}\n' "$tools" > "$ws/opencode.json"
  git -C "$ws" init -q
  git -C "$ws" add -A
  git -C "$ws" -c user.name=ab -c user.email=ab@example.invalid commit -qm "workspace at $REV"
done
echo "workspaces ready in $WORK (guide sizes: $(wc -c < "$WORK/lancet-ws/AGENTS.md") / $(wc -c < "$WORK/builtin-ws/AGENTS.md") bytes)"
