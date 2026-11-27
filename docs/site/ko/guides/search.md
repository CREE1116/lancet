---
nav_title: "텍스트 검색 · LQL · 배치"
title: "텍스트 검색: lancet rg, LQL & 다중 쿼리 배치"
description: "lancet rg vs LQL 선택 기준, near/before/fuzzy/파이프 실전 패턴, lancet batch, parse, explain."
lede: "논리·근접·퍼지·AST 스코프 질의와 한 번의 워크를 공유하는 다중 쿼리 배치 실행."
keywords: "lql search near before fuzzy terms budget batch parse explain 선택도"
source: "https://github.com/CREE1116/lancet/blob/main/docs/grammar.md"
---
Lancet은 같은 트라이그램 인덱스와 평가기 위에서 두 가지 검색 문법을 제공합니다: 익숙한 정규식·UNIX 플래그를 쓰는 `lancet rg`와 논리·근접·퍼지·AST 스코프·파이프를 조합하는 **LQL**입니다.

## lancet rg vs LQL vs lancet batch 선택 기준 {#rg-vs-lql}

| 목적 | 선택 | 이유 |
| --- | --- | --- |
| 일반 정규식, 문맥 줄(`-A/-B/-C`), `--vimgrep`, `-c`, `-r` | `lancet rg` | ripgrep 플래그 그대로, 학습 비용 0 |
| 다중 항 `AND`/`OR`/`NOT`, `near()`, `fuzzy()`, `in function`, `\| budget` | `lancet '<LQL>'` | 복합 질의를 1패스로 처리 + 토큰 예산 제어 |
| 같은 저장소에 LQL 쿼리 여러 개 연속 실행 | `lancet batch` | 인덱스 열기 및 디렉터리 순회를 1회로 공유 |

## 실전 LQL 쿼리 레시피 {#lql-recipes}

```bash title="터미널"
# 1. 한 줄 안에서의 불리언 논리 (공백은 암묵적 AND, OR/NOT 명시)
lancet '("OOM" OR "out of memory") NOT "CPU" in line' src/

# 2. Tree-sitter 함수 내부에서의 토큰 근접성 + 부모 함수 전체 펼치기
lancet 'near("retry", "backoff", 25) in function | expand parent | limit 8' src/

# 3. 순서 있는 근접 (A 뒤에 15토큰 이내 B 등장)
lancet 'before("lock", "write_atomic", 15) in function' src/

# 4. 오타 허용 검색 (레벤슈타인 거리 k <= 3, q-gram 인덱스 가속)
lancet 'fuzzy("idempotnt", 2) | limit 10' src/

# 5. 검색 결과에 함께 많이 나온 키워드 상위 15개 추출 (쿼리 구체화용)
lancet '"trigram" in ext:rs | terms top:15' src/

# 6. 추정 토큰 예산으로 출력 크기 하드 캡
lancet '"SearchOptions" in ext:rs | expand context:3 | budget tokens:1500' src/
```

## 한 번의 워크로 여러 쿼리 실행 (lancet batch) {#batch-queries}

에이전트가 관련 검색을 3~5회 연달아 돌릴 때는 `lancet batch`로 인덱스와 디렉터리 목록을 메모리에 공유하세요:

```bash title="터미널"
# 반복 가능한 -q / --name 플래그
lancet batch src \
  -q '"Conflict" in function' --name conflicts \
  -q 'near("mtime", "ctime", 20)' --name freshness \
  -q 'fuzzy("compaction", 1) | files' --name compact

# JSON 객체 또는 배열 직접 전달
lancet batch src --queries-json '{"errors": "Status::Unsupported", "locks": "lock_shared OR lock()"}'
```

## 쿼리 계획 점검 (lancet parse & lancet explain) {#debug-query}

대형 저장소에 쿼리를 날리기 전에 `parse`로 정규화된 정규식을 보고, `explain`으로 추출된 트라이그램과 인덱스 후보 비율을 확인하세요:

```bash title="터미널"
lancet parse '("alpha" OR "beta") NOT NOT "gamma" in line | limit 5'
lancet explain 'near("gradient", "clip", 30)' ./repo
```

`explain`은 `trigram_req`, `files_considered`, `index_candidates`, `stale_files`, `selectivity_pct`를 출력해 풀스캔 폴백 여부를 즉시 보여 줍니다.
