---
nav_title: "로컬 스코프 이름 변경"
title: "스코프 증명 기반 로컬 이름 변경 (lancet rename)"
description: "Rust, Python, JS, TS, TSX, Go에서 2패스 스코프·섀도잉 검증으로 로컬 바인딩만 안전하게 변경."
lede: "모호하거나 파일 경계를 넘는 심볼은 추측 대신 거절하고, 섀도잉·변수 캡처가 없음을 증명한 뒤에만 기록합니다."
keywords: "rename 로컬 스코프 python 매개변수 섀도잉 캡처 comment_mentions refs 폴백"
source: "https://github.com/CREE1116/lancet/blob/main/skills/lancet/references/edit-patch-rename.md"
---
`lancet rename`은 단일 파일 내부의 **1단계(tier-1) 로컬 스코프 심볼 이름 변경**을 수행합니다. 문자열 치환이나 휴리스틱 대신 언어별 어휘 스코프를 해석하고, 변경 후 파일 전체를 재해석해 섀도잉과 변수 캡처가 0건임을 증명할 때만 씁니다.

## 대상 지정 문법 & 지원 6개 언어 {#usage}

지원 언어: **Rust**(`.rs`), **Python**(`.py`), **JavaScript**(`.js`), **TypeScript**(`.ts`), **TSX**(`.tsx`), **Go**(`.go`).

```bash title="터미널"
# Dry-run diff (해당 줄에 OLD 바인딩이 하나뿐이면 COL 생략 가능)
lancet rename src/search.rs:42 old_name new_name

# 열(1-based byte offset, `lancet rg --column`과 동일) + CAS rev 지정
lancet rename src/search.rs:42:9 old_name new_name --rev <REV> --apply
```

`FILE:LINE[:COL]`은 **선언 위치**나 스코프 내부의 **사용 위치** 어느 쪽을 가리켜도 됩니다. 스코프 내 주석의 언급은 바꾸지 않고 JSON `comment_mentions` 횟수로만 보고합니다.

## 2패스 스코프 해석 및 섀도잉 증명 {#how-proof-works}

1. **1패스 (바인딩 수집):** 언어별 호이스팅, 블록/함수 스코프, 클로저 매개변수, 루프 바인더 규칙으로 스코프 트리를 구성해 해당 로컬 선언에 묶인 모든 AST 식별자를 찾습니다.
2. **2패스 (변경 후 재해석):** 모든 위치를 `new`로 바꾼 가상 소스에서 파일 스코프를 다시 해석합니다. 어느 한 사용 위치라도 기존과 다른 선언을 가리키면(섀도잉 또는 외부 변수 캡처) `unsupported`로 거절하고 디스크를 건드리지 않습니다.

## lancet rename이 의도적으로 거절하는 대상 {#refusals}

| 거절되는 구문 | 추측 시 코드가 망가지는 이유 |
| --- | --- |
| 모듈 / 최상위 / 외부 노출 심볼 | 다른 파일에서 import해 쓸 수 있어 프로젝트 전체 컴파일러/LSP 필요 |
| 구조체 필드 & 메서드 | 리시버를 통해 파일 경계를 넘어 접근됨 |
| **Python 함수 매개변수** | 다른 파일 호출자가 키워드 인자(`fn(old=1)`)로 넘길 수 있음 |
| 매크로 내부 & JSX 태그명 | 일반 변수 바인딩과 다른 매크로 확장·컴포넌트 해석 규칙 |
| Go 구조체 리터럴 키 (`T{old: v}`) | 지역 변수 읽기가 아니라 필드 라벨임 |
| 동적 구문 (`eval`, `with`, `locals()`, `global`, `nonlocal`) | 동적 네임스페이스 조회로 정적 스코프 증명이 불가능함 |
| 섀도잉 / 변수 캡처 발생 | `new`가 스코프 내 기존 바인딩과 충돌함 |

> [!TIP] 크로스 파일 또는 거절된 심볼 변경 방법
> `lancet refs <SYMBOL> src --json`으로 정의와 참조를 모두 확인한 뒤 파일별 `lancet edit --edits` 또는 언어 LSP를 사용하세요.
