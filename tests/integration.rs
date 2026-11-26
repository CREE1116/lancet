use lancet::ast_grep::{search_ast, AstOptions};
use lancet::fd::{find_files, parse_duration, parse_size, render_tree, FdOptions, PatternMode};
use lancet::index::{self, Index};
use lancet::outline::{build_outline, OutlineOptions};
use lancet::output::{Response, Status};
use lancet::query::{parse, FileType};
use lancet::read_cmd::{read_file_range, ReadOptions};
use lancet::refs::{find_refs, RefsOptions};
use lancet::search::{explain, search, search_batch, IndexMode, SearchOptions};
use lancet::walk::WalkOpts;
use std::fs;
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};
use tempfile::TempDir;

fn opts(root: &Path, mode: IndexMode) -> SearchOptions {
    let mut o = SearchOptions::new(root);
    o.index = mode;
    o
}

fn build_rich_corpus() -> TempDir {
    let dir = TempDir::new().unwrap();
    let r = dir.path();
    fs::create_dir_all(r.join(".git")).unwrap();
    fs::create_dir_all(r.join("src/sub")).unwrap();
    fs::create_dir_all(r.join("docs")).unwrap();
    fs::create_dir_all(r.join("data")).unwrap();
    fs::create_dir_all(r.join("ignored_dir")).unwrap();

    fs::write(r.join(".gitignore"), "ignored_dir/\n*.tmp\n").unwrap();
    fs::write(
        r.join("ignored_dir/secret.rs"),
        "fn gradient() { clip(); }\n",
    )
    .unwrap();
    fs::write(r.join("skip.tmp"), "gradient clip optimizer\n").unwrap();

    fs::write(
        r.join("src/train.py"),
        "import torch\n\ndef train_step(opt, params):\n    # Gradient clipping before optimizer step\n    clip_grad_norm_(params, 1.0)\n    opt.step()\n    return 0\n\ndef eval_step(model):\n    return model()\n\n# second block\nalpha\nbeta\n\nalpha\n\nbeta\n",
    )
    .unwrap();

    fs::write(
        r.join("src/sub/engine.rs"),
        "pub struct EngineConfig { pub lr: f64 }\n\npub fn run() {\n    // OOM on GPU, not CPU\n    let kelvin = \"\u{212A}elvin\";\n    let ab = \"ab\";\n    let single = \"z\";\n}\n\nfn helper_transmute() {\n    let banana = \"banana\";\n    let aaaaa = \"aaaaa\";\n    let transmute_ok = 1;\n}\n",
    )
    .unwrap();

    fs::write(
        r.join("src/app.ts"),
        "export interface RunnerOpts { steps: number }\nexport const runStep = (x: number) => {\n  return x + 1;\n};\nexport function legacyHelper() { return 0; }\n",
    )
    .unwrap();

    fs::write(
        r.join("src/worker.go"),
        "package main\ntype JobSpec struct { ID int }\nfunc processJob() int {\n    return 1\n}\n",
    )
    .unwrap();

    fs::write(
        r.join("docs/notes.md"),
        "# Training Notes\n\n학습률(learning_rate)은 0.0003으로 설정한다.\n학습율 오타 테스트와 accumlation 오타 테스트.\n\nOOM이 발생하면 batch size를 줄이거나 gradient와 clip을 함께 확인한다.\n",
    )
    .unwrap();

    fs::write(
        r.join("data/results.csv"),
        "model,optimizer,lr,acc\n350M,AdamW,0.0003,82.1\n600M,SGD,0.001,86.3\n",
    )
    .unwrap();

    fs::write(
        r.join("data/config.json"),
        "{\n  \"optimizer\": \"AdamW\",\n  \"learning_rate\": 0.0003,\n  \"max_grad_norm\": 1.0\n}\n",
    )
    .unwrap();

    let mut big = String::new();
    for i in 0..400 {
        big.push_str(&format!(
            "line {i:03}: filler tokens and occasionally optimizer_hit_{i}\n"
        ));
    }
    fs::write(r.join("docs/big.txt"), big).unwrap();

    fs::write(r.join("docs/empty.txt"), b"").unwrap();
    fs::write(
        r.join("data/blob.bin"),
        b"optimizer\x00\x01\x02gradient clip",
    )
    .unwrap();
    fs::write(r.join("docs/latin.txt"), b"caf\xe9 optimizer_nonutf8\n").unwrap();

    dir
}

fn assert_eq_responses(a: &Response, b: &Response, label: &str) {
    assert_eq!(a.status, b.status, "{label}: status mismatch");
    assert_eq!(a.matched, b.matched, "{label}: matched mismatch");
    assert_eq!(
        a.matched_is_lower_bound, b.matched_is_lower_bound,
        "{label}: lower_bound mismatch"
    );
    assert_eq!(a.returned, b.returned, "{label}: returned mismatch");
    assert_eq!(a.truncated, b.truncated, "{label}: truncated mismatch");
    assert_eq!(a.results, b.results, "{label}: results mismatch");
    assert_eq!(a.terms, b.terms, "{label}: terms mismatch");
    assert_eq!(a.budget, b.budget, "{label}: budget mismatch");
}

#[test]
fn parser_roundtrip_and_canonicalization() {
    let cases = [
        "optimizer",
        "find \"optimizer\"",
        "\"a\" \"b\"",
        "(\"OOM\" OR \"out of memory\") AND NOT \"CPU\" in line",
        "near(\"gradient\", \"clip\", 30) in block | expand parent | limit 5",
        "before(regex(\"clip_\\w+\"), fuzzy(\"optimzer\", 2), 10) in file",
        "\"AdamW\" in type:(code, config) ext:json path:\"data/**\" | expand context:2 | group file | count | limit 10",
        "symbol(\"train_step\") AND \"opt\" in function | expand parent",
        "symbol:EngineConfig | files | sort count asc | budget tokens:500 | limit 5",
        "\"transmute\" | terms top:10",
    ];
    for qstr in cases {
        let q = parse(qstr).unwrap_or_else(|e| panic!("failed to parse `{qstr}`: {e}"));
        let canon = q.to_string();
        let q2 = parse(&canon).unwrap_or_else(|e| panic!("failed to re-parse `{canon}`: {e}"));
        assert_eq!(q, q2, "round-trip mismatch for `{qstr}` -> `{canon}`");
    }

    assert_eq!(
        parse("symbol:train_step").unwrap().to_string(),
        "find symbol(\"train_step\")"
    );
    assert_eq!(
        parse("\"x\" | terms | budget 250").unwrap().to_string(),
        "find \"x\" | terms top:15 | budget tokens:250"
    );
    assert_eq!(
        parse("\"b\" AND \"a\" AND \"b\"").unwrap().to_string(),
        "find \"a\" AND \"b\""
    );
    assert_eq!(
        parse("(\"b\" OR \"a\") OR \"b\"").unwrap().to_string(),
        "find \"a\" OR \"b\""
    );
    assert_eq!(parse("NOT NOT \"x\"").unwrap().to_string(), "find \"x\"");
}

#[test]
fn fixture_jsonl_queries_parse_and_roundtrip() {
    let Some(dir_os) = std::env::var_os("LANCET_QUERY_FIXTURES_DIR") else {
        return;
    };
    let base = Path::new(&dir_os);
    if !base.exists() {
        return;
    }
    for fname in ["train.jsonl", "valid.jsonl"] {
        let p = base.join(fname);
        if !p.exists() {
            continue;
        }
        let body = fs::read_to_string(&p).unwrap();
        for (idx, line) in body.lines().enumerate() {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            let qstr = v["messages"].as_array().unwrap().last().unwrap()["content"]
                .as_str()
                .unwrap();
            let q = parse(qstr).unwrap_or_else(|e| panic!("{fname}:{}: `{qstr}` -> {e}", idx + 1));
            let q2 = parse(&q.to_string()).unwrap();
            assert_eq!(q, q2, "{fname}:{} round-trip mismatch", idx + 1);
        }
    }
}

#[test]
fn strict_parse_errors() {
    let dir = TempDir::new().unwrap();
    let bad = [
        "",
        "   ",
        "\"unterminated",
        "regex(\"[unclosed\")",
        "fuzzy(\"two words\", 1)",
        "fuzzy(\"abc\", 4)",
        "symbol(\"two words\")",
        "\"x\" | limit 0",
        "\"x\" | budget 0",
        "\"x\" | terms top:0",
        "\"x\" | sort bogus",
        "\"x\" | bogus",
        "\"x\" in bogus:1",
        "\"x\" in type:bogus",
        "\"x\" in line in block",
        "\"x\" )",
    ];
    for q in bad {
        let r = search(q, &opts(dir.path(), IndexMode::Off));
        assert_eq!(
            r.status,
            Status::InvalidQuery,
            "expected InvalidQuery for `{q}`"
        );
        assert!(r.error.is_some(), "expected error payload for `{q}`");
    }
}

