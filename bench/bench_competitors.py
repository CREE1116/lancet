#!/usr/bin/env python3
"""Multi-tool competitor benchmark suite across all 4 WEFT capability arenas:
  1. Code & Text Search   : weft vs ripgrep (rg)
  2. File Discovery       : weft fd vs fd (sharkdp/fd) vs rg --files vs POSIX find
  3. Structural AST Grep  : weft ast vs ast-grep (0.51+)
  4. Document & Archives  : weft vs ripgrep-all (rga) across PDF/DOCX/XLSX/HWPX/ZIP
"""

from __future__ import annotations

import argparse
import io
import json
import os
import shutil
import statistics
import struct
import subprocess
import tempfile
import time
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WEFT = ROOT / "target" / "release" / "lancet"
TOOLS_DIR = ROOT / "bench" / ".cache" / "tools"


def resolve_bin(name: str) -> str | None:
    cand = TOOLS_DIR / name
    if cand.exists() and os.access(cand, os.X_OK):
        return str(cand)
    return shutil.which(name)


def run_timed(
    cmd: list[str],
    env: dict[str, str] | None = None,
    warmup: int = 1,
    runs: int = 5,
) -> tuple[float, subprocess.CompletedProcess]:
    for _ in range(warmup):
        subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=env)
    times = []
    last = None
    for _ in range(runs):
        t0 = time.perf_counter()
        last = subprocess.run(cmd, capture_output=True, text=True, env=env)
        times.append((time.perf_counter() - t0) * 1000.0)
    assert last is not None
    return statistics.median(times), last


def make_minimal_pdf(text: str) -> bytes:
    esc = text.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")
    stream = f"BT /F1 12 Tf 72 720 Td ({esc}) Tj ET".encode("latin-1", errors="ignore")
    objs = [
        b"1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n",
        b"2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n",
        b"3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >> endobj\n",
        f"4 0 obj << /Length {len(stream)} >> stream\n".encode() + stream + b"\nendstream endobj\n",
        b"5 0 obj << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> endobj\n",
    ]
    out = bytearray(b"%PDF-1.4\n")
    offs = [0]
    for o in objs:
        offs.append(len(out))
        out.extend(o)
    xref_off = len(out)
    out.extend(f"xref\n0 {len(offs)}\n0000000000 65535 f \n".encode())
    for off in offs[1:]:
        out.extend(f"{off:010d} 00000 n \n".encode())
    out.extend(f"trailer << /Size {len(offs)} /Root 1 0 R >>\nstartxref\n{xref_off}\n%%EOF\n".encode())
    return bytes(out)


def make_docx(text: str) -> bytes:
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as zf:
        zf.writestr(
            "[Content_Types].xml",
            '<?xml version="1.0" encoding="UTF-8"?>'
            '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
            '<Default Extension="xml" ContentType="application/xml"/>'
            '<Override PartName="/word/document.xml" '
            'ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
            "</Types>",
        )
        zf.writestr(
            "_rels/.rels",
            '<?xml version="1.0" encoding="UTF-8"?>'
            '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            '<Relationship Id="rId1" '
            'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" '
            'Target="word/document.xml"/></Relationships>',
        )
        zf.writestr(
            "word/document.xml",
            '<?xml version="1.0" encoding="UTF-8"?>'
            '<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'
            f"<w:body><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:body></w:document>",
        )
    return buf.getvalue()


