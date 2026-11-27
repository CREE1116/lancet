---
nav_title: "AI 에이전트 · MCP · 스킬"
title: "AI 에이전트, 내장 MCP 서버 & 공식 스킬"
description: "Claude Code, Cursor, Codex, Antigravity, Claude Desktop용 lancet mcp 설정과 skills/lancet 설치."
lede: "--read-only, --root 지원 줄바꿈 JSON-RPC stdio 서버로 11개 도구를 연결하고 공식 스킬을 설치합니다."
keywords: "mcp server root read-only claude cursor codex antigravity skill agents.md max_output_bytes prompts"
source: "https://github.com/CREE1116/lancet/blob/main/docs/mcp.md"
---
AI 코딩 에이전트는 셸 CLI는 물론 의존성 없는 내장 stdio **Model Context Protocol(MCP)** 서버(`lancet mcp`)와 크로스 에이전트 **공식 스킬**(`skills/lancet/`)로 Lancet의 11개 엔진을 직접 호출할 수 있습니다.

## lancet mcp 실행 플래그 {#mcp-flags}

`lancet mcp`는 `stdin`/`stdout` 위에서 줄바꿈 구분 JSON-RPC 2.0(프로토콜 `2025-03-26`, `2024-11-05`)으로 동작하며 별도 데몬 없이 인프로세스로 즉시 응답합니다.

| 플래그 | 기본값 | 정확한 동작 |
| --- | --- | --- |
| `--root <DIR>` | `.` | 도구 호출의 상대 경로 기준 디렉터리 (chroot/샌드박스가 **아님**; CLI처럼 절대 경로나 `..` 경로도 허용됨) |
| `--read-only` | `false` | `tools/list`에서 `lancet_edit`, `lancet_patch`, `lancet_rename`을 제거하고 쓰기(`lancet_ast` `apply`, 인덱스 변경)를 거부 |
| `--trust-index` | `false` | 개별 호출에서 지정하지 않는 한 기본 `index_mode`를 `trust`로 설정 |
| `-j, --threads <NUM>` | CPU 코어 수 | Rayon 워커 스레드 수 |

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

## 내장 프롬프트 & AGENTS.md 규칙 {#prompts-notes}

- **`prompts/list`:** `lancet-explore`(`goal` 인자)와 `lancet-safe-edit`(`target` 인자) 제공.
- **0건 매치 규칙:** 조회·검색 도구는 `status: "empty"`일 때도 `isError: false`를 반환합니다.
- **줄 경계 바이트 상한:** `lancet_rg`는 `max_output_bytes`(기본 `131072` B = 128 KiB, 허용 범위 `256..=4194304`)에 맞춰 줄 경계에서 깔끔히 자릅니다.

```bash title="AGENTS.md / CLAUDE.md 규칙 예시"
rg/fd/ast-grep/cat/sed 대신 lancet(lancet_* MCP 또는 lancet CLI)을 우선 사용합니다.
fd/outline으로 구조를 잡고 rg/search/refs/ast로 찾은 뒤, read로 rev를 확보해 edit/patch/rename --rev --apply로 수정합니다.
```
