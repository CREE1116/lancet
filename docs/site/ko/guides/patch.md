---
nav_title: "원자적 다중 파일 패치"
title: "원자적 다중 파일 패치 (lancet patch)"
description: "git diff/diff -u 패치의 정확한 hunk 재배치, rename/권한 지원, 경합 시 롤백."
lede: "반쪽만 적용된 패치를 남기지 않고 모든 파일과 hunk를 사전 검증한 뒤 원자적으로 반영합니다."
keywords: "patch unified diff git diff -p strip root rename mode 롤백 binary copy"
source: "https://github.com/CREE1116/lancet/blob/main/skills/lancet/references/edit-patch-rename.md"
---
`lancet patch`는 여러 파일에 걸친 표준 `git diff` 및 `diff -u` 패치에 compare-and-swap, 전체 사전 검증, 경합 롤백, Tree-sitter 구문 검사를 적용합니다.

## 플래그 & 기본 사용법 {#usage}

```bash title="터미널"
# Dry-run 검증 및 diff 미리보기 (기본 동작)
lancet patch changes.diff
git diff | lancet patch

# 하위 디렉터리 기준 적용 (-p 1은 기본값으로 a/, b/ 접두어 제거)
git diff | lancet patch --root services/auth -p 1 --apply

# a/, b/ 접두어가 없는 일반 diff -u
diff -u old.rs new.rs | lancet patch -p 0 --apply
```

## 정확한 Hunk 재배치 vs 퍼지 패치 {#exact-reloc}

일반 `patch -p1`은 문맥이 어긋나도 퍼지 매칭으로 덮어쓰거나 일부 파일만 반영합니다. `src/patch.rs`의 규칙은 다릅니다:

- **문맥·삭제 줄 완전 일치 필수:** 모든 hunk의 문맥과 `-` 줄은 파일과 정확히 일치해야 합니다.
- **고유한 최근접 오프셋 재배치:** 줄 번호가 밀렸더라도 모든 원본 줄이 *단 한 곳*에서만 정확히 일치하는 가장 가까운 위치가 있으면 그 오프셋으로 재배치하고 JSON `offsets`에 기록합니다(`0` = 정확 일치). 모호하거나 일치하지 않으면 퍼지 추정 대신 거절합니다.
- **EOF 개행 마커:** 양쪽의 `\ No newline at end of file`을 정확히 보존합니다.

## 지원 및 거부되는 Diff 구문 {#git-headers}

| Unified Diff 요소 | 상태 | `lancet patch` 동작 |
| --- | --- | --- |
| 일반 수정, 다중 hunk 파일 | <span class='tag-yes'>지원</span> | 구문 검사 및 잠금 CAS 쓰기 |
| `git diff -M` 이름 변경 (내용 수정 유무 무관) | <span class='tag-yes'>지원</span> | `rename from` / `rename to` 처리 |
| 권한 변경 (`new mode`, `new file mode`) | <span class='tag-yes'>지원</span> | Unix 파일 권한 비트(`chmod`) 적용 |
| 빈 파일 생성 / 삭제 | <span class='tag-yes'>지원</span> | `/dev/null` 및 `deleted file mode` 감지 |
| `copy from` / `copy to` | <span class='tag-no'>거부</span> | 어떤 파일도 쓰지 않고 전체 패치 중단 |
| `Binary files ...` / `GIT binary patch` | <span class='tag-no'>거부</span> | 디스크를 건드리지 않고 거부 |

## 전체 사전 검증 & 경합 시 롤백 {#atomicity}

어느 한 파일이라도 쓰기 전에 패치 안의 **모든 파일**을 읽고 검증·재배치·구문 검사합니다. 이후 사전 확인한 `rev`로 잠금 `write_if_rev`를 순차 실행하며, 도중 다른 쓰기 작업과 경합이 나면 앞서 수정한 파일까지 최선으로 되돌린 뒤 `conflict`(종료 `2`)를 반환합니다.
