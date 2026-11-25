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

## 왜 `cat` / `sed` / 기본 `Read` 대신 `lancet read`인가?

`lancet read`는 단순 텍스트 출력기가 아니라 **검색 엔진과 좌표계를 공유하는 컨텍스트 메스(Context Scalpel)** 입니다.

1. **검색 좌표 ↔ 열람 좌표의 1:1 동형성 (Coordinate Isomorphism)**: 기존 워크플로에서는 `.pdf`, `.docx`, `.xlsx`, `.hwp`/`.hwpx`, `.zip` 내부에서 "42번째 줄 히트"를 찾아도 기본 `Read`/`cat`으로는 바이너리를 읽지 못하거나, 다른 외부 파서를 거치며 줄 번호가 어긋납니다. `lancet read`는 검색기와 **완전히 동일한 인프로세스 추출 파이프라인**을 공유하므로 검색이 가리킨 42번째 줄이 어떤 포맷에서든 정확히 같은 42번째 줄입니다.
2. **줄 번호 추측 없는 의미 단위 절개 (`--symbol`, `--page`, `--sheet`)**: 함수 끝 줄을 몰라 100줄씩 여러 번 읽는 왕복 낭비 없이, `lancet read src/search.rs --symbol execute_one` 한 번으로 해당 심볼의 시작~끝(상단 `///` 문서 주석·어트리뷰트 포함)만 정확히 도려냅니다.
3. **토큰 예산 하드 캡 및 이어읽기 커서 (`--budget-tokens`)**: 미니파이된 번들이나 거대 로그가 컨텍스트 창을 터뜨리지 않도록 토큰 예산 내에서 안전하게 끊고 다음 재개 위치(`next_line`)를 반환합니다.
4. **무임시파일·무설치 컨테이너 투과**: `/tmp` 압축 해제나 `pandoc`/`libreoffice` 없이 `16 MiB` 폭탄 방어 상한 안에서 PDF 페이지, 엑셀 행·열(`--rows 1:30 --cols A:D`), `logs.zip!node_0.log`를 메모리에서 바로 읽습니다.

---

## 핵심 알고리즘 (5대 기술 기둥)

1. **보수적 트라이그램 하한 유도 & 무데몬 무결성 (`Scan == Auto`)**: 쿼리 식과 정규식 HIR에서 매칭 파일이 반드시 포함해야 하는 트라이그램 수식(`Req`)만 보수적으로 유도하고 최소 빈도(`peek_count`) 포스팅부터 교집합합니다. 디스크 `(mtime_ns, size)`가 바뀐 파일만 실시간 스캔에 합류시키고 모든 후보를 동일한 평가기로 검증하므로, **백그라운드 감시 데몬(`watch`) 없이도 `rg` 대비 5~60배 빠르면서 풀스캔과 100% 동일한 결과**를 보장합니다.
2. **q-gram 임계 보조정리 기반 인덱스 퍼지 검색**: 편집 거리 `k` 이내의 단어는 최대 `3k`개의 ASCII 트라이그램(3바이트 한글 음절은 `k`개)만 깨진다는 보조정리(`min = |G| - 3k`)로 `Req::AtLeast` `k`-of-`n` 포스팅 누산을 수행해 오타 검색도 인덱스로 가속합니다.
3. **3단 하이브리드 AST 매칭 (`Trigram Index` → `SIMD memmem` → `u16 kind_id` 패턴 트리)**: 패턴 AST에서 불변 식별자 리터럴을 추출해 ① 인덱스로 파일 오픈 생략, ② 스캔 모드에서도 SIMD `memmem`으로 무관한 파일의 Tree-sitter 파싱 생략, ③ 사전 컴파일된 `u16 kind_id` 정수 트리와 스레드별 파서 캐싱으로 순회하여 **`ast-grep` 대비 무인덱스 7배, 인덱스 최대 123배** 가속을 달성합니다.
4. **유니코드 동치 궤도 폴딩(Case-Orbit) + SIMD 2-바이트 스캔**: 유니코드 대소문자 동치류(`'k'`, `'K'`, `'K' U+212A`)를 대표 문자로 정규화하되, ASCII 리터럴은 정규식 오토마타 대신 `memchr2` 벡터 명령으로 고속 스캔합니다.
5. **제로힙 `LANCET03` 세그먼트 & 지연 줄 번호 계산 (`LineMode::HitsOnly`)**: 40바이트 헤더만 확인하는 `O(1)` 제로카피 `mmap` 세그먼트(원본의 `~8%`)를 사용하며, 전체 줄바꿈 오프셋 배열을 미리 할당하지 않고 실제 매치 구간에서만 SIMD로 줄 번호를 역산합니다.

---

## 라이선스

MIT ([LICENSE-MIT](./LICENSE-MIT)) 또는 Apache-2.0 ([LICENSE-APACHE](./LICENSE-APACHE)) 듀얼 라이선스를 따릅니다.