def make_xlsx(rows: list[list[str]]) -> bytes:
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as zf:
        zf.writestr(
            "[Content_Types].xml",
            '<?xml version="1.0" encoding="UTF-8"?>'
            '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
            '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
            '<Default Extension="xml" ContentType="application/xml"/>'
            '<Override PartName="/xl/workbook.xml" '
            'ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
            '<Override PartName="/xl/worksheets/sheet1.xml" '
            'ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>'
            "</Types>",
        )
        zf.writestr(
            "_rels/.rels",
            '<?xml version="1.0" encoding="UTF-8"?>'
            '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            '<Relationship Id="rId1" '
            'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" '
            'Target="xl/workbook.xml"/></Relationships>',
        )
        zf.writestr(
            "xl/workbook.xml",
            '<?xml version="1.0" encoding="UTF-8"?>'
            '<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" '
            'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
            '<sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>',
        )
        zf.writestr(
            "xl/_rels/workbook.xml.rels",
            '<?xml version="1.0" encoding="UTF-8"?>'
            '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            '<Relationship Id="rId1" '
            'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" '
            'Target="worksheets/sheet1.xml"/></Relationships>',
        )
        body = []
        for r_idx, row in enumerate(rows, 1):
            cells = []
            for c_idx, val in enumerate(row):
                col = chr(ord("A") + c_idx)
                cells.append(f'<c r="{col}{r_idx}" t="inlineStr"><is><t>{val}</t></is></c>')
            body.append(f'<row r="{r_idx}">{"".join(cells)}</row>')
        zf.writestr(
            "xl/worksheets/sheet1.xml",
            '<?xml version="1.0" encoding="UTF-8"?>'
            '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
            f"<sheetData>{''.join(body)}</sheetData></worksheet>",
        )
    return buf.getvalue()


def make_hwpx(text: str) -> bytes:
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as zf:
        zf.writestr("mimetype", "application/hwp+zip")
        zf.writestr(
            "Contents/section0.xml",
            f'<?xml version="1.0" encoding="UTF-8"?><hs:sec xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section"><hp:p><hp:run><hp:t>{text}</hp:t></hp:run></hp:p></hs:sec>',
        )
    return buf.getvalue()


def build_doc_corpus(root: Path) -> None:
    (root / "pdf").mkdir(parents=True, exist_ok=True)
    (root / "docx").mkdir(parents=True, exist_ok=True)
    (root / "xlsx").mkdir(parents=True, exist_ok=True)
    (root / "hwpx").mkdir(parents=True, exist_ok=True)
    (root / "archives").mkdir(parents=True, exist_ok=True)

    for i in range(30):
        kw = "QUARTERLY_REVENUE_ANOMALY" if i in (7, 19) else f"routine_memo_{i}"
        (root / "pdf" / f"report_{i:02d}.pdf").write_bytes(
            make_minimal_pdf(f"Executive summary {i}: {kw} verified by finance audit team.")
        )
    for i in range(30):
        kw = "SLA_INDEMNITY_CLAUSE" if i in (4, 15, 27) else f"standard_terms_{i}"
        (root / "docx" / f"contract_{i:02d}.docx").write_bytes(
            make_docx(f"Master Service Agreement section {i}: {kw} applies upon termination.")
        )
    for i in range(25):
        kw = "H100_CLUSTER_CAPEX" if i == 12 else f"opex_item_{i}"
        (root / "xlsx" / f"budget_{i:02d}.xlsx").write_bytes(
            make_xlsx([["Category", "Code", "Amount"], ["Infra", kw, str(10000 + i * 250)]])
        )
    for i in range(20):
        kw = "국가클라우드보안인증검토" if i in (3, 14) else f"일반업무보고_{i}"
        (root / "hwpx" / f"gov_doc_{i:02d}.hwpx").write_bytes(
            make_hwpx(f"2026년 디지털 행정 보고서 {i}: {kw} 항목 포함.")
        )
    for i in range(15):
        buf = io.BytesIO()
        kw = "RAFT_QUORUM_STALL" if i == 9 else f"healthy_heartbeat_{i}"
        with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as zf:
            for j in range(5):
                zf.writestr(f"node_{j}.log", f"2026-11-25T12:00:0{j}Z INFO status={kw} term={i*10+j}\n")
        (root / "archives" / f"cluster_logs_{i:02d}.zip").write_bytes(buf.getvalue())


