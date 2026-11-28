---
nav_title: "설치 · 빠른 시작"
title: "Lancet 공식 문서 — 설치 · 빠른 시작 · 치트시트"
description: "macOS, Linux, Windows 설치, 60초 워크스루, 12개 명령 한 화면 치트시트."
lede: "탐색, 트라이그램 인덱스 검색, 24개 언어 AST 치환, 정밀 읽기, CAS 편집을 하나의 Rust 바이너리로 통합합니다."
keywords: "설치 homebrew curl windows cargo binstall no-default-features 치트시트 빠른시작"
source: "https://github.com/CREE1116/lancet/blob/main/README.ko.md"
---
**Lancet**은 `rg` + `fd` + `ast-grep` + `cat` + `sed` + `patch` + `pdftotext`를 이어 쓰던 작업을 의존성 없는 단일 Rust 바이너리로 통합합니다. 모든 명령이 같은 `.lancet` 트라이그램 인덱스, 파일 필터 규칙, **1부터 시작하는 줄 번호**, **XXH3-64 `rev`** 해시를 공유합니다.

> [!TIP] 한 줄로 요약한 설계 원칙
> 트라이그램 인덱스는 *열지 않을 파일만* 걸러냅니다. 후보 파일과 수정된 파일은 풀스캔과 완전히 동일한 평가기로 검증되며, 모든 쓰기는 배타적 잠금 기반 compare-and-swap으로 처리됩니다.

## 설치 및 빌드 변형 {#install}

릴리스마다 macOS(Apple Silicon·Intel), Linux(x86_64/arm64 glibc 및 정적 musl), Windows용 사전 빌드 바이너리를 제공합니다.

```bash title="macOS / Linux (Homebrew)"
brew tap CREE1116/lancet https://github.com/CREE1116/lancet
brew install CREE1116/lancet/lancet
```

```bash title="macOS / Linux (프리빌트 설치 스크립트)"
curl -fsSL https://raw.githubusercontent.com/CREE1116/lancet/main/install.sh | sh
```

```bash title="Windows (PowerShell)"
irm https://raw.githubusercontent.com/CREE1116/lancet/main/install.ps1 | iex
```

```bash title="Cargo (전체 빌드 vs 코드 전용 경량 빌드)"
# cargo-binstall로 바이너리 바로 받기
cargo binstall --git https://github.com/CREE1116/lancet lancet

# 전체 기능 소스 빌드 (~40 MB: Tree-sitter 24개 문법 + PDF/오피스/HWP/압축 어댑터)
cargo install --git https://github.com/CREE1116/lancet lancet

# 경량 빌드 (~4 MB: 문서/압축 어댑터 제외, 순수 코드·텍스트 전용)
cargo install --git https://github.com/CREE1116/lancet lancet --no-default-features
```

## 60초 전체 흐름 따라하기 {#walkthrough}

`.lancet` 인덱스는 선택 사항입니다. 인덱스가 없어도 모든 명령이 동작하며, 기본 `auto` 모드는 매 쿼리마다 디스크 메타데이터를 확인해 수정된 파일을 놓치지 않습니다.

```bash title="터미널"
# 1. 선택: 로컬 트라이그램 인덱스 한 번 빌드
lancet index .

# 2. 폴더 구조 파악 후 참조 수 순위로 핵심 심볼 찾기
lancet fd --tree --dirs -d 2 .
lancet refs --rank '*Service' --kind class src --limit 8

# 3. ripgrep 플래그로, 또는 구문 트리로 검색
lancet rg -w plan_targets src
lancet ast '$X.unwrap()' src --lang rs

# 4. 필요한 심볼이나 줄 범위만 읽고 XXH3-64 `rev` 확보
lancet read src/scope.rs --symbol plan

# 5. 그 텍스트만 정확히 고치기: 미리보기 후 --apply
lancet edit src/scope.rs --old-string 'let limit = 20;' \
  --new-string 'let limit = 50;' --apply
```

## 한 화면 명령어 치트시트 {#cheat-sheet}

| 작업 단계 | 명령어 | 보장 / 특징 |
| --- | --- | --- |
| 폴더 레이아웃 | `lancet fd --tree --dirs -d 2 .` | `.gitignore` / `.ignore` 반영 |
| 최근/확장자 필터 | `lancet fd -t f -e rs --changed-within 7d` | 단위: `s`, `m`, `h`, `d`, `w` |
| rg 그대로 쓰기 | `lancet rg -w -t rs 'pat' src` | `rg`와 바이트 동일 + `--docs` |
| 문서까지 검색 | `lancet rg --docs -n 'indemnity' contracts/` | `read`와 같은 줄 번호 |
| 뼈대 추출 | `lancet outline src/main.rs` | 함수·마크다운 제목·시트·압축 목록 |
| 토큰 예산 읽기 | `lancet read f.rs --lines 10:50 --budget-tokens 800` | `rev` + `next_line` 커서 반환 |
| 심볼 읽기 | `lancet read f.rs --symbol my_fn` | 문서 주석·어트리뷰트 포함 |
| 문서·압축파일 | `lancet read r.pdf -p 2:3` · `a.zip!app.log` | 검색과 1:1 동일 줄 번호; 읽기 전용 |
| 참조 순위 | `lancet refs --rank '*Service' --kind class src` | 어휘 AST 참조 수 내림차순 |
| AST 검색·치환 | `lancet ast '$X.unwrap()' src -l rs -r '$X?'` | 24개 언어; `--apply` 전엔 dry-run |
| 정확한 텍스트 고치기 | `lancet edit f.rs --old-string OLD --new-string NEW --apply` | OLD가 한 번만 나와야 함(`--replace-all`) |
| 여러 파일 한 번에 | `lancet edit --edits - --apply` | 전부 쓰거나 하나도 안 씀, CAS |
| 파일 통째로 | `lancet write f.rs --with-file - --rev REV --apply` | 새 파일, 또는 읽은 파일 교체 |
| Unified Diff | `git diff \| lancet patch --apply` | 전체 파일 사전 검증 + 경합 롤백 |
| 로컬 이름 변경 | `lancet rename f.rs:42 old new --rev REV --apply` | 6개 언어; 스코프 재해석 증명 |
| MCP 서버 | `lancet mcp --root .` | CLI 인자를 받는 `lancet` 도구 하나, stdio JSON-RPC, `--read-only` |
| 인덱스 상태 | `lancet index --status` | 세그먼트·파일 수·크기를 JSON으로 |

## 어디부터 볼까요? {#next-steps}

:::cards
- 시작하기 | 도구 전환 가이드 & 필터 규칙 | rg/fd/sg/sed/patch 1:1 대조표와 .gitignore, 글롭, 파일 타입 평가 규칙. | migration.md
- 탐색과 검색 | 탐색 · rg · AST · 문서 읽기 | fd/outline/refs 랭킹, ripgrep 호환 검색, 24개 언어 AST, PDF/HWP/압축파일. | guides/explore.md
- 편집과 에이전트 | 편집 · 패치 · 이름 변경 · MCP | --rev CAS 동시성 제어, patch 롤백, 스코프 증명 기반 rename, MCP 연동. | guides/edit.md
- 내부 구조 & 레퍼런스 | LANCET04 · 쓰기 안전성 · 스키마 | 바이너리 세그먼트 포맷, racy 검사, OS 잠금, CLI/JSON/MCP 명세 및 문제 해결. | reference/cli.md
:::
