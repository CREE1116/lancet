# Lancet (란셋)

[![CI](https://github.com/CREE1116/lancet/actions/workflows/ci.yml/badge.svg)](https://github.com/CREE1116/lancet/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#라이선스)

**AI 에이전트와 개발자를 위한 초정밀 코드·AST·복합 문서 탐색, 열람, 리팩터링 단일 CLI.**

[English README](./README.md) · [쿼리 문법 명세 (LQL)](./docs/grammar.md) · [에이전트 지침 (`AGENTS.md`)](./docs/agent-instructions.md)

---

## 왜 Lancet인가?

기존 워크플로에서는 텍스트 검색(`rg`), 파일 탐색(`fd`), 구문 구조 리팩터링(`ast-grep`), 문서 변환(`pdftotext`, `pandoc`, `unzip`)이 모두 따로 놀며 인덱스·줄 번호·토큰 예산을 공유하지 못합니다. 특히 한국어 업무 환경의 `.hwp`(5.0 OLE)·`.hwpx`나 `.xlsx`, `.pdf`, `.zip` 내부 로그는 검색 후에도 에이전트가 곧바로 열람하기 어렵습니다.

**Lancet**은 **"탐색(fd/outline) → 검색(search/refs) → 정밀 열람(read) → 안전 리팩터링(ast)"** 전 과정을 순수 Rust 단일 바이너리와 선택적 제로카피 트라이그램 인덱스(`.lancet`) 하나로 통합합니다.

- **`lancet <QUERY>`** — 불리언(`AND`, `OR`, `NOT`), 단어 근접도(`near`, `before`), 레벤슈타인 오타 허용 `fuzzy("단어", k)`, 정규식, Tree-sitter 함수/심볼 범위(`in function`, `symbol(...)`), 토큰 예산(`| budget tokens:1500`) 지원.
- **`lancet fd`** — `.gitignore` 자동 인식 고속 파일·폴더 탐색, `--tree` 디렉터리 트리, `--trust-index` mmap 즉시 조회.
- **`lancet ast`** — Rust, Python, JS, TS, TSX, Go 대상 구조적 AST 검색 및 안전한 `--rewrite` / `--apply` 리팩터링 (트라이그램 인덱스 + SIMD 프리필터 자동 가속).
- **`lancet read`** — 검색 결과와 **1:1로 일치하는 줄 번호** 기반 범위 읽기, 심볼 선언 단위 읽기(`--symbol`), PDF/PPTX 페이지(`--page`), 엑셀 시트·행·열(`--sheet`, `--rows`, `--cols`), 압축파일 내부 항목(`archive.zip!file.log`) 직접 읽기.
- **`lancet outline`** — 코드 심볼 골격, Markdown 헤딩 목차, 엑셀 시트 스키마, 압축 아카이브 엔트리 요약 추출.
- **`lancet refs`** — 주석·문자열 리터럴을 자동 배제한 고속 AST 심볼 정의(`[def]`) 및 참조(`[ref]`) 위치 추적.
- **무결성 보장 (`Scan == Auto`)** — 인덱스는 후보 파일만 좁히고 동일한 평가기로 검증하며, 변경된 파일(`mtime + size`)은 자동으로 재스캔하므로 결과가 절대 오래되지 않습니다.

---

## 기성 도구 대비 벤치마크 실측 (Apple M4 Pro, 12코어, 24 GB RAM)

[`bench/run_bench.py`](./bench/run_bench.py), [`bench/bench_competitors.py`](./bench/bench_competitors.py), [`bench/official_suite.sh`](./bench/official_suite.sh)로 재현 가능합니다.
코드 코퍼스: `28,352`개 파일 (`452.6 MiB`). 초기 인덱스 빌드 **`1.17초`** (`36.3 MiB`, 원본 대비 **`8.0%`**), 변경 없을 때 증분 동기화 **`64.6 ms`**.

### 1. 코드·불리언·근접·퍼지 검색 (`lancet` vs `ripgrep 15.2.0`)

| 쿼리 (`28,352` 파일) | `rg 15.2` | `lancet` (scan) | `lancet` (auto 인덱스) | `lancet` (`--trust-index`) |
|---|---:|---:|---:|---:|
| 희귀 리터럴 (`ThreadPoolBuilder`, 32건) | 307.7 ms | 310.6 ms | **60.5 ms (5.1×)** | **5.1 ms (60.3×)** |
| 일반 리터럴 (`unwrap_or_else`, 582건) | 311.3 ms | 314.6 ms | **65.4 ms (4.8×)** | **13.0 ms (23.9×)** |
| 불리언 (`"unsafe" AND "transmute" NOT "miri"`) | 469.8 ms | **304.8 ms** | **62.6 ms (7.5×)** | **10.1 ms (46.5×)** |
| 정규식 (`pub unsafe fn \w+\(`) | 307.4 ms | 317.9 ms | **80.5 ms (3.8×)** | **26.9 ms (11.4×)** |
| 근접 (`near("unsafe", "transmute", 15)`) | *미지원* | 307.1 ms | **65.9 ms** | **13.6 ms** |
| 오타 허용 (`fuzzy("transmte", 1)`) | *미지원* | 334.7 ms | **156.7 ms** | **107.4 ms** |
| 6개 쿼리 공유 배치 (`lancet batch`) | ~1,850 ms | 1,570.6 ms | **93.1 ms** | **30.7 ms (쿼리당 5.1 ms)** |

### 2. 파일·폴더 탐색 (`lancet fd` vs `fd 10.5.2` vs `rg --files` vs `find`)

| 작업 (`28,352` 파일) | `find` | `rg --files` | `fd 10.5` | `lancet fd` (scan) | `lancet fd --trust-index` |
|---|---:|---:|---:|---:|---:|
| 전체 파일 나열 (`28,352`개) | 176.0 ms | 47.5 ms | 49.8 ms | 60.6 ms | **14.4 ms (fd 대비 3.5×)** |
| 확장자 + 이름 (`-e toml -p Cargo`) | — | — | 47.1 ms | 52.1 ms | **6.6 ms (fd 대비 7.1×)** |
| 용량 필터 (`>= 100 KiB`, 637개) | 169.1 ms | — | 65.4 ms | **53.3 ms** | **5.6 ms (fd 대비 11.7×)** |

### 3. 구조적 AST 검색 (`lancet ast` vs `ast-grep 0.51.3`)

| Rust AST 패턴 (`28,352` 파일) | `ast-grep 0.51` | `lancet ast` (scan) | `lancet ast` (auto 인덱스) | `lancet ast` (`--trust-index`) |
|---|---:|---:|---:|---:|
| `std::mem::transmute($X)` (19건) | 1,764.7 ms | **251.1 ms (7.0×)** | **176.6 ms (10.0×)** | **95.3 ms (18.5×)** |
| `$X.unwrap_or_else($F)` (963건) | 1,649.2 ms | **268.6 ms (6.1×)** | **245.7 ms (6.7×)** | **153.0 ms (10.8×)** |
| `unreachable!($$$ARGS)` (1,893건) | 1,782.2 ms | **484.8 ms (3.7×)** | **526.7 ms (3.4×)** | **343.3 ms (5.2×)** |
| `ThreadPoolBuilder::new()` (75건) | 1,716.7 ms | **241.5 ms (7.1×)** | **65.2 ms (26.3×)** | **13.9 ms (123.5×)** |

### 4. 복합 문서·압축파일 검색 (`lancet` vs `ripgrep-all 0.10.10`, 120개 문서)

| 문서 형식 및 검색어 | `rga` (cold) | `rga` (zstd 캐시) | `lancet` (scan) | `lancet` (auto) | `lancet` (`--trust-index`) |
|---|---:|---:|---:|---:|---:|
| PDF (`QUARTERLY_REVENUE_ANOMALY`) | 263.5 ms | 67.3 ms | **10.6 ms** | **6.9 ms** | **3.4 ms** |
| DOCX (`SLA_INDEMNITY_CLAUSE`) | 262.0 ms | 67.5 ms | **10.7 ms** | **7.6 ms** | **4.1 ms** |
| XLSX (`H100_CLUSTER_CAPEX`) | *0건 (미지원)* | *0건* | **11.3 ms** | **7.5 ms** | **3.6 ms** |
| ZIP 내부 로그 (`RAFT_QUORUM_STALL`) | 274.8 ms | 67.3 ms | **12.2 ms** | **7.1 ms** | **3.5 ms** |
| 한글 HWPX (`국가클라우드보안인증검토`) | *0건 (미지원)* | *0건* | **11.2 ms** | **8.1 ms** | **3.7 ms** |

---

## 설치 및 빠른 사용법

```bash
# 빌드 및 설치 (Rust 1.88 이상)
cargo install --git https://github.com/CREE1116/lancet

# 1. 작업 공간 구조 및 파일 골격 파악
lancet fd --tree --max-depth 2 .
lancet outline src/search.rs

# 2. 인덱스 생성 및 코드·문서 통합 검색
lancet index .
lancet 'near("gradient", "clip", 30) in function | expand parent' src
lancet '("위약금" AND "통지") in type:document'

# 3. 함수 단위 / PDF 페이지 / 엑셀 시트 / ZIP 내부 파일 직접 읽기
lancet read src/search.rs --symbol execute_one
lancet read report.pdf --page 2:3
lancet read budget.xlsx --sheet 1 --rows 1:20 --cols A:D
lancet read logs.zip!node_0.log --lines 1:40

# 4. 심볼 정의·참조 추적 및 안전한 AST 일괄 리팩터링
lancet refs SearchOptions src
lancet ast '$X.unwrap()' src --lang rs --rewrite '$X?'          # Dry-run diff 미리보기
lancet ast '$X.unwrap()' src --lang rs --rewrite '$X?' --apply  # 원자적 반영
```

---

## 라이선스

MIT ([LICENSE-MIT](./LICENSE-MIT)) 또는 Apache-2.0 ([LICENSE-APACHE](./LICENSE-APACHE)) 듀얼 라이선스를 따릅니다.