def bench_fd(corpus: Path, idx_dir: Path) -> None:
    fd_bin = resolve_bin("fd")
    rg_bin = resolve_bin("rg")
    print("\n=== ARENA 2: File & Directory Discovery (`weft fd` vs `fd` vs `rg --files` vs `find`) ===")
    hdr = f"{'Workload':<30} | {'find ms':>9} | {'rg --files':>10} | {'fd ms (hits)':>16} | {'weft scan (hits)':>18} | {'weft trust (hits)':>18}"
    print(hdr)
    print("-" * len(hdr))

    # 1. All files
    m_find, p_find = run_timed(["find", str(corpus), "-type", "f"], warmup=1, runs=3)
    m_rg, p_rg = run_timed([rg_bin, "--no-ignore", "--files", str(corpus)], warmup=1, runs=5) if rg_bin else (0.0, None)
    m_fd, p_fd = run_timed([fd_bin, "--no-ignore", "-t", "f", ".", str(corpus)], warmup=1, runs=5) if fd_bin else (0.0, None)
    m_ws, p_ws = run_timed([str(WEFT), "fd", "--no-ignore", "-t", "f", ".", str(corpus), "--json"], warmup=1, runs=5)
    m_wt, p_wt = run_timed([str(WEFT), "fd", "--no-ignore", "--trust-index", "--index-dir", str(idx_dir), "-t", "f", ".", str(corpus), "--json"], warmup=1, runs=5)
    d_ws, d_wt = json.loads(p_ws.stdout), json.loads(p_wt.stdout)
    fd_hits = len(p_fd.stdout.splitlines()) if p_fd else 0
    print(
        f"{'all_files (~28k files)':<30} | {m_find:>7.1f}ms | {m_rg:>8.1f}ms | "
        f"{m_fd:>6.1f}ms ({fd_hits:>5}) | {m_ws:>7.1f}ms ({d_ws['total']:>5}) | {m_wt:>7.2f}ms ({d_wt['total']:>5})"
    )

    # 2. Extension + substring (`Cargo` in .toml)
    m_fd2, p_fd2 = run_timed([fd_bin, "--no-ignore", "-t", "f", "-e", "toml", "Cargo", str(corpus)], warmup=1, runs=5) if fd_bin else (0.0, None)
    m_ws2, p_ws2 = run_timed([str(WEFT), "fd", "--no-ignore", "-t", "f", "-e", "toml", "Cargo", str(corpus), "--json"], warmup=1, runs=5)
    m_wt2, p_wt2 = run_timed([str(WEFT), "fd", "--no-ignore", "--trust-index", "--index-dir", str(idx_dir), "-t", "f", "-e", "toml", "Cargo", str(corpus), "--json"], warmup=1, runs=5)
    d_ws2, d_wt2 = json.loads(p_ws2.stdout), json.loads(p_wt2.stdout)
    fd2_hits = len(p_fd2.stdout.splitlines()) if p_fd2 else 0
    print(
        f"{'ext:toml + pattern \"Cargo\"':<30} | {'-':>9} | {'-':>10} | "
        f"{m_fd2:>6.1f}ms ({fd2_hits:>5}) | {m_ws2:>7.1f}ms ({d_ws2['total']:>5}) | {m_wt2:>7.2f}ms ({d_wt2['total']:>5})"
    )

    # 3. Large files >= 100 KiB
    m_find3, _ = run_timed(["find", str(corpus), "-type", "f", "-size", "+100k"], warmup=1, runs=3)
    m_fd3, p_fd3 = run_timed([fd_bin, "--no-ignore", "-t", "f", "-S", "+100ki", ".", str(corpus)], warmup=1, runs=5) if fd_bin else (0.0, None)
    m_ws3, p_ws3 = run_timed([str(WEFT), "fd", "--no-ignore", "-t", "f", "-S", "+100ki", ".", str(corpus), "--json"], warmup=1, runs=5)
    m_wt3, p_wt3 = run_timed([str(WEFT), "fd", "--no-ignore", "--trust-index", "--index-dir", str(idx_dir), "-t", "f", "-S", "+100ki", ".", str(corpus), "--json"], warmup=1, runs=5)
    d_ws3, d_wt3 = json.loads(p_ws3.stdout), json.loads(p_wt3.stdout)
    fd3_hits = len(p_fd3.stdout.splitlines()) if p_fd3 else 0
    print(
        f"{'min_size >= 100 KiB':<30} | {m_find3:>7.1f}ms | {'-':>10} | "
        f"{m_fd3:>6.1f}ms ({fd3_hits:>5}) | {m_ws3:>7.1f}ms ({d_ws3['total']:>5}) | {m_wt3:>7.2f}ms ({d_wt3['total']:>5})"
    )


