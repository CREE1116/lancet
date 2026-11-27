---
nav_title: "AI 에이전트 · MCP · 스킬"
title: "AI 에이전트, 내장 MCP 서버 & 공식 스킬"
description: "Claude Code, Cursor, Codex, Antigravity, Claude Desktop용 lancet mcp 설정과 skills/lancet 설치."
lede: "CLI 인자를 그대로 받는 `lancet` 도구 하나를 줄바꿈 JSON-RPC stdio로 제공하고(--read-only, --root 지원), 공식 스킬을 설치합니다."
keywords: "mcp server root read-only claude cursor codex antigravity skill agents.md max_output_bytes args stdin"
source: "https://github.com/CREE1116/lancet/blob/main/docs/mcp.md"
---
AI 코딩 에이전트는 셸 CLI, 의존성 없는 내장 stdio **Model Context Protocol(MCP)** 서버(`lancet mcp`), 크로스 에이전트 **공식 스킬**(`skills/lancet/`)로 Lancet을 씁니다. MCP 도구는 CLI와 같은 인자를 받습니다.

## lancet mcp 실행 플래그 {#mcp-flags}

`lancet mcp`는 `stdin`/`stdout` 위에서 줄바꿈 구분 JSON-RPC 2.0(프로토콜 `2025-03-26`, `2024-11-05`)으로 동작하고, 도구 호출마다 `--root` 디렉터리에서 `lancet` CLI를 실행합니다.

| 플래그 | 기본값 | 정확한 동작 |
| --- | --- | --- |
| `--root <DIR>` | `.` | 명령을 실행할 디렉터리. 상대 경로의 기준입니다 (chroot/샌드박스가 **아님**; CLI처럼 절대 경로나 `..` 경로도 허용됨) |
| `--read-only` | `false` | 쓰는 명령을 거절: `edit`, `patch`, `rename`, `index`, 그리고 `--apply`가 들어간 모든 호출 |
| `--trust-index` | `false` | 검색(`rg`, `fd`, `ast`, `refs`, `batch`, 쿼리)이 인덱스 모드를 직접 정하지 않으면 `--trust-index`를 붙임 |
| `-j, --threads <NUM>` | CPU 코어 수 | 같은 검색들에 `-j NUM`을 붙임(직접 지정하지 않은 경우) |

## 클라이언트별 설정 {#client-configs}

전체 템플릿: [`skills/lancet/references/mcp-config.json`](https://github.com/CREE1116/lancet/blob/main/skills/lancet/references/mcp-config.json).

### Claude Code {#claude-code}

```bash title="터미널"
# 읽기 + 잠금 CAS 쓰기
claude mcp add --transport stdio lancet -- lancet mcp --root .

# 읽기 전용 탐색 서버
claude mcp add --transport stdio lancet -- lancet mcp --root . --read-only
```

### Cursor (.cursor/mcp.json 또는 ~/.cursor/mcp.json) {#cursor}

```json title="JSON"
{
  "mcpServers": {
    "lancet": {
      "command": "lancet",
      "args": ["mcp", "--root", "."]
    }
  }
}
```

### Claude Desktop / Windsurf / Antigravity {#desktop-agy}

데스크톱 클라이언트가 저장소 루트를 CWD로 두지 않고 실행한다면 `--root`에 절대 경로를 넘기세요:

```json title="JSON"
{
  "mcpServers": {
    "lancet": {
      "command": "lancet",
      "args": ["mcp", "--root", "/abs/path/to/your/repo"]
    }
  }
}
```

### OpenAI Codex (~/.codex/config.toml) {#codex}

```toml title="TOML"
[mcp_servers.lancet]
command = "lancet"
args = ["mcp", "--root", "."]
```

## 공식 에이전트 스킬 설치 {#skill-install}

[`skills/lancet/`](https://github.com/CREE1116/lancet/tree/main/skills/lancet)는 `SKILL.md`와 LQL·24개 AST 언어·edit/patch/rename CAS 규칙·MCP 설정용 `references/` 문서를 포함합니다.

```bash title="터미널"
# 프로젝트 로컬 (.agents 공통 규약)
mkdir -p .agents/skills && ln -s "$(pwd)/skills/lancet" .agents/skills/lancet

# 개인 전역 설치 (Claude Code / Codex / Antigravity)
mkdir -p ~/.claude/skills && cp -R skills/lancet ~/.claude/skills/lancet
mkdir -p ~/.codex/skills  && cp -R skills/lancet ~/.codex/skills/lancet
mkdir -p ~/.gemini/config/skills && cp -R skills/lancet ~/.gemini/config/skills/lancet
```

## `lancet` 도구 {#mcp-tool}

서버는 **도구 하나**, `lancet`만 제공합니다. 인자는 명령줄 그대로입니다. `{"args": ["rg", "-w", "spin_lock", "src"]}`는 `lancet rg -w spin_lock src`와 똑같이(같은 바이너리, 플래그, 출력, 종료 코드) `--root` 디렉터리에서 실행됩니다. CLI에 생긴 기능은 MCP에서도 바로 쓸 수 있고, 스키마 전체가 약 2.4 KB입니다(예전 11개 도구 서버는 12.5 KB).

| 필드 | 타입 | 의미 |
| --- | --- | --- |
| `args` | 문자열 배열 (필수) | `lancet` 뒤의 인자, 원소 하나에 인자 하나. 셸을 거치지 않으므로 따옴표, glob, 파이프, `&&`는 쓰지 않습니다(`["'\"OOM\" in line'"]`이 아니라 `["\"OOM\" in line", "src"]`). |
| `stdin` | 문자열 | `-` 인자에 넘길 내용: `edit --edits -`의 hunk, `patch -`의 diff, `--with-file -`의 텍스트. |
| `max_output_bytes` | 정수 | 출력 상한. 기본 `131072`, 허용 범위 `256..=4194304`. 넘치면 잘리고 `truncated`가 표시됩니다. |

결과는 명령의 stdout, 그 뒤에 stderr, 종료 코드가 `0`이 아니면 `[exit N]` 순서입니다. `isError`는 종료 코드 `2`(거절·잘못된 입력)일 때만 `true`이고, `1`은 "결과 없음"이라 오류가 아닙니다.

```json title="tools/call"
{"name": "lancet", "arguments": {"args": ["edit", "--edits", "-", "--apply"],
  "stdin": "[{\"path\": \"src/a.rs\", \"rev\": \"7b47…\", \"find\": \"old(\", \"with\": \"new(\"}]"}}
```

```bash title="AGENTS.md / CLAUDE.md 규칙 예시"
rg/fd/ast-grep/cat/sed 대신 lancet(CLI, 또는 같은 인자를 받는 MCP `lancet` 도구)을 우선 사용합니다.
fd/outline/refs --rank로 구조를 잡고 rg/refs/ast로 찾은 뒤, read에서 rev를 확인해 edit/patch/rename --rev --apply로 수정합니다.
```