#[test]
fn ast_functions_and_symbols_across_languages() {
    let dir = build_rich_corpus();
    let root = dir.path();

    // Python function scope + expand parent + symbol metadata
    let r_py = search(
        "\"opt\" in function | expand parent",
        &opts(root, IndexMode::Off),
    );
    assert_eq!(r_py.status, Status::Complete);
    assert_eq!(r_py.matched, 1);
    assert_eq!(r_py.results[0].path, "src/train.py");
    assert_eq!(r_py.results[0].symbol.as_deref(), Some("train_step"));
    assert!(r_py.results[0]
        .text
        .as_deref()
        .unwrap()
        .contains("clip_grad_norm_"));

    // TypeScript arrow function + Go function
    let r_ts = search("\"x + 1\" in function", &opts(root, IndexMode::Off));
    assert_eq!(r_ts.matched, 1);
    assert_eq!(r_ts.results[0].symbol.as_deref(), Some("runStep"));

    let r_go = search("\"return 1\" in function", &opts(root, IndexMode::Off));
    assert_eq!(r_go.matched, 1);
    assert_eq!(r_go.results[0].symbol.as_deref(), Some("processJob"));

    // Symbol definitions: exact identifier match, NOT substring or call-site!
    let r_sym = search(
        "symbol(\"train_step\") AND \"opt\" in function",
        &opts(root, IndexMode::Off),
    );
    assert_eq!(r_sym.matched, 1);
    assert_eq!(r_sym.results[0].symbol.as_deref(), Some("train_step"));

    let r_partial = search("symbol(\"train\")", &opts(root, IndexMode::Off));
    assert_eq!(r_partial.status, Status::Empty);

    for sym in [
        "EngineConfig",
        "RunnerOpts",
        "runStep",
        "JobSpec",
        "processJob",
    ] {
        let r = search(&format!("symbol:{sym}"), &opts(root, IndexMode::Off));
        assert_eq!(r.matched, 1, "expected symbol:{sym} to match");
    }
}

#[test]
fn semantics_matchers_units_filters_and_pipes() {
    let dir = build_rich_corpus();
    let root = dir.path();

    let r = search("\"kelvin\" in line", &opts(root, IndexMode::Off));
    assert_eq!(r.status, Status::Complete);
    assert_eq!(r.matched, 1);
    assert_eq!(r.results[0].path, "src/sub/engine.rs");

    let r_kr = search("\"학습률\"", &opts(root, IndexMode::Off));
    assert_eq!(r_kr.matched, 1);
    assert_eq!(r_kr.results[0].path, "docs/notes.md");

    let r_short = search("\"ab\" in ext:rs", &opts(root, IndexMode::Off));
    assert_eq!(r_short.matched, 1);
    let r_one = search("\"z\" in ext:rs", &opts(root, IndexMode::Off));
    assert_eq!(r_one.matched, 1);

    let r_file = search(
        "regex(\"alpha\\nbeta\") in file",
        &opts(root, IndexMode::Off),
    );
    assert_eq!(r_file.matched, 1);
    let r_block = search(
        "regex(\"alpha\\nbeta\") in block",
        &opts(root, IndexMode::Off),
    );
    assert_eq!(r_block.matched, 1);
    let r_line = search(
        "regex(\"alpha\\nbeta\") in line",
        &opts(root, IndexMode::Off),
    );
    assert_eq!(r_line.status, Status::Empty);
    let r_gap = search(
        "regex(\"alpha\\n\\nbeta\") in block",
        &opts(root, IndexMode::Off),
    );
    assert_eq!(r_gap.status, Status::Empty);

    let r_near = search(
        "near(\"gradient\", \"clip\", 3) in ext:md",
        &opts(root, IndexMode::Off),
    );
    assert_eq!(r_near.matched, 1);
    let r_before_ok = search(
        "before(\"gradient\", \"clip\", 3) in ext:md",
        &opts(root, IndexMode::Off),
    );
    assert_eq!(r_before_ok.matched, 1);
    let r_before_rev = search(
        "before(\"clip\", \"gradient\", 3) in ext:md",
        &opts(root, IndexMode::Off),
    );
    assert_eq!(r_before_rev.status, Status::Empty);

    let r_fz = search(
        "fuzzy(\"accumlation\", 0) in ext:md",
        &opts(root, IndexMode::Off),
    );
    assert_eq!(r_fz.matched, 1);
    let r_fz_typo = search(
        "fuzzy(\"accumulation\", 2) in ext:md",
        &opts(root, IndexMode::Off),
    );
    assert_eq!(r_fz_typo.matched, 1);
    let r_fz_kr = search(
        "fuzzy(\"학습율\", 1) in ext:md",
        &opts(root, IndexMode::Off),
    );
    assert_eq!(r_fz_kr.matched, 1);

    let r_bin = search(
        "\"gradient\" in path:\"data/**\"",
        &opts(root, IndexMode::Off),
    );
    assert_eq!(r_bin.status, Status::Empty);
    let r_ig = search(
        "\"gradient\" in path:\"ignored_dir/**\"",
        &opts(root, IndexMode::Off),
    );
    assert_eq!(r_ig.status, Status::Empty);

    let r_latin = search("\"optimizer_nonutf8\"", &opts(root, IndexMode::Off));
    assert_eq!(r_latin.matched, 1);
    assert_eq!(r_latin.results[0].path, "docs/latin.txt");

    let r_not_file = search(
        "NOT \"absent_xyz_999\" in path:\"docs/empty.txt\"",
        &opts(root, IndexMode::Off),
    );
    assert_eq!(r_not_file.matched, 1);
    let r_not_line = search(
        "NOT \"absent_xyz_999\" in line in path:\"docs/empty.txt\"",
        &opts(root, IndexMode::Off),
    );
    assert_eq!(r_not_line.status, Status::Empty);

    // | files, | sort count [asc], | terms, | budget
    let r_files = search("\"optimizer\" | files", &opts(root, IndexMode::Off));
    assert!(r_files.matched >= 4);
    assert!(r_files
        .results
        .iter()
        .all(|it| it.lines.is_empty() && it.count.is_none() && it.text.is_none()));

    let r_sort_desc = search(
        "\"optimizer\" in line | count | sort count",
        &opts(root, IndexMode::Off),
    );
    let counts_desc: Vec<usize> = r_sort_desc
        .results
        .iter()
        .map(|it| it.count.unwrap())
        .collect();
    assert!(counts_desc.windows(2).all(|w| w[0] >= w[1]));

    let r_sort_asc = search(
        "\"optimizer\" in line | count | sort count asc",
        &opts(root, IndexMode::Off),
    );
    let counts_asc: Vec<usize> = r_sort_asc
        .results
        .iter()
        .map(|it| it.count.unwrap())
        .collect();
    assert!(counts_asc.windows(2).all(|w| w[0] <= w[1]));

    let r_terms = search(
        "\"clip_grad_norm_\" in function | terms top:5",
        &opts(root, IndexMode::Off),
    );
    assert_eq!(r_terms.status, Status::Complete);
    assert!(!r_terms.terms.is_empty() && r_terms.terms.len() <= 5);
    assert!(r_terms.terms.iter().all(|t| t.term != "clip_grad_norm_"));

    let r_budget = search(
        "\"optimizer_hit_\" in line | budget tokens:60",
        &opts(root, IndexMode::Off),
    );
    assert!(r_budget.truncated);
    assert!(r_budget.returned < r_budget.matched);
    let b = r_budget.budget.as_ref().unwrap();
    assert_eq!(b.limit_tokens, 60);
    assert!(b.used_tokens <= 60);
    assert!(b.estimated);

    let r_lim = search(
        "\"optimizer_hit_\" in line | limit 5",
        &opts(root, IndexMode::Off),
    );
    assert_eq!(r_lim.returned, 5);
    assert_eq!(r_lim.matched, 400);
    assert!(r_lim.truncated);
    assert!(!r_lim.matched_is_lower_bound);

    let many = TempDir::new().unwrap();
    for i in 0..300 {
        fs::write(many.path().join(format!("f_{i:03}.txt")), "needle\n").unwrap();
    }
    let r_early = search("\"needle\" | limit 5", &opts(many.path(), IndexMode::Off));
    assert_eq!(r_early.returned, 5);
    assert!(r_early.truncated);
    assert!(r_early.matched_is_lower_bound);
}

