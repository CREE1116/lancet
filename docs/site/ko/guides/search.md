---
nav_title: "텍스트 검색 (lancet rg)"
title: "텍스트 검색: 항상 최신인 인덱스 위의 lancet rg"
description: "lancet 트라이그램 인덱스 위에서 도는 ripgrep의 플래그·출력·종료 코드: 인덱스 모드, 여러 경로, --in/--in0, --docs로 문서 검색."
lede: "이미 아는 ripgrep 명령을 그대로 치세요. 인덱스는 출력을 한 바이트도 바꾸지 않고 빠르게만 합니다."
keywords: "rg ripgrep 정규식 인덱스 trust-index no-index docs in in0 max-bytes 호환"
source: "https://github.com/CREE1116/lancet/blob/main/docs/rg-parity.md"
---
`lancet rg`는 ripgrep입니다. 플래그, Rust `regex` 엔진, 탐색 규칙(`.gitignore`, `.ignore`, `.rgignore`, `--ignore-file`), 출력, 종료 코드(`0` 일치, `1` 일치 없음, `2` 오류)가 모두 같습니다. 다른 점은 밑에 있습니다. `.lancet` 인덱스가 있으면 일치할 수 없는 파일은 아예 열지 않습니다.

## 자주 쓰는 검색 {#recipes}

```bash title="터미널"
lancet rg -w spin_lock src                 # 단어 단위
lancet rg -n -C 2 'fn \w+_locked\(' src    # 정규식 + 앞뒤 문맥
lancet rg -i -l 'connection reset' .       # 파일 목록만, 대소문자 무시
lancet rg -t rs -g '!*_test.rs' unwrap .   # 타입과 글롭
lancet rg --json 'TODO' src                # ripgrep의 JSON 이벤트
lancet rg -c ThreadPoolBuilder a/ b/ c.rs  # 경로 여러 개를 한 번에
```

지원하는 플래그와 정확한 동작은 [ripgrep 호환 레퍼런스](../reference/rg-parity.md)에 모두 있습니다. CI에서 150개 케이스의 출력을 ripgrep 16.0과 바이트 단위로 비교합니다.

## 인덱스를 쓰는 방식 {#index-modes}

| 모드 | 플래그 | 동작 |
| --- | --- | --- |
| auto | (기본) | 정규식을 트라이그램 논리식으로 바꿔, 들어 있을 수 있는 파일만 읽습니다. 인덱스를 만든 뒤 바뀐 파일(`mtime`, 크기, `ctime`, inode)은 직접 읽으므로 결과가 항상 최신입니다. |
| scan | `--no-index` | 인덱스 없이 병렬 스캔. ripgrep과 몇 퍼센트 차이입니다. |
| trust | `--trust-index` | 신선도 확인을 건너뜁니다. 가장 빠르지만, 조용한 기본값이 아니라 명시적으로 고르는 선택입니다. |

인덱스는 선택 사항입니다(`lancet index .`). 없으면 `lancet rg`는 ripgrep과 똑같이 동작합니다. 인덱스는 일치할 수 없는 파일을 *빼기만* 합니다. 패턴에서 조건을 뽑을 수 없으면(예: `\w+`) 추측하지 않고 모든 파일을 읽습니다.

## 파일에서 경로 받기: --in, --in0 {#in-files}

```bash title="터미널"
git diff --name-only | lancet rg --in - 'unsafe'
fd -0 -e rs | lancet rg --in0 - 'unsafe'
```

목록의 경로는 준 순서를 지키고, 위치 인자 경로와는 ripgrep 16과 같은 순서로 섞입니다.

## 문서와 압축 파일: --docs {#docs}

기본적으로 `lancet rg`는 ripgrep이 찾는 곳만 찾아서 출력이 같습니다. `--docs`를 붙이면 PDF, Office, HWP/HWPX, EPUB, 압축 파일 안까지 찾습니다. 줄 번호는 `lancet read`의 줄 번호와 같아서, 찾은 곳을 그대로 읽을 수 있습니다:

```bash title="터미널"
lancet rg --docs -n 'indemnity' contracts/
lancet read contracts/msa.docx:120-140
```

## 지원하지 않는 것 {#unsupported}

`-P` / PCRE2(전후방 탐색, 역참조)는 거부합니다. Rust 엔진이라야 매칭이 선형 시간이고, 인덱스가 모든 패턴에서 건전한 필터를 뽑을 수 있습니다. 명령을 주는 `--pre`도 거부합니다. lancet은 파일마다 프로세스를 띄우지 않고, 문서와 압축 파일은 `--docs`로 프로세스 안에서 읽습니다. 전체 목록은 [호환 레퍼런스](../reference/rg-parity.md)를 보세요.
