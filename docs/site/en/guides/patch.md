---
nav_title: "Atomic Multi-File Patch"
title: "Atomic Multi-File Patching (lancet patch)"
description: "Apply git diff and diff -u patches across files with exact hunk relocation, rename/mode support, and race rollback."
lede: "Never leave a half-applied patch on disk: verify every hunk across every file before writing a byte."
keywords: "patch unified diff git diff -p strip root rename mode rollback binary copy"
source: "https://github.com/CREE1116/lancet/blob/main/skills/lancet/references/edit-patch-rename.md"
---
`lancet patch` brings compare-and-swap safety, pre-verification, rollback, and syntax checks to standard `git diff` and `diff -u` unified patches across multiple files.

## Flags & Basic Usage {#usage}

```bash title="Terminal"
# Dry-run verify and preview resulting diffs (default)
lancet patch changes.diff
git diff | lancet patch

# Apply atomically from subdir with custom prefix strip (-p 1 strips a/ and b/)
git diff | lancet patch --root services/auth -p 1 --apply

# Plain diff -u without a/ or b/ prefixes
diff -u old.rs new.rs | lancet patch -p 0 --apply
```

## Exact Hunk Relocation vs Fuzzy Patching {#exact-reloc}

Traditional `patch -p1` allows fuzzy context mismatches and applies hunks partially per file. In `src/patch.rs`:

- **Exact context & removals required:** every hunk's context and `-` lines must match verbatim.
- **Unique nearest-offset relocation:** if line numbers shifted cleanly, Lancet searches for the nearest offset where all old lines match *uniquely* and records the shift in JSON `offsets` (`0` = exact header match). Ambiguous or non-matching hunks are refused — never fuzzy.
- **EOF newline markers:** honors `\ No newline at end of file` accurately on both old and new sides.

## Supported & Refused Diff Constructs {#git-headers}

| Unified Diff Feature | Status | Behavior in `lancet patch` |
| --- | --- | --- |
| Normal edits, multi-hunk files | <span class='tag-yes'>Supported</span> | Syntax-checked & written via CAS |
| `git diff -M` renames (with or without edits) | <span class='tag-yes'>Supported</span> | Handles `rename from` / `rename to` cleanly |
| Mode changes (`new mode`, `new file mode`) | <span class='tag-yes'>Supported</span> | Applies Unix permission bits (`chmod`) |
| Created / deleted empty files | <span class='tag-yes'>Supported</span> | Detects `/dev/null` and `deleted file mode` |
| `copy from` / `copy to` | <span class='tag-no'>Refused</span> | Aborts entire patch before writing any file |
| `Binary files ...` / `GIT binary patch` | <span class='tag-no'>Refused</span> | Binary patches are refused without touching disk |

## Pre-Verification & Race Rollback {#atomicity}

Before mutating any file, Lancet reads, verifies, relocates, and Tree-sitter-checks **every file in the patch**. Each modification writes via locked `write_if_rev` against the pre-checked `rev`. If another writer wins a race mid-patch, previously written files in the same patch are restored via best-effort rollback and exit code `2` (`conflict`) is returned.