#[test]
fn index_equals_scan_oracle_equivalence() {
    let dir = build_rich_corpus();
    let root = dir.path();
    let idx_dir = index::default_dir(root);

    let queries = [
        "\"optimizer\"",
        "\"optimizer\" in line",
        "\"optimizer\" in block",
        "\"AdamW\"",
        "\"kelvin\"",
        "\"학습률\"",
        "\"ab\"",
        "\"z\"",
        "\"OOM\"",
        "\"alpha\" AND \"beta\" in block",
        "\"alpha\" AND \"beta\" in line",
        "(\"OOM\" OR \"AdamW\") NOT \"CPU\"",
        "(\"OOM\" OR \"AdamW\") NOT \"CPU\" in line",
        "NOT \"absent_term_xyz\"",
        "NOT \"absent_term_xyz\" in line",
        "near(\"gradient\", \"clip\", 10)",
        "before(\"gradient\", \"clip\", 10)",
        "near(\"gradient\", \"clip\", 10) in line",
        "fuzzy(\"accumulation\", 2)",
        "fuzzy(\"transmute_ok\", 1)",
        "fuzzy(\"banana\", 1)",
        "fuzzy(\"aaaaa\", 1)",
        "fuzzy(\"\u{212A}elvin\", 1)",
        "fuzzy(\"학습율\", 1)",
        "regex(\"max_grad_norm.*1\\\\.0\")",
        "regex(\"alpha|AdamW\") in line",
        "regex(\"alpha\\nbeta\") in block",
        "\"opt\" in function | expand parent",
        "\"return\" in function | sort count",
        "symbol(\"train_step\")",
        "symbol:EngineConfig",
        "symbol(\"runStep\") AND \"x + 1\" in function",
        "\"optimizer\" in type:(code, config) | files",
        "\"optimizer\" in path:\"src/**\" | count",
        "\"optimizer\" | terms top:5",
        "\"optimizer_hit_\" in line | group file",
        "\"optimizer_hit_\" in line | budget tokens:80",
        "\"optimizer_hit_\" in line | limit 3",
    ];

    let rep = index::rebuild(root, &idx_dir, &WalkOpts::default()).unwrap();
    assert!(rep.added >= 9);

    for q in queries {
        let scan = search(q, &opts(root, IndexMode::Off));
        let auto = search(q, &opts(root, IndexMode::Auto));
        let trust = search(q, &opts(root, IndexMode::Trust));
        assert_eq!(auto.mode, "index", "expected index mode for `{q}`");
        assert_eq!(
            trust.mode, "index_trusted",
            "expected index_trusted mode for `{q}`"
        );
        assert_eq_responses(&scan, &auto, &format!("scan vs auto ({q})"));
        assert_eq_responses(&scan, &trust, &format!("scan vs trust ({q})"));
    }

    // Long non-matching ASCII literal & overlapping literal match
    let r_miss = search(
        "\"definitely_not_in_any_crate_xyz\"",
        &opts(root, IndexMode::Trust),
    );
    assert_eq!(r_miss.status, Status::Empty);
    assert_eq!(r_miss.stats.index_candidates, Some(0));

    let ex = explain("\"clip_grad_norm_\"", &opts(root, IndexMode::Trust));
    assert!(ex.grams_available.unwrap_or(0) > 0);
    assert!(ex.grams_decoded.unwrap_or(0) > 0);
}

#[cfg(feature = "adapters")]
fn make_zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zw = zip::ZipWriter::new(&mut buf);
        let opt = zip::write::SimpleFileOptions::default();
        for (name, data) in entries {
            zw.start_file(name, opt).unwrap();
            zw.write_all(data).unwrap();
        }
        zw.finish().unwrap();
    }
    buf.into_inner()
}

