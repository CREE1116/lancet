---
nav_title: "벤치마크 & 경쟁 비교"
title: "벤치마크 & 정직한 경쟁 도구 비교"
description: "8개 코드베이스 SUITE.md 수치(rg/zoekt/ast-grep) 및 COMPETITORS.md(tgrep/grix/aft/tensor-grep/probe) 결과."
lede: "실제 저장소에서 ripgrep, zoekt, ast-grep, tgrep, grix, aft, probe와 비교해 앞서는 점과 뒤지는 점을 모두 공개합니다."
keywords: "벤치마크 suite competitors zoekt tgrep grix aft tensor-grep probe linux llvm rust 최신성"
source: "https://github.com/CREE1116/lancet/blob/main/bench/results/COMPETITORS.md"
---
Apple M4 Pro(arm64 macOS), 웜 캐시, `hyperfine -N` 5회 중앙값 기준입니다. `auto`, `--trust-index`, `--no-index`의 모든 텍스트 출력은 **`ripgrep 15.2`와 바이트 단위로 대조**했습니다.

![8개 코드베이스에서 lancet과 zoekt의 인덱스 빌드 시간, 디스크 크기, 피크 메모리.](diagram:bench-index-footprint.ko.svg "zoekt 대비 빌드 4~25배 빠름, 디스크 9~30배 작음, 피크 RAM 3.2~5.0배 적음.")

![LLVM 19.1의 8개 텍스트 쿼리 유형과 8개 코드베이스 AST 검색 성능.](diagram:bench-workload-profile.ko.svg "희귀/흔한 리터럴, -i, -w, 리터럴 정규식, 리터럴 없는 정규식, 0건, -c, AST 포함.")

## 핵심 8개 코드베이스 벤치마크 (SUITE.md) {#suite-table}

| 코퍼스 | 파일 | lancet 인덱스 (빌드 · 디스크 · 피크) | zoekt 인덱스 (빌드 · 디스크 · 피크) | auto 대 rg | trust 대 rg | AST 대 ast-grep |
| --- | ---: | --- | --- | --- | --- | --- |
| Linux v6.12 (C) | 86,680 | 3.3 s · 134 MB · 358 MB | 82 s · 2.9 GB · 1.2 GB | 5.8~7.5배 | 13~403배 | 4.0배 |
| LLVM 19.1 (C++) | 146,319 | 4.7 s · 122 MB · 280 MB | 53 s · 3.7 GB · 1.3 GB | 6.1~8.0배 | 34~786배 | 2.3배 |
| rust 1.83 (Rust) | 49,964 | 1.2 s · 29 MB · 184 MB | 4.9 s · 327 MB · 739 MB | 1.5~9.6배 | 2~273배 | 1.8배 |
| kubernetes 1.31 (Go) | 26,232 | 1.0 s · 30 MB · 224 MB | 4.1 s · 537 MB · 1.0 GB | 3.5~5.8배 | 9~210배 | 1.9배 |
| VS Code 1.95 (TS) | 7,708 | 0.4 s · 12 MB · 114 MB | 2.4 s · 245 MB · 573 MB | 2.1~3.9배 | 4~63배 | 2.7배 |
| Django 5.1 (Python) | 6,815 | 0.2 s · 7 MB · 76 MB | 1.2 s · 95 MB · 302 MB | 2.4~2.9배 | 10~60배 | 1.4배 |
| CPython 3.13 (C/Py) | 4,882 | 0.3 s · 10 MB · 116 MB | 3.7 s · 227 MB · 509 MB | 3.0~5.4배 | 5~30배 | 2.6배 |
| Spring 6.1 (Java) | 10,163 | 0.3 s · 12 MB · 122 MB | 1.9 s · 146 MB · 395 MB | 2.9~5.3배 | 6~95배 | 3.1배 |

## 인덱스·에이전트 도구 비교 (COMPETITORS.md) {#competitors}

재현 스크립트: [`bench/competitors_agents.py`](https://github.com/CREE1116/lancet/blob/main/bench/competitors_agents.py) ([전체 원시 표](https://github.com/CREE1116/lancet/blob/main/bench/results/COMPETITORS.md)).

### 수정 직후 0 ms 최신성 & 정확도 (Django .py 20개 수정) {#comp-freshness}

| 도구 / 모드 | 새 토큰 탐지 (0 ms) | 삭제 토큰 잔존 (0 ms) | 20/20 rg 줄 수 일치 |
| --- | --- | --- | --- |
| `lancet rg` (`auto`) | **20 / 20** | 0 / 20 | **20 / 20** |
| `lancet rg --trust-index` | 0 / 20 | 0 / 20 | **20 / 20** |
| `grix` (auto) | **20 / 20** | 0 / 20 | 19 / 20 (LLVM 리터럴 없는 정규식 2줄 누락) |
| `tgrep` (서버리스) | 0 / 20 | 0 / 20 | **20 / 20** |
| `tgrep serve` (상주 데몬) | **20 / 20** | 0 / 20 | **20 / 20** |
| `aft` (상주 stdio + 워처) | 0 / 20 (~1.5 s 대기 필요) | 0 / 20 | 일부 파일 누락 (Rust 49,031 vs 49,964) |

### 정직한 요약: 우위와 열위 {#comp-honest}

> [!NOTE] 양쪽 숫자를 가감 없이 확인하세요
> - **Lancet이 앞서는 영역:** 데몬 없는 20/20 0 ms 최신성; `aft`/`tgrep` 대비 **1/3~1/7 인덱스 디스크 크기**(Linux 141.8 MB vs 368.5 MB / 955.2 MB); `grix` 대비 **약 1/3 피크 빌드 RAM**(Linux 353.5 MB vs 1,001.6 MB); Django·Rust·Linux·LLVM 모두에서 가장 빠른 리터럴 AST 검색; 읽기·편집·패치·이름 변경까지 단일 바이너리 통합.
> - **상주 데몬이나 trust 전용 도구가 더 빠른 영역:** 상주 데몬 `tgrep serve`는 Rust(119.2 ms vs 508.7 ms)·LLVM(19.9 ms vs 295.8 ms) `regex with literal`과 소형 리터럴 없는 정규식(Django 15.4 ms, Rust 60.9 ms)에서 데몬 없는 `lancet auto`보다 빠릅니다. `grix --no-auto-index`는 Linux 희귀 리터럴에서 `lancet --trust-index`보다 빠릅니다(11.5 ms vs 43.6 ms). 상주 `aft`(RAM 587~1,260 MB)는 Rust 희귀 리터럴(8.5 ms vs 73.5 ms)에서 앞섭니다. 리터럴 없는 정규식과 AST 패턴은 각각 `rg`(최대 1.4배 느림), `ast-grep`(0.6~1.0배) 수준입니다.