def bench_ast(corpus: Path, idx_dir: Path) -> None:
    ag_bin = resolve_bin("ast-grep")
    print("\n=== ARENA 3: Structural AST Search (`weft ast` vs `ast-grep` across 28,352 files) ===")
    hdr = f"{'AST Pattern (Rust)':<32} | {'ast-grep ms (hits)':>20} | {'weft scan ms':>14} | {'weft auto ms (cands)':>22} | {'weft trust ms (hits)':>22}"
    print(hdr)
    print("-" * len(hdr))

    cases = [
        ("std::mem::transmute($X)", "std::mem::transmute($X)"),
        ("$X.unwrap_or_else($F)", "$X.unwrap_or_else($F)"),
        ("unreachable!($$$ARGS)", "unreachable!($$$ARGS)"),
        ("ThreadPoolBuilder::new()", "ThreadPoolBuilder::new()"),
    ]
    for label, pat in cases:
        if ag_bin:
            m_ag, p_ag = run_timed(
                [
                    ag_bin,
                    "run",
                    "-p",
                    pat,
                    "-l",
                    "rust",
                    "--no-ignore",
                    "hidden",
                    "--no-ignore",
                    "vcs",
                    "--no-ignore",
                    "dot",
                    str(corpus),
                    "--json=compact",
                ],
                warmup=0,
                runs=3,
            )
            ag_hits = len(json.loads(p_ag.stdout)) if p_ag.stdout.strip() else 0
            ag_col = f"{m_ag:10.1f}ms ({ag_hits:>5})"
        else:
            ag_col = f"{'-':>20}"

        m_ws, p_ws = run_timed([str(WEFT), "ast", pat, str(corpus), "--lang", "rs", "--no-ignore", "--no-index", "--json"], warmup=0, runs=3)
        d_ws = json.loads(p_ws.stdout)

        m_wa, p_wa = run_timed([str(WEFT), "ast", pat, str(corpus), "--lang", "rs", "--no-ignore", "--index-dir", str(idx_dir), "--json"], warmup=1, runs=5)
        d_wa = json.loads(p_wa.stdout)

        m_wt, p_wt = run_timed([str(WEFT), "ast", pat, str(corpus), "--lang", "rs", "--no-ignore", "--trust-index", "--index-dir", str(idx_dir), "--json"], warmup=1, runs=5)
        d_wt = json.loads(p_wt.stdout)

        cands = d_wa["stats"].get("index_candidates", "-")
        print(
            f"{label:<32} | {ag_col:>20} | {m_ws:>8.1f}ms ({d_ws['matched']:>3}) | "
            f"{m_wa:>10.1f}ms ({cands:>5}f) | {m_wt:>10.1f}ms ({d_wt['matched']:>5})"
        )