#[cfg(feature = "adapters")]
fn make_minimal_pdf(text: &str) -> Vec<u8> {
    let stream = format!("BT\n/F1 12 Tf\n72 720 Td\n({text}) Tj\nET\n");
    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.4\n");
    let mut offs = Vec::new();
    let objs = [
        "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".to_string(),
        "2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n".to_string(),
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>\nendobj\n".to_string(),
        format!("4 0 obj\n<< /Length {} >>\nstream\n{stream}endstream\nendobj\n", stream.len()),
        "5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n".to_string(),
    ];
    for o in &objs {
        offs.push(pdf.len());
        pdf.extend_from_slice(o.as_bytes());
    }
    let xref_off = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for off in offs {
        pdf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_off}\n%%EOF\n",
            objs.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

#[cfg(feature = "adapters")]
fn make_minimal_xlsx() -> Vec<u8> {
    let ct = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#;
    let rels = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
    let wb_rels = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#;
    let wb = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Metrics" sheetId="1" r:id="rId1"/></sheets></workbook>"#;
    let sheet = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>model</t></is></c><c r="B1" t="inlineStr"><is><t>optimizer</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>weft_xlsx_probe</t></is></c><c r="B2" t="inlineStr"><is><t>AdamW</t></is></c></row></sheetData></worksheet>"#;
    make_zip_bytes(&[
        ("[Content_Types].xml", ct),
        ("_rels/.rels", rels),
        ("xl/_rels/workbook.xml.rels", wb_rels),
        ("xl/workbook.xml", wb),
        ("xl/worksheets/sheet1.xml", sheet),
    ])
}

#[cfg(feature = "adapters")]
fn make_minimal_hwp(text: &str) -> Vec<u8> {
    use flate2::write::DeflateEncoder;
    use flate2::Compression;
    use std::io::Write;

    let mut hdr = vec![0u8; 256];
    hdr[..17].copy_from_slice(b"HWP Document File");
    hdr[36..40].copy_from_slice(&1u32.to_le_bytes()); // compressed = bit 0

    let mut payload = Vec::new();
    for u in text.encode_utf16() {
        payload.extend_from_slice(&u.to_le_bytes());
    }
    let rec_hdr: u32 = 67 | ((payload.len() as u32) << 20);
    let mut section_raw = Vec::new();
    section_raw.extend_from_slice(&rec_hdr.to_le_bytes());
    section_raw.extend_from_slice(&payload);

    let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
    enc.write_all(&section_raw).unwrap();
    let compressed_section = enc.finish().unwrap();

    let cur = std::io::Cursor::new(Vec::new());
    let mut cf = cfb::CompoundFile::create(cur).unwrap();
    cf.create_stream("FileHeader")
        .unwrap()
        .write_all(&hdr)
        .unwrap();
    cf.create_storage("BodyText").unwrap();
    cf.create_stream("BodyText/Section0")
        .unwrap()
        .write_all(&compressed_section)
        .unwrap();
    cf.into_inner().into_inner()
}

#[cfg(feature = "adapters")]
#[test]
fn adapters_extract_and_index_oracle_equivalence() {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    let dir = TempDir::new().unwrap();
    let r = dir.path();

    // DOCX
    fs::write(
        r.join("spec.docx"),
        make_zip_bytes(&[(
            "word/document.xml",
            br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>docx_probe gradient clipping</w:t></w:r></w:p></w:body></w:document>"#,
        )]),
    )
    .unwrap();

    // PPTX
    fs::write(
        r.join("deck.pptx"),
        make_zip_bytes(&[(
            "ppt/slides/slide1.xml",
            br#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>pptx_probe architecture</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#,
        )]),
    )
    .unwrap();

    // ODT
    fs::write(
        r.join("memo.odt"),
        make_zip_bytes(&[(
            "content.xml",
            br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><office:body><office:text><text:p>odt_probe meeting notes</text:p></office:text></office:body></office:document-content>"#,
        )]),
    )
    .unwrap();

    // HWPX (Korean)
    fs::write(
        r.join("report.hwpx"),
        make_zip_bytes(&[(
            "Contents/section0.xml",
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><hs:sec xmlns:hs=\"http://www.hancom.co.kr/hwpml/2011/section\" xmlns:hp=\"http://www.hancom.co.kr/hwpml/2011/paragraph\"><hp:p><hp:run><hp:t>hwpx_probe 학습률 설정 보고서</hp:t></hp:run></hp:p></hs:sec>".as_bytes(),
        )]),
    )
    .unwrap();

    // EPUB
    fs::write(
        r.join("book.epub"),
        make_zip_bytes(&[(
            "OEBPS/ch1.xhtml",
            br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><h1>Chapter 1</h1><p>epub_probe chapter body</p><script>ignored_script</script></body></html>"#,
        )]),
    )
    .unwrap();

    // XLSX
    fs::write(r.join("metrics.xlsx"), make_minimal_xlsx()).unwrap();

    // PDF + corrupt PDF
    fs::write(
        r.join("paper.pdf"),
        make_minimal_pdf("pdf_probe gradient clipping"),
    )
    .unwrap();
    fs::write(r.join("broken.pdf"), b"%PDF-1.4\nnot valid pdf").unwrap();

    // GZ
    {
        let mut enc = GzEncoder::new(Vec::new(), Compression::default());
        enc.write_all(b"gz_probe log line 1\ngz_probe log line 2\n")
            .unwrap();
        fs::write(r.join("app.log.gz"), enc.finish().unwrap()).unwrap();
    }

    // TAR + TAR.GZ
    let tar_bytes = {
        let mut tb = tar::Builder::new(Vec::new());
        let payload = b"tar_probe inside tarball\n";
        let mut hdr = tar::Header::new_gnu();
        hdr.set_size(payload.len() as u64);
        hdr.set_mode(0o644);
        hdr.set_cksum();
        tb.append_data(&mut hdr, "inner/readme.txt", &payload[..])
            .unwrap();
        tb.into_inner().unwrap()
    };
    fs::write(r.join("bundle.tar"), &tar_bytes).unwrap();
    {
        let mut enc = GzEncoder::new(Vec::new(), Compression::default());
        enc.write_all(&tar_bytes).unwrap();
        fs::write(r.join("bundle.tar.gz"), enc.finish().unwrap()).unwrap();
    }

    // ZIP (with mixed text + binary) + corrupt ZIP
    fs::write(
        r.join("archive.zip"),
        make_zip_bytes(&[
            ("notes.txt", b"zip_probe inside archive\n"),
            ("bin.dat", b"\x00\x01\x02skip_binary_entry"),
        ]),
    )
    .unwrap();
    fs::write(r.join("broken.zip"), b"PK\x03\x04corrupt").unwrap();

    // Legacy binary HWP (5.0 OLE/CFB)
    fs::write(
        r.join("legacy.hwp"),
        make_minimal_hwp("hwp_legacy_probe 한글 구형 문서"),
    )
    .unwrap();

    // PNG image (skipped when ocr=false; graceful when ocr=true)
    fs::write(r.join("scan.png"), b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR").unwrap();

    let rep = index::sync(r, &index::default_dir(r), &WalkOpts::default()).unwrap();
    assert!(rep.added >= 12);

    let checks = [
        ("\"docx_probe\" in type:document", "spec.docx"),
        ("\"pptx_probe\" in type:document", "deck.pptx"),
        ("\"odt_probe\" in type:document", "memo.odt"),
        (
            "\"학습률\" AND \"hwpx_probe\" in type:document",
            "report.hwpx",
        ),
        (
            "\"hwp_legacy_probe\" AND \"구형\" in type:document",
            "legacy.hwp",
        ),
        ("\"epub_probe\" in type:document", "book.epub"),
        ("\"pdf_probe\" in type:document", "paper.pdf"),
        (
            "\"weft_xlsx_probe\" AND \"AdamW\" in type:spreadsheet",
            "metrics.xlsx",
        ),
        ("\"gz_probe\" in type:archive", "app.log.gz"),
        ("\"zip_probe\" in type:archive", "archive.zip"),
        ("\"tar_probe\" in ext:tar", "bundle.tar"),
        ("\"tar_probe\" in path:\"bundle.tar.gz\"", "bundle.tar.gz"),
    ];
    for (q, expected_path) in checks {
        let scan = search(q, &opts(r, IndexMode::Off));
        let auto = search(q, &opts(r, IndexMode::Auto));
        let trust = search(q, &opts(r, IndexMode::Trust));
        assert_eq!(scan.status, Status::Complete, "expected match for `{q}`");
        assert_eq!(scan.results[0].path, expected_path, "wrong path for `{q}`");
        assert_eq_responses(&scan, &auto, &format!("adapter scan vs auto ({q})"));
        assert_eq_responses(&scan, &trust, &format!("adapter scan vs trust ({q})"));
    }

    // Script tag in EPUB and binary entry inside ZIP must be ignored.
    assert_eq!(
        search("\"ignored_script\"", &opts(r, IndexMode::Off)).status,
        Status::Empty
    );
    assert_eq!(
        search("\"skip_binary_entry\"", &opts(r, IndexMode::Off)).status,
        Status::Empty
    );
    // PNG skipped when ocr=false and does not panic when ocr=true.
    assert_eq!(
        search("\"IHDR\" in type:image", &opts(r, IndexMode::Off)).status,
        Status::Empty
    );
    let mut o_ocr = opts(r, IndexMode::Off);
    o_ocr.ocr = true;
    let _ = search("\"anything\" in type:image", &o_ocr);
}

#[test]
fn line_fast_path_matches_full_path_and_context() {
    let dir = TempDir::new().unwrap();
    let r = dir.path();
    fs::write(
        r.join("a.txt"),
        "hit first\nline 2\nline 3\nhit mid\nline 5\nhit last_no_nl",
    )
    .unwrap();
    fs::write(r.join("b.txt"), "one\nhit middle\nthree\n").unwrap();

    for q in [
        "\"hit\" in line | expand context:1",
        "\"hit\" in line | expand context:2",
        "\"hit\" in line | count",
        "\"hit\" in line | expand parent",
    ] {
        let fast = search(q, &opts(r, IndexMode::Off));
        assert_eq!(fast.status, Status::Complete);
        // Compare against group file (which uses LineMode::Full) for snippet/context parity.
        if q.contains("expand context:") {
            let grouped = search(&format!("{q} | group file"), &opts(r, IndexMode::Off));
            let fast_lines_a: Vec<_> = fast
                .results
                .iter()
                .filter(|it| it.path == "a.txt")
                .flat_map(|it| it.lines.clone())
                .collect();
            let mut dedup_fast = fast_lines_a.clone();
            dedup_fast.sort_by_key(|l| (l.line, l.context));
            dedup_fast.dedup_by_key(|l| l.line);
            assert_eq!(
                dedup_fast.len(),
                grouped.results[0].lines.len(),
                "context parity on a.txt for `{q}`"
            );
            for (a, b) in dedup_fast.iter().zip(&grouped.results[0].lines) {
                assert_eq!(a.line, b.line);
                assert_eq!(a.text, b.text);
            }
        }
    }
    let cnt = search("\"hit\" in line | count", &opts(r, IndexMode::Off));
    assert_eq!(cnt.results[0].count, Some(3));
    assert_eq!(cnt.results[1].count, Some(1));
}

#[test]
fn batch_and_explain_match_individual_searches() {
    let dir = build_rich_corpus();
    let root = dir.path();
    let idx_dir = index::default_dir(root);
    index::sync(root, &idx_dir, &WalkOpts::default()).unwrap();

    let pairs = vec![
        ("q_lit".into(), "\"optimizer\" in line | limit 3".into()),
        ("q_fn".into(), "\"opt\" in function".into()),
        ("q_sym".into(), "symbol(\"EngineConfig\")".into()),
        ("q_terms".into(), "\"AdamW\" | terms top:5".into()),
    ];
    for mode in [IndexMode::Off, IndexMode::Auto, IndexMode::Trust] {
        let o = opts(root, mode);
        let batch = search_batch(&pairs, &o);
        assert_eq!(batch.results.len(), pairs.len());
        for (item, (name, q)) in batch.results.iter().zip(&pairs) {
            assert_eq!(&item.name, name);
            let single = search(q, &o);
            assert_eq_responses(
                &item.response,
                &single,
                &format!("batch vs single ({name}, {mode:?})"),
            );
        }
    }

    let ex = explain("fuzzy(\"accumulation\", 1)", &opts(root, IndexMode::Trust));
    assert_eq!(ex.status, Status::Complete);
    assert!(ex.index_exists);
    assert!(ex.trigram_req.as_deref().unwrap().starts_with("AtLeast("));
    assert!(ex.index_candidates.unwrap() < ex.files_considered);
}

#[test]
fn incremental_sync_freshness_and_segment_gc() {
    let dir = build_rich_corpus();
    let root = dir.path();
    let idx_dir = index::default_dir(root);
    // Files written within the racy window of a sync are re-indexed on the next one.
    std::thread::sleep(Duration::from_millis(250));

    let rep1 = index::sync(root, &idx_dir, &WalkOpts::default()).unwrap();
    assert_eq!(rep1.segments, 1);

    fs::write(
        root.join("data/config.json"),
        "{\"optimizer\": \"Lion\", \"brand_new_token\": 42}\n",
    )
    .unwrap();
    fs::write(root.join("src/added.rs"), "fn brand_new_token() {}\n").unwrap();
    fs::remove_file(root.join("docs/latin.txt")).unwrap();

    for q in [
        "\"Lion\"",
        "\"brand_new_token\"",
        "\"AdamW\"",
        "\"optimizer_nonutf8\"",
    ] {
        let scan = search(q, &opts(root, IndexMode::Off));
        let auto = search(q, &opts(root, IndexMode::Auto));
        assert_eq_responses(&scan, &auto, &format!("stale auto vs scan ({q})"));
        assert!(auto.stats.stale_files.unwrap_or(0) >= 2);
    }

    let rep2 = index::sync(root, &idx_dir, &WalkOpts::default()).unwrap();
    assert_eq!(rep2.added, 2);
    assert_eq!(rep2.removed, 1);
    assert!(rep2.unchanged >= 4);
    assert_eq!(rep2.segments, 2);

    for q in [
        "\"Lion\"",
        "\"brand_new_token\"",
        "\"AdamW\"",
        "\"optimizer_nonutf8\"",
    ] {
        let scan = search(q, &opts(root, IndexMode::Off));
        let trust = search(q, &opts(root, IndexMode::Trust));
        assert_eq_responses(&scan, &trust, &format!("synced trust vs scan ({q})"));
    }

    fs::remove_file(root.join("data/config.json")).unwrap();
    fs::remove_file(root.join("src/added.rs")).unwrap();
    let rep3 = index::sync(root, &idx_dir, &WalkOpts::default()).unwrap();
    assert_eq!(rep3.removed, 2);
    assert_eq!(rep3.segments, 1);
    let ix = Index::open(&idx_dir).unwrap().unwrap();
    assert_eq!(ix.segs.len(), 1);
    assert!(!idx_dir.join("seg-000002.wseg").exists());

    let rep4 = index::rebuild(root, &idx_dir, &WalkOpts::default()).unwrap();
    assert_eq!(rep4.segments, 1);
    let ix2 = Index::open(&idx_dir).unwrap().unwrap();
    assert_eq!(ix2.dead_count(), 0);
}

#[test]
fn fd_filters_tree_and_trust_index() {
    let dir = build_rich_corpus();
    let root = dir.path();

    // Mark docs/empty.txt as very old (year 2001) so `--changed-within` distinguishes it cleanly.
    let old_file = fs::File::options()
        .write(true)
        .open(root.join("docs/empty.txt"))
        .unwrap();
    old_file
        .set_modified(UNIX_EPOCH + Duration::from_secs(1_000_000_000))
        .unwrap();

    // 1. Default respects .gitignore & excludes hidden .git
    let base = find_files(&FdOptions::new(root));
    assert_eq!(base.status, Status::Complete);
    assert!(base.entries.iter().all(|e| !e.path.contains("ignored_dir")));
    assert!(base.entries.iter().all(|e| e.path != "skip.tmp"));
    assert!(base.entries.iter().all(|e| !e.path.starts_with(".git")));
    assert!(base
        .entries
        .iter()
        .any(|e| e.path == "src" && e.kind == "dir"));
    assert!(base
        .entries
        .iter()
        .any(|e| e.path == "src/sub/engine.rs" && e.kind == "file"));

    // 2. Glob & Regex & extension & FileType filters
    let mut glob_rs = FdOptions::new(root);
    glob_rs.pattern = Some("*.rs".into());
    glob_rs.pattern_mode = PatternMode::Glob;
    let r_glob = find_files(&glob_rs);
    assert_eq!(
        r_glob
            .entries
            .iter()
            .map(|e| e.path.as_str())
            .collect::<Vec<_>>(),
        vec!["src/sub/engine.rs"]
    );

    let mut rx_py_go = FdOptions::new(root);
    rx_py_go.pattern = Some(r"^src/.*\.(py|go)$".into());
    rx_py_go.pattern_mode = PatternMode::Regex;
    let r_rx = find_files(&rx_py_go);
    assert_eq!(
        r_rx.entries
            .iter()
            .map(|e| e.path.as_str())
            .collect::<Vec<_>>(),
        vec!["src/train.py", "src/worker.go"]
    );

    let mut tc_doc = FdOptions::new(root);
    tc_doc.file_types = vec![FileType::Document];
    let r_doc = find_files(&tc_doc);
    let doc_paths: Vec<&str> = r_doc.entries.iter().map(|e| e.path.as_str()).collect();
    assert!(doc_paths.contains(&"docs/notes.md"));
    assert!(doc_paths.contains(&"docs/big.txt"));
    assert!(!doc_paths.contains(&"src/train.py"));

    // 3. Max-depth & size & changed-within
    let mut depth1 = FdOptions::new(root);
    depth1.max_depth = Some(1);
    let r_d1 = find_files(&depth1);
    assert!(r_d1.entries.iter().all(|e| !e.path.contains('/')));

    let mut big_only = FdOptions::new(root);
    big_only.min_size = Some(parse_size("5k").unwrap());
    let r_big = find_files(&big_only);
    assert_eq!(
        r_big
            .entries
            .iter()
            .map(|e| e.path.as_str())
            .collect::<Vec<_>>(),
        vec!["docs/big.txt"]
    );

    let mut recent = FdOptions::new(root);
    recent.include_files = true;
    recent.changed_within_ns = Some(parse_duration("7d").unwrap());
    let r_rec = find_files(&recent);
    assert!(r_rec.entries.iter().any(|e| e.path == "src/train.py"));
    assert!(!r_rec.entries.iter().any(|e| e.path == "docs/empty.txt"));

    // 4. Tree rendering & Trust-index parity (including ancestor discovery on subdir)
    let tree = render_tree(root, &base.entries);
    assert!(tree.contains("├── ") || tree.contains("└── "));
    assert!(tree.contains("src/\n"));

    index::rebuild(root, &index::default_dir(root), &WalkOpts::default()).unwrap();

    let mut trust_opts = FdOptions::new(root);
    trust_opts.index = IndexMode::Trust;
    trust_opts.include_files = true;
    let r_trust = find_files(&trust_opts);
    assert_eq!(r_trust.mode, "index_trusted");
    let mut scan_files_opts = FdOptions::new(root);
    scan_files_opts.include_files = true;
    let r_scan_files = find_files(&scan_files_opts);
    assert_eq!(r_trust.entries, r_scan_files.entries);

    let mut sub_trust = FdOptions::new(root.join("src"));
    sub_trust.index = IndexMode::Trust;
    sub_trust.include_files = true;
    let r_sub_trust = find_files(&sub_trust);
    assert_eq!(r_sub_trust.mode, "index_trusted");
    let mut sub_scan = FdOptions::new(root.join("src"));
    sub_scan.include_files = true;
    assert_eq!(r_sub_trust.entries, find_files(&sub_scan).entries);
}

#[cfg(feature = "ast")]
#[test]
fn ast_search_multi_lang_and_index_equivalence() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/main.rs"),
        r#"
fn demo(a: i32, b: i32, opt: Option<i32>) -> i32 {
    // comment.unwrap() must NOT match
    let s = "string.unwrap() must NOT match";
    let _x = opt.unwrap();
    if a == a {
        panic!("same", a);
    }
    if a == b {
        panic!();
    }
    0
}
"#,
    )
    .unwrap();
    // Filler Rust files without `unwrap` to confirm trigram narrowing skips them.
    for i in 0..12 {
        fs::write(
            root.join(format!("src/helper_{i}.rs")),
            format!("pub fn helper_{i}(v: i32) -> i32 {{ v + {i} }}\n"),
        )
        .unwrap();
    }
    fs::write(
        root.join("src/script.py"),
        "def run(x):\n    # clip_grad_norm_(x)\n    y = clip_grad_norm_(x, 1.0)\n    return y\n",
    )
    .unwrap();
    fs::write(
        root.join("src/app.ts"),
        "export function logIt(x: number) {\n  // console.log(x)\n  console.log('val', x);\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("src/worker.go"),
        "package main\nimport \"fmt\"\nfunc fail(err error) error {\n\treturn fmt.Errorf(\"wrap: %w\", err)\n}\n",
    )
    .unwrap();

    // 1. Rust AST search: ignores comments/strings, enforces repeated $X == $X, handles $$$ARGS
    let mut o_unwrap = AstOptions::new("$X.unwrap()", root);
    o_unwrap.lang = Some("rs".into());
    let res_scan = search_ast(&o_unwrap);
    assert_eq!(res_scan.status, Status::Complete);
    assert_eq!(res_scan.matched, 1);
    assert_eq!(
        res_scan.matches[0].captures.get("X").map(String::as_str),
        Some("opt")
    );
    assert!(res_scan
        .stats
        .trigram_literals
        .contains(&"unwrap".to_string()));

    let mut o_eq = AstOptions::new("$X == $X", root);
    o_eq.lang = Some("rs".into());
    let res_eq = search_ast(&o_eq);
    assert_eq!(res_eq.matched, 1);
    assert_eq!(res_eq.matches[0].text, "a == a");

    let mut o_panic = AstOptions::new("panic!($$$ARGS)", root);
    o_panic.lang = Some("rs".into());
    let res_panic = search_ast(&o_panic);
    assert_eq!(res_panic.matched, 2);
    assert_eq!(
        res_panic.matches[0]
            .captures
            .get("ARGS")
            .map(String::as_str),
        Some("\"same\", a")
    );
    assert_eq!(
        res_panic.matches[1]
            .captures
            .get("ARGS")
            .map(String::as_str),
        Some("")
    );

    // 2. Python / TypeScript / Go
    let mut o_py = AstOptions::new("clip_grad_norm_($$$ARGS)", root);
    o_py.lang = Some("py".into());
    let res_py = search_ast(&o_py);
    assert_eq!(res_py.matched, 1);
    assert_eq!(
        res_py.matches[0].captures.get("ARGS").map(String::as_str),
        Some("x, 1.0")
    );

    let mut o_ts = AstOptions::new("$OBJ.log($$$ARGS)", root);
    o_ts.lang = Some("ts".into());
    let res_ts = search_ast(&o_ts);
    assert_eq!(res_ts.matched, 1);
    assert_eq!(
        res_ts.matches[0].captures.get("OBJ").map(String::as_str),
        Some("console")
    );
    assert_eq!(
        res_ts.matches[0].captures.get("ARGS").map(String::as_str),
        Some("'val', x")
    );

    let mut o_go = AstOptions::new("fmt.Errorf($$$ARGS)", root);
    o_go.lang = Some("go".into());
    let res_go = search_ast(&o_go);
    assert_eq!(res_go.matched, 1);
    assert_eq!(
        res_go.matches[0].captures.get("ARGS").map(String::as_str),
        Some("\"wrap: %w\", err")
    );

    // 3. Invalid pattern fails strictly
    let mut o_bad = AstOptions::new("fn (", root);
    o_bad.lang = Some("rs".into());
    assert_eq!(search_ast(&o_bad).status, Status::InvalidQuery);

    // 4. Lancet index acceleration + Scan == Auto == Trust + ancestor index discovery
    index::rebuild(root, &index::default_dir(root), &WalkOpts::default()).unwrap();

    let mut o_auto = o_unwrap.clone();
    o_auto.index = IndexMode::Auto;
    let res_auto = search_ast(&o_auto);
    assert_eq!(res_auto.mode, "index");
    assert_eq!(res_auto.matches, res_scan.matches);
    assert!(res_auto.stats.index_candidates.unwrap() < res_auto.stats.files_considered);
    assert_eq!(res_auto.stats.files_parsed, 1);

    let mut o_trust = o_unwrap.clone();
    o_trust.index = IndexMode::Trust;
    let res_trust = search_ast(&o_trust);
    assert_eq!(res_trust.mode, "index_trusted");
    assert_eq!(res_trust.matches, res_scan.matches);
    assert_eq!(res_trust.stats.files_parsed, 1);

    // Run directly on `src` subdir and still pick up root `.weft/`
    let mut o_sub = AstOptions::new("$X.unwrap()", root.join("src"));
    o_sub.lang = Some("rs".into());
    o_sub.index = IndexMode::Trust;
    let res_sub = search_ast(&o_sub);
    assert_eq!(res_sub.mode, "index_trusted");
    assert_eq!(res_sub.matches.len(), 1);
    assert_eq!(res_sub.matches[0].path, "main.rs");
}

