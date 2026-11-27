---
nav_title: "정밀 읽기 · PDF · HWP"
title: "정밀 읽기, PDF·오피스·한글(HWP)·압축 파일 & OCR"
description: "줄 범위, 심볼, 토큰 예산 읽기, PDF/PPTX 페이지, 엑셀 셀, HWP/HWPX, ZIP/TAR 항목과 16 MiB 상한."
lede: "코드와 문서·압축 파일까지 1:1 동일한 줄 좌표를 공유하며 16 MiB 압축 폭탄 상한으로 안전하게 읽습니다."
keywords: "read budget-tokens next_line pdf docx pptx hwp hwpx xlsx zip tar ocr 16 mib 압축폭탄"
source: "https://github.com/CREE1116/lancet/blob/main/docs/agent-instructions.md"
---
`lancet read`는 소스 코드, PDF, 오피스 문서, 한글(HWP/HWPX), 압축 파일에서 필요한 조각만 꺼내 오며, 검색 출력과 **1:1로 일치하는 줄 번호**를 반환합니다.

## 코드 읽기: 줄 범위, 심볼 & 토큰 예산 {#code-reading}

```bash title="터미널"
# 인라인 줄 범위 (file:10-50, file:42, --lines 100: 모두 지원)
lancet read src/search.rs:100-160

# 한 번의 호출로 여러 파일/범위 동시 읽기 (v0.1.4+; 각각 `# PATH rev REV` 헤더 출력)
lancet read src/search.rs:100-140 src/main.rs:1-40

# Tree-sitter 선언 전체 읽기 (위쪽 문서 주석·어트리뷰트 포함!)
lancet read src/search.rs --symbol execute_one --json

# 추정 토큰 수로 자르고 다음 이어 읽기 커서(next_line) 받기
lancet read src/mcp.rs --lines 1:800 --budget-tokens 800 --json

# 파이프/복사용으로 헤더와 줄 번호 거터 생략
lancet read src/lib.rs --lines 1:40 --raw
```

모든 일반 텍스트 응답에는 파일 바이트의 16자리 16진수 **`rev`**(XXH3-64)가 포함됩니다. 여러 타깃을 `--json`으로 읽으면 `{"status": ..., "results": [ReadResponse, ...]}` 형태로 묶여 반환됩니다(종료 코드는 전체 중 가장 나쁜 상태 기준). 이 `rev`를 [`lancet edit --rev`](edit.md)나 [`lancet rename --rev`](rename.md)에 그대로 넘기면 됩니다.

## 지원 문서 · 스프레드시트 · 압축 포맷 {#formats}

| 포맷 | 확장자 | Read 선택자 | 비고 |
| --- | --- | --- | --- |
| PDF | `.pdf` | `--page 2` · `--page 2:5` · `--lines` | Pure-Rust 인프로세스 추출; `--ocr` 선택 |
| Word / ODT / EPUB | `.docx`, `.odt`, `.epub` | `--lines START:END` | XML 문단 스트림 추출 |
| PowerPoint | `.pptx` | `--page 3` · `--page 1:4` | 슬라이드를 `[Slide N]` 섹션에 매핑 |
| 한글 (HWP) | `.hwp` (5.0 OLE), `.hwpx` | `--lines START:END` | 네이티브 OLE/XML 추출기 |
| 스프레드시트 | `.xlsx`, `.xls`, `.xlsm`, `.xlsb`, `.ods` | `--sheet S` · `--rows 1:30` · `--cols A:E` | `--sheet`는 시트 이름 또는 1부터 시작하는 번호 |
| 압축 파일 | `.zip`, `.tar`, `.gz`, `.tgz`, `.tar.gz` | `bundle.zip!inner/file.log` 또는 `--entry` | `--lines` 및 `--budget-tokens`와 조합 가능 |
| 이미지 (`--ocr` 사용 시) | `.png`, `.jpg`, `.jpeg`, `.tif`, `.tiff`, `.bmp`, `.webp` | `--ocr` | `PATH`의 외부 `tesseract` 필요 |

## 비텍스트 문서의 검색 vs 읽기 규칙 {#rg-docs}

- **`lancet '<LQL>'`:** 문서·압축 어댑터를 기본 활성화해 인덱싱·검색합니다(`type:document`, `type:spreadsheet`, `type:archive` 필터).
- **`lancet rg --docs`:** `lancet rg`는 `ripgrep`과 바이트 일치 출력을 지키기 위해 기본 OFF입니다. 문서를 포함하려면 `--docs`를 붙이세요.

```bash title="터미널"
# 스프레드시트에서 키워드 위치 확인 후 해당 셀 범위만 읽기
lancet '"EBITDA" in type:spreadsheet' ./finance
lancet read finance/q4.xlsx --sheet 1 --rows 12:35 --cols A:F

# 압축 파일 목록 확인 후 특정 로그만 스트리밍
lancet outline artifacts/bundle.zip
lancet read artifacts/bundle.zip!worker_0/panic.log --lines 1:80 --ocr
```

## 자원 상한, 패닉 격리 & 읽기 전용 보장 {#caps-safety}

- **하드 상한 (`src/adapters.rs`):** 입력 파일 최대 **64 MiB**(`MAX_INPUT_BYTES`), 추출 UTF-8 버퍼 최대 **16 MiB**(`MAX_EXTRACTED_BYTES`, UTF-8 문자 경계에서 안전 절단), 압축 내부 순회 최대 **500개 항목**(`MAX_ARCHIVE_ENTRIES`)으로 압축 폭탄을 차단합니다.
- **패닉 격리:** 손상된 PDF/문서 파서 패닉은 `catch_unwind`로 격리해 전체 워커를 죽이지 않고 읽기 불가로 처리합니다.
- **읽기 전용 계약:** 문서와 압축 항목은 JSON에 `rev`를 내지 않으며 `lancet edit`이 수정을 거부합니다.
- **스캔 PDF OCR:** `PATH`에 `pdftoppm`과 `tesseract`가 있을 때 `--ocr`을 넘기면 광학 문자를 추출합니다.
