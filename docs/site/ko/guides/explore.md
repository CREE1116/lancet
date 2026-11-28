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

`lancet fd`는 fd의 명령줄을 그대로 받습니다. 패턴은 이름에 맞춰 보는 정규식이고(smart case), `-g`를 주면 글롭이며, 경로는 패턴 뒤에 옵니다. 여기에 `--tree`를 더했고, `--trust-index`면 `.lancet` 인덱스에서 파일 목록을 바로 읽습니다. 전체 플래그는 [fd 호환 레퍼런스](../reference/fd-parity.md)에 있습니다.

```bash title="터미널"
# 파일 소음 없이 폴더 계층만 출력 또는 여러 루트 동시 탐색
lancet fd --tree --dirs --max-depth 2 .
lancet fd --tree -d 2 . src docs
lancet fd . crates/core crates/cli

# 기본은 정규식, -g는 글롭, -e는 확장자
lancet fd '_test\.rs$' crates/
lancet fd -g '*_test.rs' crates/
lancet fd -e proto

# 수정 시각 + 크기 조건 (기간은 fd처럼 48h, 2weeks / 크기는 k = 1000, ki = 1024)
lancet fd -t f --changed-within 48h -S +4k -S -2m

# size, mtime_ns 포함 JSON (반환 항목당 stat 1회 비용)
lancet fd -e rs --meta --max-results 25 --json . src
```

> [!NOTE] 종료 코드는 fd를 따릅니다
> `lancet fd`는 찾은 것이 없어도 `0`(fd와 같음), 없는 경로 같은 자체 오류는 `1`, 잘못된 인자는 `2`로 끝납니다. 있는지 없는지만 알고 싶으면 `-q`를 쓰세요.

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