#[cfg(feature = "ast")]
#[test]
fn ast_rewrite_dry_run_and_apply() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let rs_path = root.join("sample.rs");
    let orig = r#"fn work(a: Option<i32>, b: Option<Option<i32>>) -> Option<i32> {
    // a.unwrap() in comment
    let v = a.unwrap();
    let w = b.unwrap().unwrap();
    Some(v + w)
}
"#;
    fs::write(&rs_path, orig).unwrap();

    // 1. Dry-run produces diff without altering file on disk
    let mut dry = AstOptions::new("$X.unwrap()", root);
    dry.lang = Some("rs".into());
    dry.rewrite = Some("$X?".into());
    let r_dry = search_ast(&dry);
    assert_eq!(r_dry.status, Status::Complete);
    assert_eq!(r_dry.diffs.len(), 1);
    assert!(!r_dry.diffs[0].applied);
    assert_eq!(r_dry.diffs[0].replacements, 2);
    assert_eq!(r_dry.diffs[0].skipped_overlaps, 1);
    assert_eq!(r_dry.diffs[0].rev, lancet::edit::rev(orig.as_bytes()));
    assert!(r_dry.diffs[0].diff.contains("--- a/sample.rs"));
    assert!(r_dry.diffs[0].diff.contains("-    let v = a.unwrap();"));
    assert!(r_dry.diffs[0].diff.contains("+    let v = a?;"));
    assert_eq!(fs::read_to_string(&rs_path).unwrap(), orig);

    // 2. Apply writes changes cleanly while preserving comments
    let mut apply = dry.clone();
    apply.apply = true;
    let r_apply = search_ast(&apply);
    assert!(r_apply.diffs[0].applied);
    let updated = fs::read_to_string(&rs_path).unwrap();
    assert!(updated.contains("// a.unwrap() in comment"));
    assert!(updated.contains("let v = a?;"));
    assert!(updated.contains("let w = b.unwrap()?;"));

    // Second pass catches the remaining inner `b.unwrap()?` -> `b??`!
    let r_apply2 = search_ast(&apply);
    assert_eq!(r_apply2.matched, 1);
    assert_eq!(r_apply2.diffs[0].rev, lancet::edit::rev(updated.as_bytes()));
    assert!(fs::read_to_string(&rs_path)
        .unwrap()
        .contains("let w = b??;"));

    // Third pass has zero matches!
    assert_eq!(search_ast(&apply).status, Status::Empty);

    // 3. Safety rejections: unknown metavariable, --apply without --rewrite, --apply with --limit
    let mut bad_mv = AstOptions::new("$X.unwrap()", root);
    bad_mv.lang = Some("rs".into());
    bad_mv.rewrite = Some("$UNKNOWN?".into());
    assert_eq!(search_ast(&bad_mv).status, Status::InvalidQuery);

    let mut bad_no_rw = AstOptions::new("$X.unwrap()", root);
    bad_no_rw.lang = Some("rs".into());
    bad_no_rw.apply = true;
    assert_eq!(search_ast(&bad_no_rw).status, Status::InvalidQuery);

    let mut bad_lim = dry;
    bad_lim.apply = true;
    bad_lim.limit = Some(1);
    assert_eq!(search_ast(&bad_lim).status, Status::InvalidQuery);
}

