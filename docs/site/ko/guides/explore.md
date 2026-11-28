---
nav_title: "파일 탐색 · 아웃라인 · 랭킹"
title: "파일 탐색, 아웃라인 & 심볼 사용 빈도 랭킹"
description: "lancet fd --dirs --tree, outline, refs, refs --rank로 낯선 저장소의 핵심을 빠르게 파악하는 법."
lede: "파일 전체를 열어 보기 전에 디렉터리, 선언 골격, 참조 빈도순 핵심 심볼 지도를 먼저 만듭니다."
keywords: "fd tree dirs changed-within meta outline refs rank kinds 선언 랭킹"
source: "https://github.com/CREE1116/lancet/blob/main/docs/agent-instructions.md"
---
파일 전체를 열어 보는 대신 `fd`, `outline`, `refs`로 디렉터리, 선언 골격, 사용 빈도순 핵심 심볼을 먼저 지도화하면 컨텍스트 토큰 소비를 크게 줄일 수 있습니다.

## 빠른 트리 & 메타데이터 탐색 (lancet fd) {#fd-guide}

`lancet fd`는 디스크를 순회하거나 `--trust-index`로 mmap 된 `.lancet`에서 워크 없이 즉시 목록을 읽어 옵니다.

```bash title="터미널"
# 파일 소음 없이 폴더 계층만 출력 또는 여러 루트 동시 탐색
lancet fd --tree --dirs --max-depth 2 .
lancet fd --tree -d 2 src docs
lancet fd . crates/core crates/cli

# 글롭/정규식 매칭 (`*`, `?`, `[` 포함 시 v0.1.3+에서 자동 글롭 처리)
lancet fd '*_test.rs' crates/
lancet fd -p config -r 'auth|session' src

# 수정 시각 + 크기 조건 (기간: s, m, h, d, w / 크기: b, k, m, g)
lancet fd -t f --changed-within 48h --min-size 4k --max-size 2m .

# size, mtime_ns 포함 JSON (반환 항목당 stat 1회 비용)
lancet fd -e rs --meta --limit 25 --json src
```

> [!NOTE] fd에서 -e / --type-class / 크기 필터를 쓸 때
> `-t`를 주지 않은 채 `-e`, `--type-class`, `--min-size`, `--max-size`를 지정하면 `--dirs`나 `-t d`를 함께 주지 않는 한 자동으로 파일만 추립니다.

## 코드·문서·시트·압축 뼈대 추출 (lancet outline) {#outline-guide}

`lancet outline`은 AST 선언 시그니처(코드), 마크다운·PDF·PPTX·DOCX·HWP 제목, 엑셀 시트명과 크기, 압축 파일 내부 항목 목록을 줄 번호와 함께 뽑아 줍니다.

```bash title="터미널"
# 단일 파일 AST 시그니처와 시작/끝 줄 범위
lancet outline src/scope.rs

# 디렉터리 깊이·확장자 제한 요약
lancet outline src -d 2 -e rs --limit 20 --json

# 비코드 파일 뼈대: 마크다운 목차, 엑셀 시트, tar.gz 내부 파일 목록
lancet outline docs/spec.md
lancet outline financial_model.xlsx
lancet outline logs/crash_bundle.tar.gz
```

## 어휘 AST 참조 & 사용 빈도 랭킹 (lancet refs) {#refs-and-rank}

`lancet refs`는 24개 Tree-sitter 문법으로 후보 파일을 파싱해 `[def]` 선언과 `[ref]` 참조 위치를 분리하고, 감싸는 함수(`enclosing`)를 기록하며, **주석과 문자열 리터럴을 자동 제외**합니다.

```bash title="터미널"
# 정의와 모든 어휘 호출·참조 위치 조회
lancet refs SearchOptions src --lang rs

# 코드 외 파일(Markdown, TOML 등)까지 어휘 매칭으로 포함
lancet refs SearchOptions . --all-files
```

### refs --rank로 핵심 선언 순위 뽑기 {#rank-mode}

`--rank`를 주면 첫 인자를 선언 이름 글롭(예: `*Service`, `*Handler`)으로 해석해 어휘 참조 횟수 내림차순으로 정렬합니다. 글롭에 리터럴 구간(예: `Service`)이 있으면 트라이그램 인덱스로 후보를 먼저 좁힙니다.

```bash title="터미널"
# src/에서 가장 많이 참조되는 *Service 클래스 상위 10개
lancet refs --rank '*Service' --kind class src --limit 10

# Rust 크레이트 내 핵심 function, struct 순위
lancet refs --rank '*' --kind function,struct src --limit 15 --json
```

여러 선언이 같은 이름을 공유할 때는(예: `new`, `run`) 터미널에 `(name shared by N declarations)`가, JSON에 `definitions: N`이 표시되어 어휘 참조 수가 공유됨을 알려 줍니다.