def bench_docs() -> None:
    rga_bin = resolve_bin("rga")
    env = os.environ.copy()
    env["PATH"] = f"{TOOLS_DIR}:/opt/homebrew/bin:{env.get('PATH', '')}"

    with tempfile.TemporaryDirectory(prefix="weft_doc_bench_") as td:
        root = Path(td) / "docs_corpus"
        idx_dir = Path(td) / "idx"
        rga_cache = Path(td) / "rga_cache"
        env["RGA_CACHE_PATH"] = str(rga_cache)

        build_doc_corpus(root)
        t0 = time.perf_counter()
        subprocess.run([str(WEFT), "index", str(root), "--index-dir", str(idx_dir)], check=True, stdout=subprocess.DEVNULL)
        idx_ms = (time.perf_counter() - t0) * 1000.0

        print(f"\n=== ARENA 4: Multi-Format Documents & Archives (120 PDF/DOCX/XLSX/HWPX/ZIP, index build={idx_ms:.1f}ms) ===")
        hdr = f"{'Document Query':<34} | {'rga no-cache':>15} | {'rga cached':>14} | {'weft scan':>13} | {'weft auto':>13} | {'weft trust':>13}"
        print(hdr)
        print("-" * len(hdr))

        queries = [
            ("PDF: QUARTERLY_REVENUE_ANOMALY", "QUARTERLY_REVENUE_ANOMALY", '"QUARTERLY_REVENUE_ANOMALY"'),
            ("DOCX: SLA_INDEMNITY_CLAUSE", "SLA_INDEMNITY_CLAUSE", '"SLA_INDEMNITY_CLAUSE"'),
            ("XLSX: H100_CLUSTER_CAPEX", "H100_CLUSTER_CAPEX", '"H100_CLUSTER_CAPEX"'),
            ("ZIP logs: RAFT_QUORUM_STALL", "RAFT_QUORUM_STALL", '"RAFT_QUORUM_STALL"'),
            ("HWPX (KO): 국가클라우드보안인증검토", "국가클라우드보안인증검토", '"국가클라우드보안인증검토"'),
        ]

        # Warm rga cache once before timed cached runs
        if rga_bin:
            subprocess.run(
                [rga_bin, f"--rga-cache-path={rga_cache}", "-l", "warmup_populate_cache", str(root)],
                env=env,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )

        cold_idx = 0
        for label, raw_q, wql in queries:
            if rga_bin:
                cold_times = []
                rc_hits = 0
                for _ in range(3):
                    cold_idx += 1
                    cpath = Path(td) / f"rga_cold_{cold_idx}"
                    t_c0 = time.perf_counter()
                    p_rc = subprocess.run(
                        [rga_bin, f"--rga-cache-path={cpath}", "-l", raw_q, str(root)],
                        env=env,
                        capture_output=True,
                        text=True,
                    )
                    cold_times.append((time.perf_counter() - t_c0) * 1000.0)
                    rc_hits = len([x for x in p_rc.stdout.splitlines() if x.strip()])
                m_rga_cold = statistics.median(cold_times)
                m_rga_warm, p_rw = run_timed(
                    [rga_bin, f"--rga-cache-path={rga_cache}", "-l", raw_q, str(root)],
                    env=env,
                    warmup=1,
                    runs=5,
                )
                rw_hits = len([x for x in p_rw.stdout.splitlines() if x.strip()])
                rc_s = f"{m_rga_cold:6.1f}ms ({rc_hits})"
                rw_s = f"{m_rga_warm:6.1f}ms ({rw_hits})"
            else:
                rc_s = rw_s = "-"

            # Same question as rga -l: which files contain the text (documents included).
            rg = [str(WEFT), "rg", "--docs", "-l", raw_q, str(root)]
            m_ws, p_ws = run_timed([*rg, "--no-index"], warmup=1, runs=5)
            m_wa, p_wa = run_timed([*rg, "--index-dir", str(idx_dir)], warmup=1, runs=5)
            m_wt, p_wt = run_timed([*rg, "--trust-index", "--index-dir", str(idx_dir)], warmup=1, runs=5)
            hits = [len([x for x in p.stdout.splitlines() if x.strip()]) for p in (p_ws, p_wa, p_wt)]

            print(
                f"{label:<34} | {rc_s:>15} | {rw_s:>14} | "
                f"{m_ws:>6.1f}ms ({hits[0]}) | {m_wa:>6.1f}ms ({hits[1]}) | {m_wt:>6.2f}ms ({hits[2]})"
            )


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--corpus", type=Path, default=Path.home() / ".cargo" / "registry" / "src")
    ap.add_argument("--index-dir", type=Path, default=ROOT / "bench" / ".cache" / "cargo-idx")
    args = ap.parse_args()

    corpus = args.corpus.expanduser().resolve()
    idx_dir = args.index_dir.resolve()
    if not idx_dir.exists():
        idx_dir.mkdir(parents=True, exist_ok=True)
        subprocess.run([str(WEFT), "index", str(corpus), "--index-dir", str(idx_dir), "--no-ignore"], check=True)

    bench_fd(corpus, idx_dir)
    bench_ast(corpus, idx_dir)
    bench_docs()


if __name__ == "__main__":
    main()