#[test]
fn read_outline_and_refs_end_to_end() {
    let dir = build_rich_corpus();
    let root = dir.path();

    // Add a multi-symbol Rust file with comments/strings mentioning `EngineConfig` & `run`
    fs::write(
        root.join("src/caller.rs"),
        r#"/// Builds a default engine configuration and runs it.
pub fn invoke_engine() -> f64 {
    // EngineConfig in comment must NOT be counted as a ref
    let _note = "EngineConfig in string must NOT be counted as a ref";
    let cfg = EngineConfig { lr: 0.01 };
    run();
    cfg.lr
}
"#,
    )
    .unwrap();

    // Also add a ZIP archive with multiple inner files
    fs::write(
        root.join("data/bundle.zip"),
        make_zip_bytes(&[
            ("logs/node_0.log", b"line1_alpha\nline2_beta\nline3_gamma\n"),
            ("logs/node_1.log", b"other_entry\n"),
        ]),
    )
    .unwrap();

    // 1. `lancet read`: colon line-range shorthand + `--budget-tokens`
    let r_lines = read_file_range(&ReadOptions::new(format!(
        "{}:1-4",
        root.join("src/train.py").display()
    )));
    assert_eq!(r_lines.status, Status::Complete);
    assert_eq!(r_lines.start_line, 1);
    assert_eq!(r_lines.end_line, 4);
    assert_eq!(r_lines.returned_lines, 4);
    let train = fs::read(root.join("src/train.py")).unwrap();
    assert_eq!(r_lines.rev, Some(lancet::edit::rev(&train)));

    let mut r_budget_opts =
        ReadOptions::new(root.join("docs/big.txt").to_string_lossy().into_owned());
    r_budget_opts.budget_tokens = Some(60);
    let r_budget = read_file_range(&r_budget_opts);
    assert!(r_budget.truncated);
    assert!(r_budget.next_line.unwrap() > 1);

    // 2. `lancet read`: archive bang-path (`bundle.zip!node_0.log`)
    #[cfg(feature = "adapters")]
    {
        let bang = format!("{}!node_0.log:2-3", root.join("data/bundle.zip").display());
        let r_zip = read_file_range(&ReadOptions::new(bang));
        assert_eq!(r_zip.status, Status::Complete);
        assert_eq!(r_zip.returned_lines, 2);
        assert_eq!(r_zip.lines[0].text, "line2_beta");
        assert_eq!(r_zip.lines[1].text, "line3_gamma");
        assert_eq!(r_zip.rev, None, "archive entries can't be written back");
    }

    // 3. `lancet read --symbol` (includes doc comment above `invoke_engine`)
    #[cfg(feature = "ast")]
    {
        let mut sym_opts =
            ReadOptions::new(root.join("src/caller.rs").to_string_lossy().into_owned());
        sym_opts.symbol = Some("invoke_engine".into());
        let r_sym = read_file_range(&sym_opts);
        assert_eq!(r_sym.status, Status::Complete);
        assert_eq!(r_sym.start_line, 1);
        assert!(r_sym.lines[0].text.contains("Builds a default engine"));
        assert!(r_sym
            .lines
            .iter()
            .any(|l| l.text.contains("pub fn invoke_engine()")));
    }

    // 4. `lancet outline`: Markdown headings, code skeleton, and ZIP container entries
    let out_md = build_outline(&OutlineOptions::new(root.join("docs/notes.md")));
    assert_eq!(out_md.status, Status::Complete);
    assert_eq!(out_md.files[0].items[0].kind, "h1");
    assert_eq!(out_md.files[0].items[0].name, "Training Notes");

    #[cfg(feature = "ast")]
    {
        let out_rs = build_outline(&OutlineOptions::new(root.join("src/sub/engine.rs")));
        assert_eq!(out_rs.status, Status::Complete);
        let names: Vec<&str> = out_rs.files[0]
            .items
            .iter()
            .map(|i| i.name.as_str())
            .collect();
        assert!(names.contains(&"EngineConfig"));
        assert!(names.contains(&"run"));
        assert!(names.contains(&"helper_transmute"));
    }

    #[cfg(feature = "adapters")]
    {
        let out_zip = build_outline(&OutlineOptions::new(root.join("data/bundle.zip")));
        assert_eq!(out_zip.status, Status::Complete);
        assert_eq!(out_zip.files[0].items.len(), 2);
        assert_eq!(out_zip.files[0].items[0].kind, "entry");
    }

    // 5. `lancet refs`: AST definition + reference split, ignoring comments/strings, Scan == Auto == Trust
    #[cfg(feature = "ast")]
    {
        index::rebuild(root, &index::default_dir(root), &WalkOpts::default()).unwrap();

        let mut ro_scan = RefsOptions::new("EngineConfig", root);
        ro_scan.index = IndexMode::Off;
        let refs_scan = find_refs(&ro_scan);
        assert_eq!(refs_scan.status, Status::Complete);
        assert_eq!(refs_scan.definitions.len(), 1);
        assert_eq!(refs_scan.definitions[0].path, "src/sub/engine.rs");
        assert_eq!(refs_scan.definitions[0].kind, "struct");
        assert_eq!(refs_scan.references.len(), 1);
        assert_eq!(refs_scan.references[0].path, "src/caller.rs");
        assert_eq!(
            refs_scan.references[0].enclosing.as_deref(),
            Some("invoke_engine")
        );

        let mut ro_auto = RefsOptions::new("EngineConfig", root);
        ro_auto.index = IndexMode::Auto;
        let refs_auto = find_refs(&ro_auto);
        assert_eq!(refs_auto.mode, "index");
        assert_eq!(refs_auto.definitions, refs_scan.definitions);
        assert_eq!(refs_auto.references, refs_scan.references);

        let mut ro_trust = RefsOptions::new("EngineConfig", root);
        ro_trust.index = IndexMode::Trust;
        let refs_trust = find_refs(&ro_trust);
        assert_eq!(refs_trust.mode, "index_trusted");
        assert_eq!(refs_trust.definitions, refs_scan.definitions);
        assert_eq!(refs_trust.references, refs_scan.references);
    }
}

