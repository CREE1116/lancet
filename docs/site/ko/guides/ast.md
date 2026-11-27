---
nav_title: "구조적 AST 검색 · 치환"
title: "구조적 AST 검색 & 안전한 치환 (24개 언어)"
description: "$VAR, $_, $$$ARGS, 문장 메타변수, 구조 동등성, dry-run diff, 중첩 건너뛰기(skipped_overlaps)."
lede: "트라이그램 프리필터와 dry-run diff 위에서 24개 Tree-sitter 언어의 구문 트리를 검색·변환합니다."
keywords: "ast tree-sitter 메타변수 rewrite apply limit skipped_overlaps if else cpp 헤더"
source: "https://github.com/CREE1116/lancet/blob/main/skills/lancet/references/query-and-ast.md"
---
`lancet ast`는 내장된 24개 Tree-sitter 문법으로 구문 트리를 매치합니다. 공백, 개행, 주석, 문자열 내용 차이를 무시하고 실제 코드 구조만 겨냥합니다.

## 메타변수와 구조 동등성 제약 {#metavariables}

- **`$VAR` (단일 명명 노드):** 이름 있는 AST 노드 1개를 캡처합니다. 같은 이름을 반복하면(예: `$X == $X`, `$X = $X`) 두 서브트리의 **구조적 동등성**을 강제합니다.
- **`$_` (익명 와일드카드):** 단일 노드에 매치되지만 리라이트 캡처에 저장하지 않습니다.
- **`$$$ARGS` (가변 형제 노드):** 0개 이상의 형제 노드(함수 인자, 구조체 필드, 문장 블록)에 매치합니다.
- **C 계열 문장 메타변수:** C, C++, Rust, Go, JS/TS/TSX, Java, C# 등에서 `if ($C) $S`, `{ $$$B }` 같은 문장/블록 패턴이 동작합니다.

## 언어별 AST 검색·치환 레시피 {#recipes}

```bash title="터미널"
# Rust: .unwrap()을 ? 연산자로 바꾸는 dry-run 미리보기
lancet ast '$X.unwrap()' src --lang rs --rewrite '$X?'

# Rust: 가변 인자 panic! 호출 찾기
lancet ast 'panic!($$$ARGS)' src --lang rs

# Python: 자기 자신과의 중복 비교 탐지 ($X 구조 동등성)
lancet ast '$X == $X' . --lang py

# Go: 테스트 파일을 제외한 fmt.Errorf 래핑 일괄 치환 미리보기
lancet ast 'fmt.Errorf($FMT, $ERR)' pkg/ --lang go --path '!*_test.go' --rewrite 'errors.Wrap($ERR, $FMT)'

# TypeScript / TSX: console.log 호출 찾기
lancet ast 'console.log($$$ARGS)' web/ --lang tsx
```

## 안전한 리라이트와 원자적 --apply {#rewrites}

1. **먼저 미리보기:** `--rewrite TEMPLATE`만 주면 디스크를 수정하지 않고 unified diff(`mode: "dry-run"`)를 출력합니다.
2. **`skipped_overlaps` 확인:** 한 파일 안에 중첩된 매치(예: 중첩된 `$X.unwrap().foo.unwrap()`)가 있으면 가장 앞/바깥쪽 편집을 우선 적용하고 겹치는 안쪽 편집은 건너뛰어 오프셋 손상을 막습니다. 안쪽 잔여분은 한 번 더 실행하면 됩니다.
3. **원자적 기록:** `--apply`를 추가하면 Tree-sitter 구문 검사 게이트와 `write_atomic`을 거쳐 반영합니다.

> [!WARNING] 주의할 제약과 ast-grep과의 차이
> - **`--apply`는 `--limit`과 병행 불가:** 일부 개수만 잘린 리라이트가 실수로 반영되지 않도록 `--limit`을 거절합니다. 대신 `--path` 글롭으로 대상을 좁히세요.
> - **엄격한 `if` 모양:** `if ($C) $S`는 `else`가 없는 순수 `if`만 매치합니다(`ast-grep`은 `if ... else`도 함께 매치).
> - **`--lang cpp`의 `.h` 포함:** C++ 검색 시 `.cpp`/`.cc`/`.cxx`/`.hpp`뿐 아니라 `.h` 헤더 파일도 스캔합니다.
> - **리터럴 없는 패턴:** 3바이트 이상 리터럴이 없으면 `.lancet`으로 좁힐 수 없어 해당 언어 파일을 전부 파싱합니다(`ast-grep`의 0.6~1.0배 속도; 최악의 `$$$` 백트래킹은 메모이제이션과 단계 예산으로 제한).
