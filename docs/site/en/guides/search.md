---
nav_title: "Text Search, LQL & Batch"
title: "Text Search: lancet rg, LQL & Multi-Query Batching"
description: "Choose between lancet rg, LQL proximity/fuzzy/boolean operators, pipe stages, lancet batch, parse, and explain."
lede: "Express boolean, proximity, fuzzy, and AST-scoped queries or run multiple queries over one shared walk."
keywords: "lql search near before fuzzy terms budget batch parse explain selectivity"
source: "https://github.com/CREE1116/lancet/blob/main/docs/grammar.md"
---
Lancet offers two complementary search syntaxes backed by the exact same trigram index and evaluator: `lancet rg` (regexes and UNIX flags) and **LQL** (structured boolean logic, token proximity, fuzzy matching, AST scopes, and pipes).

## When to Use lancet rg vs LQL vs lancet batch {#rg-vs-lql}

| Need | Use | Why |
| --- | --- | --- |
| Standard regex, context lines (`-A/-B/-C`), `--vimgrep`, `-c`, `-r` | `lancet rg` | Drop-in ripgrep flags, zero query-language friction |
| Multi-term `AND`/`OR`/`NOT`, `near()`, `fuzzy()`, `in function`, `\| budget` | `lancet '<LQL>'` | Single-pass compound query with token cap |
| Multiple LQL queries over the same repository | `lancet batch` | Opens `.lancet` and walks directory entries once |

## LQL Practical Patterns {#lql-recipes}

```bash title="Terminal"
# 1. Boolean logic in a single line (implicit AND + explicit OR/NOT)
lancet '("OOM" OR "out of memory") NOT "CPU" in line' src/

# 2. Word proximity within a Tree-sitter function body + parent expansion
lancet 'near("retry", "backoff", 25) in function | expand parent | limit 8' src/

# 3. Ordered proximity (B appears within 15 tokens AFTER A)
lancet 'before("lock", "write_atomic", 15) in function' src/

# 4. Typo-tolerant search (Levenshtein distance k <= 3, trigram q-gram accelerated)
lancet 'fuzzy("idempotnt", 2) | limit 10' src/

# 5. Inspect co-occurring keywords for iterative query refinement
lancet '"trigram" in ext:rs | terms top:15' src/

# 6. Hard cap output to estimated token budget
lancet '"SearchOptions" in ext:rs | expand context:3 | budget tokens:1500' src/
```

## Sharing One Walk Across Many Queries (lancet batch) {#batch-queries}

Agents often run 3–5 exploratory searches back-to-back. `lancet batch` opens the index and caches the directory walk in memory across every query in the batch:

```bash title="Terminal"
# Using repeatable -q / --name flags
lancet batch src \
  -q '"Conflict" in function' --name conflicts \
  -q 'near("mtime", "ctime", 20)' --name freshness \
  -q 'fuzzy("compaction", 1) | files' --name compact

# Or passing JSON object / array directly
lancet batch src --queries-json '{"errors": "Status::Unsupported", "locks": "lock_shared OR lock()"}'
```

## Inspecting Query Plans (lancet parse & lancet explain) {#debug-query}

Before running a query on an 80k-file repo, use `parse` to inspect canonical normalization and `explain` to verify which trigrams were extracted and how selective the index is:

```bash title="Terminal"
lancet parse '("alpha" OR "beta") NOT NOT "gamma" in line | limit 5'
lancet explain 'near("gradient", "clip", 30)' ./repo
```

`explain` prints `trigram_req`, `files_considered`, `index_candidates`, `stale_files`, and `selectivity_pct` so you immediately see whether a regex or short token causes a fallback full scan.