#[test]
fn lsm_segment_compaction_and_zero_alloc_cursor_equivalence() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let idx_dir = index::default_dir(root);

    // Seed 50 files
    fs::create_dir_all(root.join("pkg")).unwrap();
    for i in 0..50 {
        fs::write(
            root.join(format!("pkg/mod_{i:02}.rs")),
            format!("pub fn worker_{i:02}() {{ let token_common = {i}; }}\n"),
        )
        .unwrap();
    }
    let r0 = index::sync(root, &idx_dir, &WalkOpts::default()).unwrap();
    assert_eq!(r0.segments, 1);

    // Perform 8 incremental mutations (exceeds COMPACT_MAX_SEGMENTS = 6 -> triggers automatic LSM merge)
    for round in 1..=8 {
        let target = round * 3;
        fs::write(
            root.join(format!("pkg/mod_{target:02}.rs")),
            format!("pub fn worker_{target:02}() {{ let mutated_round_{round} = 999; }}\n"),
        )
        .unwrap();
        fs::write(
            root.join(format!("pkg/extra_{round}.rs")),
            format!("pub fn extra_fn_{round}() {{ let token_common = {round}; }}\n"),
        )
        .unwrap();
        let _ = index::sync(root, &idx_dir, &WalkOpts::default()).unwrap();
    }

    let ix_auto = Index::open(&idx_dir).unwrap().unwrap();
    assert!(
        ix_auto.segs.len() <= 6,
        "expected tiered LSM compaction to bound segments <= 6, got {}",
        ix_auto.segs.len()
    );

    // Explicit `index::compact` collapses everything into 1 tombstoneless segment
    let _ = index::compact(&idx_dir).unwrap();
    let ix_clean = Index::open(&idx_dir).unwrap().unwrap();
    assert_eq!(ix_clean.segs.len(), 1);
    assert_eq!(ix_clean.dead_count(), 0);

    // Verify exact Scan == Auto == Trust parity across original, mutated, and added tokens
    for q in [
        "\"token_common\"",
        "\"mutated_round_1\"",
        "\"mutated_round_8\"",
        "\"extra_fn_5\"",
        "\"worker_49\"",
    ] {
        let s_scan = search(q, &opts(root, IndexMode::Off));
        let s_auto = search(q, &opts(root, IndexMode::Auto));
        let s_trust = search(q, &opts(root, IndexMode::Trust));
        assert_eq_responses(&s_scan, &s_auto, &format!("compacted auto vs scan ({q})"));
        assert_eq_responses(&s_scan, &s_trust, &format!("compacted trust vs scan ({q})"));
    }
}

#[cfg(feature = "ast")]
#[test]
fn polyglot_24_languages_ast_and_symbols() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    let samples: &[(&str, &str, &str, &str, Option<&str>)] = &[
        (
            "kern.c",
            "c",
            "static int alloc_buf(size_t n) {\n    // kmalloc(n, 0)\n    void *p = kmalloc(n, GFP_KERNEL);\n    return p != 0;\n}\n",
            "kmalloc($SZ, $FLAGS)",
            Some("alloc_buf"),
        ),
        (
            "engine.cpp",
            "cpp",
            "class TensorOp {\npublic:\n    int compute(int x) { return std::max(x, 0); }\n};\n",
            "std::max($A, $B)",
            Some("TensorOp"),
        ),
        (
            "main.zig",
            "zig",
            "const std = @import(\"std\");\npub fn addValues(a: i32, b: i32) i32 {\n    return @max(a, b);\n}\n",
            "@max($A, $B)",
            Some("addValues"),
        ),
        (
            "Service.java",
            "java",
            "public class Service {\n    public String greet(String s) {\n        return String.valueOf(s);\n    }\n}\n",
            "String.valueOf($X)",
            Some("Service"),
        ),
        (
            "Handler.cs",
            "cs",
            "public class Handler {\n    public int ParseId(string s) {\n        return int.Parse(s);\n    }\n}\n",
            "int.Parse($X)",
            Some("Handler"),
        ),
        (
            "Repo.kt",
            "kt",
            "class Repo {\n    fun loadItem(id: String): Int {\n        return listOf(1, 2, 3).size\n    }\n}\n",
            "listOf($$$ITEMS)",
            Some("loadItem"),
        ),
        (
            "Pipe.scala",
            "scala",
            "object Pipe {\n  def runJob(x: Int): Option[Int] = Some(x)\n}\n",
            "Some($X)",
            Some("Pipe"),
        ),
        (
            "View.swift",
            "swift",
            "struct CardView {\n    func renderTitle(s: String) -> Int {\n        return max(1, 2)\n    }\n}\n",
            "max($A, $B)",
            Some("renderTitle"),
        ),
        (
            "worker.rb",
            "rb",
            "class JobWorker\n  def perform(id)\n    Rails.logger.info(id)\n  end\nend\n",
            "Rails.logger.info($MSG)",
            Some("JobWorker"),
        ),
        (
            "Controller.php",
            "php",
            "<?php\nclass UserController {\n    public function store($req) {\n        return array_merge($req, []);\n    }\n}\n",
            "array_merge($A, $B)",
            Some("UserController"),
        ),
        (
            "deploy.sh",
            "bash",
            "deploy_app() {\n  kubectl apply -f app.yaml\n}\n",
            "kubectl apply -f $FILE",
            Some("deploy_app"),
        ),
        (
            "plugin.lua",
            "lua",
            "function setup_plugin(opts)\n  return string.format(\"%s\", opts)\nend\n",
            "string.format($FMT, $ARG)",
            Some("setup_plugin"),
        ),
        (
            "main.tf",
            "hcl",
            "resource \"aws_s3_bucket\" \"logs\" {\n  bucket = \"my-logs\"\n}\n",
            "bucket = $VAL",
            Some("resource.aws_s3_bucket.logs"),
        ),
        (
            "styles.css",
            "css",
            ".btn-primary {\n  color: #ff0000;\n}\n",
            "color: $HEX;",
            Some(".btn-primary"),
        ),
        (
            "cfg.json",
            "json",
            "{\n  \"timeout\": 30,\n  \"retries\": 3\n}\n",
            "\"timeout\": $VAL",
            Some("timeout"),
        ),
    ];

    for &(fname, _lang, content, _pat, _sym) in samples {
        fs::write(root.join(fname), content).unwrap();
    }

    index::rebuild(root, &index::default_dir(root), &WalkOpts::default()).unwrap();

    for &(fname, lang, _content, pat, expected_sym) in samples {
        let mut ao = AstOptions::new(pat, root.join(fname));
        ao.lang = Some(lang.into());
        ao.index = IndexMode::Auto;
        let res = search_ast(&ao);
        assert_eq!(
            res.status,
            Status::Complete,
            "AST pattern `{pat}` failed for lang `{lang}` ({fname}): {:?}",
            res.error
        );
        assert_eq!(res.matched, 1, "expected 1 match in {fname} ({lang})");

        if let Some(sym_name) = expected_sym {
            let out = build_outline(&OutlineOptions::new(root.join(fname)));
            assert_eq!(out.status, Status::Complete, "outline failed for {fname}");
            assert!(
                out.files[0].items.iter().any(|it| it.name == sym_name),
                "expected symbol `{sym_name}` in {fname}, got {:?}",
                out.files[0].items
            );
        }
    }
}

#[test]
fn edit_lines_symbol_rev_and_syntax_guard() {
    use lancet::edit::rev;
    use lancet::edit_cmd::{edit_file, EditOptions, Op, Target};

    let dir = TempDir::new().unwrap();
    let rs = dir.path().join("a.rs");
    let orig = "/// Adds.\nfn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n\nfn main() {}\n";
    fs::write(&rs, orig).unwrap();

    // Dry-run by default: diff, nothing written.
    let mut o = EditOptions::new(
        &rs,
        Target::Symbol("add".into()),
        Op::Replace("/// Adds.\nfn add(a: i32, b: i32) -> i32 {\n    b + a\n}".into()),
    );
    let r = edit_file(&o);
    assert_eq!(r.status, Status::Complete);
    assert_eq!((r.start_line, r.end_line, r.applied), (1, 4, false));
    assert!(r.diff.contains("+    b + a"));
    assert_eq!(fs::read_to_string(&rs).unwrap(), orig);

    // Apply against the rev that `read` reported.
    let read_rev = read_file_range(&ReadOptions::new(rs.to_string_lossy().into_owned()))
        .rev
        .unwrap();
    o.rev = Some(read_rev.clone());
    o.apply = true;
    let r = edit_file(&o);
    assert!(r.applied);
    let now = fs::read_to_string(&rs).unwrap();
    assert!(now.contains("    b + a\n"));
    assert_eq!(r.rev_after, Some(rev(now.as_bytes())));

    // The old rev is stale now: conflict, nothing written.
    let mut del = EditOptions::new(&rs, Target::Lines(1, Some(1)), Op::Delete);
    del.rev = Some(read_rev);
    del.apply = true;
    assert_eq!(edit_file(&del).status, Status::Conflict);
    assert_eq!(fs::read_to_string(&rs).unwrap(), now);

    // An edit that breaks the syntax is refused unless explicitly allowed.
    #[cfg(feature = "ast")]
    {
        let mut bad = EditOptions::new(
            &rs,
            Target::Lines(4, Some(4)),
            Op::Replace("    b +".into()),
        );
        bad.apply = true;
        let r = edit_file(&bad);
        assert_eq!(r.status, Status::InvalidQuery);
        assert!(r.rev_after.is_none());
        assert_eq!(fs::read_to_string(&rs).unwrap(), now);
        bad.allow_syntax_errors = true;
        assert!(edit_file(&bad).applied);
        fs::write(&rs, &now).unwrap();
    }

    // Insert before / after, delete, out-of-range.
    let mut ins = EditOptions::new(
        &rs,
        Target::Lines(6, None),
        Op::InsertAfter("// end".into()),
    );
    ins.apply = true;
    edit_file(&ins);
    let mut ins = EditOptions::new(
        &rs,
        Target::Lines(1, Some(1)),
        Op::InsertBefore("// top".into()),
    );
    ins.apply = true;
    edit_file(&ins);
    let txt = fs::read_to_string(&rs).unwrap();
    assert!(txt.starts_with("// top\n/// Adds.\n") && txt.ends_with("fn main() {}\n// end\n"));
    let r = edit_file(&EditOptions::new(
        &rs,
        Target::Lines(99, Some(99)),
        Op::Delete,
    ));
    assert_eq!(r.status, Status::InvalidQuery);

    // Line endings: no trailing newline stays absent, CRLF stays CRLF.
    let py = dir.path().join("n.py");
    fs::write(&py, "x = 1\ny = 2").unwrap();
    let mut o = EditOptions::new(&py, Target::Lines(2, Some(2)), Op::Replace("y = 3".into()));
    o.apply = true;
    edit_file(&o);
    assert_eq!(fs::read_to_string(&py).unwrap(), "x = 1\ny = 3");
    let crlf = dir.path().join("c.txt");
    fs::write(&crlf, "a\r\nb\r\n").unwrap();
    let mut o = EditOptions::new(
        &crlf,
        Target::Lines(1, Some(1)),
        Op::Replace("A1\nA2".into()),
    );
    o.apply = true;
    edit_file(&o);
    assert_eq!(fs::read_to_string(&crlf).unwrap(), "A1\r\nA2\r\nb\r\n");

    // Documents and archives are read-only.
    #[cfg(feature = "adapters")]
    {
        let zip = dir.path().join("z.zip");
        fs::write(&zip, make_zip_bytes(&[("x.log", b"hello\n")])).unwrap();
        let r = edit_file(&EditOptions::new(
            &zip,
            Target::Lines(1, Some(1)),
            Op::Delete,
        ));
        assert_eq!(r.status, Status::Unsupported);
    }
}

#[test]
fn freshness_catches_preserved_mtime_and_racy_files() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let a = root.join("a.txt");
    fs::write(&a, "alpha_token\n").unwrap();
    let old_mtime = fs::metadata(&a).unwrap().modified().unwrap();
    std::thread::sleep(Duration::from_millis(250)); // leave the racy window
    let idx_dir = index::default_dir(root);
    index::sync(root, &idx_dir, &WalkOpts::default()).unwrap();

    // Same size, new content, mtime put back (what `cp -p`, `tar -x`, `touch -r` do):
    // (mtime, size) look unchanged, but ctime/inode do not.
    fs::write(&a, "omega_token\n").unwrap();
    fs::File::options()
        .write(true)
        .open(&a)
        .unwrap()
        .set_modified(old_mtime)
        .unwrap();
    assert_eq!(fs::metadata(&a).unwrap().modified().unwrap(), old_mtime);
    for q in ["\"omega_token\"", "\"alpha_token\""] {
        let scan = search(q, &opts(root, IndexMode::Off));
        let auto = search(q, &opts(root, IndexMode::Auto));
        assert_eq_responses(&scan, &auto, &format!("preserved mtime ({q})"));
    }
    #[cfg(unix)]
    assert_eq!(
        search("\"omega_token\"", &opts(root, IndexMode::Auto)).matched,
        1
    );

    // A file written right before a sync is flagged racy and always rescanned.
    fs::write(root.join("b.txt"), "beta_token\n").unwrap();
    index::sync(root, &idx_dir, &WalkOpts::default()).unwrap();
    let ix = Index::open(&idx_dir).unwrap().unwrap();
    let (si, id) = ix.lookup_live(b"b.txt").unwrap();
    assert_ne!(ix.segs[si].doc_meta(id).flags & index::FLAG_RACY, 0);
    let auto = search("\"beta_token\"", &opts(root, IndexMode::Auto));
    assert_eq!(auto.matched, 1);
    assert!(auto.stats.stale_files.unwrap_or(0) >= 1);
}
