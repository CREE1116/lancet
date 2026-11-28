use lancet::ast_grep::{search_ast, AstOptions};
use lancet::fd::{
    find_files, now_ns, parse_time, render_tree, FdOptions, PatternMode, SizeFilter, Types,
};
use lancet::index::{self, Index};
use lancet::outline::{build_outline, OutlineOptions};
use lancet::output::Status;
use lancet::query::FileType;
use lancet::read_cmd::{read_file_range, ReadOptions};
use lancet::refs::{find_refs, RefsOptions};
use lancet::rg::{run as rg_run, Case, RgOptions};
use lancet::scope::IndexMode;
use lancet::walk::WalkOpts;
use std::fs;
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};
use tempfile::TempDir;

/// lancet shows paths joined onto the root it was given (like `rg`/`fd`); tests
/// compare the part below the temporary directory.
fn rel(p: &str) -> String {
    match p.find("/.tmp") {
        Some(i) => p[i + 1..]
            .split_once('/')
            .map(|x| x.1.to_string())
            .unwrap_or_default(),
        None => p.to_string(),
    }
}

type RgMod = fn(&mut RgOptions);

/// What `rg` prints for `pat` under `root` with `mode`: the probe the index and
/// freshness tests compare across modes.
fn rg_probe(root: &Path, pat: &str, mode: IndexMode, f: RgMod) -> String {
    let mut o = RgOptions::new(vec![pat.into()]);
    o.paths = vec![root.to_path_buf()];
    o.index = mode;
    f(&mut o);
    let mut out = Vec::new();
    let _ = rg_run(&o, &mut out);
    String::from_utf8_lossy(&out).into_owned()
}

fn plain(_: &mut RgOptions) {}

fn docs(o: &mut RgOptions) {
    o.docs = true;
}

/// Searches whose output must not depend on the index: literals, case folding,
/// Unicode, patterns too short for trigrams, regex, multi-line, context, file
/// lists, documents.
fn probes() -> Vec<(&'static str, RgMod)> {
    vec![
        ("optimizer", plain),
        ("optimizer", |o| o.case = Case::Insensitive),
        ("AdamW", |o| o.word = true),
        ("kelvin", |o| o.case = Case::Insensitive),
        ("학습률", plain),
        ("ab", plain),
        ("z", plain),
        ("OOM", |o| o.count = true),
        ("alpha|AdamW", plain),
        ("max_grad_norm.*1\\.0", plain),
        ("alpha\nbeta", |o| o.multiline = true),
        ("optimizer_hit_1", |o| {
            o.before = 1;
            o.after = 1;
        }),
        ("optimizer", |o| o.files_with_matches = true),
        ("absent_term_xyz", |o| o.files_without_match = true),
        ("clip_grad_norm_(", |o| o.fixed_strings = true),
        ("gradient", docs),
    ]
}

/// Every probe prints the same with the index (in each of `modes`) as without.
fn assert_index_independent(
    root: &Path,
    probes: &[(&str, RgMod)],
    modes: &[IndexMode],
    label: &str,
) {
    for (pat, f) in probes {
        let scan = rg_probe(root, pat, IndexMode::Off, *f);
        for m in modes {
            assert_eq!(
                scan,
                rg_probe(root, pat, *m, *f),
                "{label}: `{pat}` scan vs {m:?}"
            );
        }
    }
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

#[test]
fn index_equals_scan_oracle_equivalence() {
    let dir = build_rich_corpus();
    let root = dir.path();
    let rep = index::rebuild(root, &index::default_dir(root), &WalkOpts::default()).unwrap();
    assert!(rep.added >= 9);
    assert_index_independent(
        root,
        &probes(),
        &[IndexMode::Auto, IndexMode::Trust],
        "fresh index",
    );
    // The index really chooses the files: --trust-index skips the freshness check, so
    // a literal written after indexing is invisible to it, while the default sees it.
    let notes = root.join("docs/notes.md");
    let mut body = fs::read_to_string(&notes).unwrap();
    body.push_str("late_literal_qq\n");
    fs::write(&notes, body).unwrap();
    let late = |m| rg_probe(root, "late_literal_qq", m, plain);
    assert_eq!(late(IndexMode::Trust), "");
    assert!(late(IndexMode::Auto).contains("notes.md"));
    assert_eq!(late(IndexMode::Auto), late(IndexMode::Off));
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

    let found = [
        ("docx_probe", "spec.docx"),
        ("pptx_probe", "deck.pptx"),
        ("odt_probe", "memo.odt"),
        ("hwpx_probe 학습률", "report.hwpx"),
        ("hwp_legacy_probe 한글 구형", "legacy.hwp"),
        ("epub_probe", "book.epub"),
        ("pdf_probe", "paper.pdf"),
        ("weft_xlsx_probe", "metrics.xlsx"),
        ("gz_probe", "app.log.gz"),
        ("zip_probe", "archive.zip"),
        ("tar_probe", "bundle.tar"),
        ("tar_probe", "bundle.tar.gz"),
    ];
    for (pat, path) in found {
        let out = rg_probe(r, pat, IndexMode::Off, docs);
        assert!(
            out.lines().any(|l| rel(l).starts_with(&format!("{path}:"))),
            "`{pat}` should be found in {path}:\n{out}"
        );
        assert_index_independent(
            r,
            &[(pat, docs)],
            &[IndexMode::Auto, IndexMode::Trust],
            "documents",
        );
    }
    // An EPUB's script, a binary ZIP entry and an image (without --ocr) are not text.
    for pat in ["ignored_script", "skip_binary_entry", "IHDR"] {
        assert_eq!(rg_probe(r, pat, IndexMode::Off, docs), "", "{pat}");
    }
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

    let changed: &[(&str, RgMod)] = &[
        ("Lion", plain),
        ("brand_new_token", plain),
        ("AdamW", plain),
        ("optimizer_nonutf8", plain),
    ];
    assert_index_independent(root, changed, &[IndexMode::Auto], "stale index");
    assert!(rg_probe(root, "Lion", IndexMode::Auto, plain).contains("config.json"));

    let rep2 = index::sync(root, &idx_dir, &WalkOpts::default()).unwrap();
    assert_eq!(rep2.added, 2);
    assert_eq!(rep2.removed, 1);
    assert!(rep2.unchanged >= 4);
    assert_eq!(rep2.segments, 2);

    assert_index_independent(root, changed, &[IndexMode::Trust], "synced index");

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
    assert!(base
        .entries
        .iter()
        .all(|e| !rel(&e.path).contains("ignored_dir")));
    assert!(base.entries.iter().all(|e| rel(&e.path) != "skip.tmp"));
    assert!(base
        .entries
        .iter()
        .all(|e| !rel(&e.path).starts_with(".git")));
    assert!(base
        .entries
        .iter()
        .any(|e| rel(&e.path) == "src" && e.kind == "dir"));
    assert!(base
        .entries
        .iter()
        .any(|e| rel(&e.path) == "src/sub/engine.rs" && e.kind == "file"));

    // 2. Glob & Regex & extension & FileType filters
    let mut glob_rs = FdOptions::new(root);
    glob_rs.patterns = vec!["*.rs".into()];
    glob_rs.pattern_mode = PatternMode::Glob;
    let r_glob = find_files(&glob_rs);
    assert_eq!(
        r_glob
            .entries
            .iter()
            .map(|e| rel(&e.path))
            .collect::<Vec<_>>(),
        vec!["src/sub/engine.rs"]
    );

    let mut rx_py_go = FdOptions::new(root);
    // As fd: the pattern meets the name, or the absolute path with `full_path`.
    rx_py_go.patterns = vec![r"/src/[^/]+\.(py|go)$".into()];
    rx_py_go.pattern_mode = PatternMode::Regex;
    rx_py_go.full_path = true;
    let r_rx = find_files(&rx_py_go);
    assert_eq!(
        r_rx.entries
            .iter()
            .map(|e| rel(&e.path))
            .collect::<Vec<_>>(),
        vec!["src/train.py", "src/worker.go"]
    );

    let mut tc_doc = FdOptions::new(root);
    tc_doc.file_types = vec![FileType::Document];
    let r_doc = find_files(&tc_doc);
    let doc_paths: Vec<String> = r_doc.entries.iter().map(|e| rel(&e.path)).collect();
    assert!(doc_paths.iter().any(|p| p == "docs/notes.md"));
    assert!(doc_paths.iter().any(|p| p == "docs/big.txt"));
    assert!(!doc_paths.iter().any(|p| p == "src/train.py"));

    // 3. Max-depth & size & changed-within
    let mut depth1 = FdOptions::new(root);
    depth1.max_depth = Some(1);
    let r_d1 = find_files(&depth1);
    assert!(r_d1.entries.iter().all(|e| !rel(&e.path).contains('/')));

    let mut big_only = FdOptions::new(root);
    big_only.sizes = vec![SizeFilter::parse("+5ki").unwrap()];
    let r_big = find_files(&big_only);
    assert_eq!(
        r_big
            .entries
            .iter()
            .map(|e| rel(&e.path))
            .collect::<Vec<_>>(),
        vec!["docs/big.txt"]
    );

    let mut recent = FdOptions::new(root);
    recent.types = Types::parse(&["f".into()]).unwrap();
    recent.changed_after_ns = Some(parse_time("7d", now_ns()).unwrap());
    let r_rec = find_files(&recent);
    assert!(r_rec.entries.iter().any(|e| rel(&e.path) == "src/train.py"));
    assert!(!r_rec
        .entries
        .iter()
        .any(|e| rel(&e.path) == "docs/empty.txt"));

    // 4. Tree rendering & Trust-index parity (including ancestor discovery on subdir)
    let tree = render_tree(root, &base.entries);
    assert!(tree.contains("├── ") || tree.contains("└── "));
    assert!(tree.contains("src/\n"));

    index::rebuild(root, &index::default_dir(root), &WalkOpts::default()).unwrap();

    let mut trust_opts = FdOptions::new(root);
    trust_opts.index = IndexMode::Trust;
    trust_opts.types = Types::parse(&["f".into()]).unwrap();
    let r_trust = find_files(&trust_opts);
    assert_eq!(r_trust.mode, "index_trusted");
    let mut scan_files_opts = FdOptions::new(root);
    scan_files_opts.types = Types::parse(&["f".into()]).unwrap();
    let r_scan_files = find_files(&scan_files_opts);
    assert_eq!(r_trust.entries, r_scan_files.entries);

    let mut sub_trust = FdOptions::new(root.join("src"));
    sub_trust.index = IndexMode::Trust;
    sub_trust.types = Types::parse(&["f".into()]).unwrap();
    let r_sub_trust = find_files(&sub_trust);
    assert_eq!(r_sub_trust.mode, "index_trusted");
    let mut sub_scan = FdOptions::new(root.join("src"));
    sub_scan.types = Types::parse(&["f".into()]).unwrap();
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
    // Paths are shown under the root that was searched (`src`).
    assert_eq!(rel(&res_sub.matches[0].path), "src/main.rs");
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
    assert!(r_dry.diffs[0]
        .diff
        .contains(&format!("--- a/{}", r_dry.diffs[0].path)));
    assert!(r_dry.diffs[0].path.ends_with("/sample.rs"));
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
fn ast_rewrites_are_written_together_or_not_at_all() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("a.rs"), "fn a() { foo(1); }\n").unwrap();
    fs::write(root.join("b.rs"), "fn b() { foo(2); }\n").unwrap();
    let mut o = AstOptions::new("foo($X)", root);
    o.lang = Some("rs".into());
    o.rewrite = Some("foo($X".into());
    o.apply = true;

    // One rewrite would break the syntax: nothing is written, not even the other file.
    let r = search_ast(&o);
    assert_eq!(r.status, Status::InvalidQuery);
    let msg = r.error.unwrap().message;
    assert!(
        msg.contains("syntax error") && msg.contains("nothing written"),
        "{msg}"
    );
    assert!(r.diffs.iter().all(|d| !d.applied));
    assert_eq!(
        fs::read_to_string(root.join("a.rs")).unwrap(),
        "fn a() { foo(1); }\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("b.rs")).unwrap(),
        "fn b() { foo(2); }\n"
    );

    // A clean rewrite lands in both.
    o.rewrite = Some("bar($X)".into());
    let r = search_ast(&o);
    assert_eq!(r.status, Status::Complete);
    assert_eq!(r.diffs.iter().filter(|d| d.applied).count(), 2);
    assert_eq!(
        fs::read_to_string(root.join("a.rs")).unwrap(),
        "fn a() { bar(1); }\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("b.rs")).unwrap(),
        "fn b() { bar(2); }\n"
    );
}

#[cfg(feature = "ast")]
#[test]
fn ast_rewrites_keep_comments_and_rust_match_arms_are_patterns() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let py =
        "def f(x):\n    if x:\n        # keep me\n        return 1\n    if x:\n        return 2\n";
    fs::write(root.join("c.py"), py).unwrap();
    let mut o = AstOptions::new("if $C: return $X", root);
    o.lang = Some("py".into());
    o.rewrite = Some("if $C: return -$X".into());
    // The commented `if` matches but isn't rewritten: the template would drop `# keep me`.
    let r = search_ast(&o);
    assert_eq!(r.matched, 2);
    assert!(
        r.diffs[0].diff.contains("+    if x: return -2"),
        "{}",
        r.diffs[0].diff
    );
    assert!(!r.diffs[0].diff.contains("-        # keep me"));
    assert!(r.warnings.iter().any(|w| w.contains("drop a comment")));
    // …and under --apply that refusal stops the whole rewrite.
    o.apply = true;
    assert_eq!(search_ast(&o).status, Status::InvalidQuery);
    assert_eq!(fs::read_to_string(root.join("c.py")).unwrap(), py);

    fs::write(
        root.join("m.rs"),
        "fn f() { match x { _ => 1, Some(y) => 2 } }\nfn g() { match x {} }\n",
    )
    .unwrap();
    let mut m = AstOptions::new("match $X { $$$ARMS }", root);
    m.lang = Some("rs".into());
    assert_eq!(search_ast(&m).matched, 2);
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
        assert_eq!(rel(&refs_scan.definitions[0].path), "src/sub/engine.rs");
        assert_eq!(refs_scan.definitions[0].kind, "struct");
        assert_eq!(refs_scan.references.len(), 1);
        assert_eq!(rel(&refs_scan.references[0].path), "src/caller.rs");
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
    let pats: Vec<(&str, RgMod)> = [
        "token_common",
        "mutated_round_1",
        "mutated_round_8",
        "extra_fn_5",
        "worker_49",
    ]
    .into_iter()
    .map(|p| (p, plain as RgMod))
    .collect();
    assert_index_independent(
        root,
        &pats,
        &[IndexMode::Auto, IndexMode::Trust],
        "compacted",
    );
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
    // (mtime, size) look unchanged, but ctime/inode do not. Windows exposes no ctime
    // through std, so this is a documented Unix-only guarantee.
    #[cfg(unix)]
    {
        fs::write(&a, "omega_token\n").unwrap();
        fs::File::options()
            .write(true)
            .open(&a)
            .unwrap()
            .set_modified(old_mtime)
            .unwrap();
        assert_eq!(fs::metadata(&a).unwrap().modified().unwrap(), old_mtime);
        let pats: &[(&str, RgMod)] = &[("omega_token", plain), ("alpha_token", plain)];
        assert_index_independent(root, pats, &[IndexMode::Auto], "preserved mtime");
        assert!(
            rg_probe(root, "omega_token", IndexMode::Auto, plain).contains("a.txt:1:omega_token")
        );
    }
    #[cfg(not(unix))]
    let _ = old_mtime;

    // A file written right before a sync is flagged racy and always rescanned.
    fs::write(root.join("b.txt"), "beta_token\n").unwrap();
    index::sync(root, &idx_dir, &WalkOpts::default()).unwrap();
    let ix = Index::open(&idx_dir).unwrap().unwrap();
    let (si, id) = ix.lookup_live(b"b.txt").unwrap();
    assert_ne!(ix.segs[si].doc_meta(id).flags & index::FLAG_RACY, 0);
    assert!(rg_probe(root, "beta_token", IndexMode::Auto, plain).contains("b.txt:1:beta_token"));
}

#[test]
fn concurrent_edits_with_same_rev_have_exactly_one_winner() {
    use lancet::edit_cmd::{edit_file, EditOptions, Op, Target};
    use std::sync::{Arc, Barrier};

    let dir = TempDir::new().unwrap();
    let f = dir.path().join("shared.rs");
    fs::write(&f, "fn base() {}\n").unwrap();
    let rev0 = lancet::edit::rev(&fs::read(&f).unwrap());

    let n = 8;
    let barrier = Arc::new(Barrier::new(n));
    let handles: Vec<_> = (0..n)
        .map(|i| {
            let (f, rev0, barrier) = (f.clone(), rev0.clone(), barrier.clone());
            std::thread::spawn(move || {
                let mut o = EditOptions::new(
                    &f,
                    Target::Lines(1, Some(1)),
                    Op::InsertAfter(format!("fn agent_{i}() {{}}")),
                );
                o.rev = Some(rev0);
                o.apply = true;
                barrier.wait(); // all agents read the same revision, then race
                let resp = edit_file(&o);
                (resp.status, resp.error.map(|e| e.message))
            })
        })
        .collect();
    let results: Vec<(Status, Option<String>)> =
        handles.into_iter().map(|h| h.join().unwrap()).collect();
    let winners = results
        .iter()
        .filter(|(s, _)| *s == Status::Complete)
        .count();
    let conflicts = results
        .iter()
        .filter(|(s, _)| *s == Status::Conflict)
        .count();
    assert_eq!((winners, conflicts), (1, n - 1), "{results:?}");

    // Exactly the winner's line was added; no update was lost or interleaved.
    let text = fs::read_to_string(&f).unwrap();
    assert_eq!(text.lines().count(), 2, "{text}");
    assert!(text.starts_with("fn base() {}\nfn agent_"));
}

#[test]
fn multi_hunk_edits_use_original_coordinates_and_are_all_or_nothing() {
    use lancet::edit_cmd::{edit_file, parse_hunks, EditOptions};

    let dir = TempDir::new().unwrap();
    let f = dir.path().join("m.rs");
    let orig = "fn a() {}\nfn b() {}\nfn c() {}\nfn d() {}\n";
    fs::write(&f, orig).unwrap();

    // Hunk 1 grows by two lines; hunks 2 and 3 still address the original lines 3 and 4.
    let hunks = parse_hunks(
        r#"[{"lines": "1", "with": "fn a() {\n    // grew\n}"},
            {"lines": "3", "delete": true},
            {"symbol": "d", "insert_after": "fn e() {}"}]"#,
    )
    .unwrap();
    let mut o = EditOptions::multi(&f, hunks);
    o.apply = true;
    let r = edit_file(&o);
    assert_eq!(r.status, Status::Complete);
    assert_eq!(r.hunks, vec![[1, 1], [3, 3], [4, 4]]);
    assert_eq!(
        fs::read_to_string(&f).unwrap(),
        "fn a() {\n    // grew\n}\nfn b() {}\nfn d() {}\nfn e() {}\n"
    );

    // Overlapping hunks, ambiguous same-spot inserts, or one bad hunk: nothing is written.
    fs::write(&f, orig).unwrap();
    for spec in [
        r#"[{"lines": "1:2", "delete": true}, {"lines": "2", "with": "x"}]"#,
        r#"[{"lines": "1", "insert_after": "x"}, {"lines": "2", "insert_before": "y"}]"#,
        r#"[{"lines": "1", "with": "x"}, {"lines": "99", "delete": true}]"#,
        r#"[{"lines": "1", "with": "x"}, {"symbol": "missing", "delete": true}]"#,
    ] {
        let mut o = EditOptions::multi(&f, parse_hunks(spec).unwrap());
        o.apply = true;
        assert_eq!(edit_file(&o).status, Status::InvalidQuery, "{spec}");
        assert_eq!(fs::read_to_string(&f).unwrap(), orig, "{spec}");
    }

    // Malformed specs are rejected up front.
    for bad in [
        r#"[]"#,
        r#"[{"lines": "1"}]"#,
        r#"[{"lines": "1", "symbol": "a", "delete": true}]"#,
        r#"[{"lines": "1", "with": "x", "delete": true}]"#,
        r#"[{"lines": "1", "replace": "x"}]"#,
    ] {
        assert!(parse_hunks(bad).is_err(), "{bad}");
    }
}

#[cfg(feature = "ast")]
#[test]
fn ast_multi_metavariables_have_bounded_worst_case() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let args: Vec<String> = (0..200).map(|i| format!("a{i}")).collect();
    let src = format!("f({})\n", args.join(", "));
    fs::write(root.join("x.py"), &src).unwrap();
    let run = |pat: &str, rewrite: Option<&str>| {
        let mut o = AstOptions::new(pat, root);
        o.lang = Some("py".into());
        o.rewrite = rewrite.map(Into::into);
        o.apply = rewrite.is_some();
        let t = std::time::Instant::now();
        (search_ast(&o), t.elapsed())
    };

    // Four `$$$` on a 200-argument call: n^4 splits without memoisation; exact answers now.
    let (r, took) = run("f($$$A, $$$B, $$$C, $$$D, a0)", None);
    assert_eq!((r.matched, r.warnings.len()), (0, 0), "{:?}", r.warnings);
    assert!(took < Duration::from_secs(15), "{took:?}");
    let (r, _) = run("f($$$A, a7, $$$B, a150, $$$C)", None);
    assert_eq!((r.matched, r.warnings.len()), (1, 0));

    // Back-references are part of the memo key, so they stay exact and fast too.
    let (r, took) = run("f($X, $$$A, $$$B, $$$C, $$$D, $X)", None);
    assert_eq!((r.matched, r.warnings.len()), (0, 0), "{:?}", r.warnings);
    assert!(took < Duration::from_secs(15), "{took:?}");

    // Repeated `$$$` groups defeat memoisation: past the per-node step budget the file
    // is reported as incomplete (loudly) and is never rewritten.
    let (r, took) = run("f($$$A, $$$B, $$$C, $$$A, $$$B, $$$C, a0)", Some("g()"));
    assert!(
        r.warnings.iter().any(|w| w.contains("too ambiguous")),
        "{:?}",
        r.warnings
    );
    assert!(r.diffs.is_empty());
    assert_eq!(fs::read_to_string(root.join("x.py")).unwrap(), src);
    assert!(took < Duration::from_secs(60), "{took:?}"); // generous: CI runs debug builds
}

/// Differential fuzzing of the core guarantee: for random corpora, random edits and
/// random queries, the indexed modes return exactly what a full scan returns
/// (auto always; trust whenever the index is up to date).
#[test]
fn fuzz_scan_equals_auto_equals_trust() {
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            // xorshift64*: deterministic, no dependency
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
        fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
            xs[self.below(xs.len())]
        }
    }
    const WORDS: &[&str] = &[
        "alpha",
        "beta",
        "gamma",
        "delta",
        "Foo_bar",
        "KEY",
        "\u{212A}ey",
        "key",
        "Straße",
        "STRASSE",
        "검색",
        "한글",
        "x1",
        "x42",
        "error",
        "err0r",
        "the",
        "unwrap",
        "spin_lock",
    ];
    const EXTS: &[&str] = &["txt", "rs", "log", "md"];

    fn content(r: &mut Rng) -> String {
        let mut s = String::new();
        for _ in 0..1 + r.below(30) {
            if r.below(6) == 0 {
                s.push('\n'); // blank lines shape `in block`
                continue;
            }
            for w in 0..1 + r.below(8) {
                if w > 0 {
                    s.push_str(r.pick(&[" ", " ", ", ", "(", "::", "\t"]));
                }
                s.push_str(r.pick(WORDS));
            }
            s.push('\n');
        }
        s
    }
    fn probe(r: &mut Rng) -> (String, RgMod) {
        // "zqxjk" never appears in generated content; only same-size swaps introduce it,
        // so a stale index can only miss it if freshness detection is broken.
        let w = if r.below(5) == 0 {
            "zqxjk"
        } else {
            r.pick(WORDS)
        };
        let pat = match r.below(5) {
            0 => r
                .pick(&["x\\d+", "(?i)gam+a", "^alpha", "err[0o]r", "beta$"])
                .to_string(),
            1 => format!("{w}|{}", r.pick(WORDS)),
            2 => {
                let cut: String = w.chars().skip(1).take(3).collect();
                if cut.is_empty() {
                    w.to_string()
                } else {
                    cut
                }
            }
            _ => w.to_string(),
        };
        let mods: [RgMod; 8] = [
            plain,
            plain,
            |o| o.case = Case::Insensitive,
            |o| o.word = true,
            |o| o.files_with_matches = true,
            |o| o.count = true,
            |o| o.invert = true,
            |o| o.globs = vec!["d1/**".into(), "*.rs".into()],
        ];
        (pat, mods[r.below(mods.len())])
    }

    for seed in [0x9E37_79B9_7F4A_7C15u64, 42, 0xDEAD_BEEF] {
        let mut r = Rng(seed);
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        let mut files = Vec::new();
        for i in 0..40 {
            let rel = format!("d{}/f{i}.{}", r.below(3), r.pick(EXTS));
            let p = root.join(&rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            let body = match i {
                0 => String::new(),                               // empty
                1 => format!("{}\0binary\n", content(&mut r)),    // binary
                2 => (0..400).map(|_| content(&mut r)).collect(), // large
                _ => content(&mut r),
            };
            fs::write(&p, body).unwrap();
            files.push(p);
        }
        std::thread::sleep(Duration::from_millis(250)); // leave the racy window
        let idx_dir = index::default_dir(root);
        let check = |r: &mut Rng, phase: &str, with_trust: bool| {
            for n in 0..60 {
                let (pat, f) = probe(r);
                let modes: &[IndexMode] = if with_trust {
                    &[IndexMode::Auto, IndexMode::Trust]
                } else {
                    &[IndexMode::Auto]
                };
                let label = format!("seed {seed:#x} {phase} #{n}");
                assert_index_independent(root, &[(&pat, f)], modes, &label);
            }
        };

        index::sync(root, &idx_dir, &WalkOpts::default()).unwrap();
        check(&mut r, "fresh index", true);

        // Edit without re-indexing: rewrites, appends, same-size swaps with the old
        // mtime put back, deletions and new files.
        for _ in 0..12 {
            let p = &files[3 + r.below(files.len() - 3)];
            if !p.exists() {
                continue;
            }
            match r.below(4) {
                0 => fs::write(p, content(&mut r)).unwrap(),
                1 => {
                    let mut s = fs::read_to_string(p).unwrap();
                    s.push_str(&content(&mut r));
                    fs::write(p, s).unwrap();
                }
                2 => {
                    let old = fs::metadata(p).unwrap().modified().unwrap();
                    let s = fs::read_to_string(p).unwrap().replace("alpha", "zqxjk");
                    fs::write(p, s).unwrap();
                    // Restoring mtime is only detectable where ctime exists (Unix).
                    if cfg!(unix) {
                        fs::File::options()
                            .write(true)
                            .open(p)
                            .unwrap()
                            .set_modified(old)
                            .unwrap();
                    }
                }
                _ => fs::remove_file(p).unwrap(),
            }
        }
        for j in 0..3 {
            fs::write(root.join(format!("d{j}/new{j}.txt")), content(&mut r)).unwrap();
        }
        check(&mut r, "stale index", false);

        std::thread::sleep(Duration::from_millis(250));
        index::sync(root, &idx_dir, &WalkOpts::default()).unwrap();
        check(&mut r, "re-synced index", true);
    }
}

#[test]
fn rg_mode_matches_ripgrep_semantics_and_is_index_independent() {
    use lancet::rg::{run, Case, RgOptions};

    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let files = [
        ("a.txt", "foo bar\nFoo Bar\nfoofoo foo,foo\nnothing\n"),
        ("a/b.txt", "alpha foo\n\nbeta\nfoo\n"),
        ("sym.txt", "a @x b\nx@x\n"),
        ("crlf.txt", "foo\r\nbar\r\n"),
        ("empty.txt", ""),
    ];
    for (rel, body) in files {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }
    let rg = |f: &dyn Fn(&mut RgOptions), pat: &str| -> (bool, String) {
        let mut o = RgOptions::new(vec![pat.into()]);
        o.paths = vec![root.to_path_buf()];
        o.index = IndexMode::Off;
        f(&mut o);
        let mut out = Vec::new();
        let any = run(&o, &mut out).unwrap();
        // lancet prints `/`-separated paths on every platform.
        let base = format!("{}/", root.display()).replace('\\', "/");
        (any, String::from_utf8(out).unwrap().replace(&base, ""))
    };

    // Component-wise path order (`a/b.txt` before `a.txt`), path:line:text.
    let (any, out) = rg(&|_| {}, "foo");
    assert!(any);
    assert_eq!(
        out,
        "a/b.txt:1:alpha foo\na/b.txt:4:foo\na.txt:1:foo bar\na.txt:3:foofoo foo,foo\ncrlf.txt:1:foo\r\n"
    );
    // -w: boundaries may be shared and may be non-word characters.
    let (_, out) = rg(
        &|o| {
            o.word = true;
            o.only_matching = true;
        },
        "foo",
    );
    assert_eq!(
        out,
        "a/b.txt:1:foo\na/b.txt:4:foo\na.txt:1:foo\na.txt:3:foo\na.txt:3:foo\ncrlf.txt:1:foo\n"
    );
    let (_, out) = rg(&|o| o.word = true, "@x");
    assert_eq!(out, "sym.txt:1:a @x b\n");
    // Case modes, counts, inversion, file lists.
    let (_, out) = rg(
        &|o| {
            o.case = Case::Smart;
            o.count = true;
        },
        "Foo",
    );
    assert_eq!(out, "a.txt:1\n");
    let (_, out) = rg(
        &|o| {
            o.case = Case::Smart;
            o.count = true;
        },
        "foo",
    );
    assert_eq!(out, "a/b.txt:2\na.txt:3\ncrlf.txt:1\n");
    let (_, out) = rg(
        &|o| {
            o.invert = true;
            o.files_with_matches = true;
        },
        "foo",
    );
    assert_eq!(out, "a/b.txt\na.txt\ncrlf.txt\nsym.txt\n");
    let (_, out) = rg(&|o| o.files_without_match = true, "foo");
    assert_eq!(out, "empty.txt\nsym.txt\n");
    // Context groups are separated by `--` only when context is requested.
    let (_, out) = rg(&|o| o.after = 1, "alpha|beta");
    assert_eq!(
        out,
        "a/b.txt:1:alpha foo\na/b.txt-2-\na/b.txt:3:beta\na/b.txt-4-foo\n"
    );
    let (_, out) = rg(&|o| o.replace = Some("[$1]".into()), "(f)oo b");
    assert_eq!(out, "a.txt:1:[f]ar\n");
    let (any, out) = rg(&|_| {}, "zzz");
    assert!(!any && out.is_empty());

    // With an index (fresh, then after an edit without re-indexing) output is identical.
    std::thread::sleep(Duration::from_millis(250));
    index::sync(root, &index::default_dir(root), &WalkOpts::default()).unwrap();
    fs::write(root.join("a/b.txt"), "zeta foo\n").unwrap();
    for pat in ["foo", "zeta", "o+ b"] {
        let off = rg(&|_| {}, pat);
        let auto = rg(&|o| o.index = IndexMode::Auto, pat);
        assert_eq!(off, auto, "{pat}");
    }
}

#[test]
fn patch_applies_unified_diffs_all_or_nothing() {
    use lancet::patch::{apply_patch, PatchOptions};

    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let a_src: String = (0..20).map(|i| format!("fn f{i}() {{}}\n")).collect();
    fs::write(root.join("a.rs"), &a_src).unwrap();
    fs::write(root.join("b.txt"), "one\ntwo\nthree\n").unwrap();
    fs::write(root.join("old.txt"), "bye\n").unwrap();
    let apply = |diff: &str, write: bool| {
        let mut o = PatchOptions::new(diff, root);
        o.apply = write;
        apply_patch(&o)
    };

    // Two files, a hunk at a shifted position, a creation and a deletion.
    let diff = "\
--- a/a.rs
+++ b/a.rs
@@ -5,3 +5,3 @@
 fn f4() {}
-fn f5() {}
+fn f5() -> i32 { 5 }
 fn f6() {}
--- a/b.txt
+++ b/b.txt
@@ -2,1 +2,1 @@
-two
+TWO
--- /dev/null
+++ b/new.txt
@@ -0,0 +1,1 @@
+hello
--- a/old.txt
+++ /dev/null
@@ -1,1 +0,0 @@
-bye
";
    fs::write(root.join("a.rs"), format!("// shifted\n{a_src}")).unwrap();
    let dry = apply(diff, false);
    assert_eq!(dry.status, Status::Complete, "{:?}", dry.error);
    assert!(!dry.applied && root.join("old.txt").exists());
    assert_eq!(dry.files[0].offsets, vec![1]); // found one line below its header
    let r = apply(diff, true);
    assert!(r.applied, "{:?}", r.error);
    assert!(fs::read_to_string(root.join("a.rs"))
        .unwrap()
        .contains("fn f5() -> i32 { 5 }\nfn f6"));
    assert_eq!(
        fs::read_to_string(root.join("b.txt")).unwrap(),
        "one\nTWO\nthree\n"
    );
    assert_eq!(fs::read_to_string(root.join("new.txt")).unwrap(), "hello\n");
    assert!(!root.join("old.txt").exists());

    // The second file's context no longer matches: the first file is not touched.
    let before = fs::read_to_string(root.join("a.rs")).unwrap();
    let diff = "\
--- a/a.rs
+++ b/a.rs
@@ -2,1 +2,1 @@
-fn f0() {}
+fn zero() {}
--- a/b.txt
+++ b/b.txt
@@ -2,1 +2,1 @@
-two
+deux
";
    let r = apply(diff, true);
    assert_eq!(r.status, Status::Conflict);
    assert!(!r.applied);
    assert_eq!(fs::read_to_string(root.join("a.rs")).unwrap(), before);

    // Creating an existing file, deleting a changed one, escaping the root, or adding
    // syntax errors is refused.
    for (diff, status) in [
        (
            "--- /dev/null\n+++ b/b.txt\n@@ -0,0 +1,1 @@\n+x\n",
            Status::Conflict,
        ),
        (
            "--- a/new.txt\n+++ /dev/null\n@@ -1,1 +0,0 @@\n-not hello\n",
            Status::Conflict,
        ),
        (
            "--- a/../x\n+++ b/../x\n@@ -1,1 +1,1 @@\n-a\n+b\n",
            Status::InvalidQuery,
        ),
        ("not a diff\n", Status::InvalidQuery),
    ] {
        assert_eq!(apply(diff, true).status, status, "{diff}");
    }
    #[cfg(feature = "ast")]
    {
        let r = apply(
            "--- a/a.rs\n+++ b/a.rs\n@@ -2,1 +2,1 @@\n-fn f0() {}\n+fn f0() {\n",
            true,
        );
        assert_eq!(r.status, Status::InvalidQuery);
        assert_eq!(fs::read_to_string(root.join("a.rs")).unwrap(), before);
    }
}

#[cfg(feature = "ast")]
#[test]
fn ast_ignores_trailing_commas_like_ast_grep() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("t.py"),
        "self.assertEqual(a, b)\nself.assertEqual(\n    a,\n    b,\n)\nself.assertEqual(a, b, msg)\n",
    )
    .unwrap();
    fs::write(dir.path().join("t.rs"), "fn f() { g(1, 2,); g(1, 2); }\n").unwrap();
    let count = |pat: &str, lang: &str| {
        let mut o = AstOptions::new(pat, dir.path());
        o.lang = Some(lang.into());
        search_ast(&o).matched
    };
    assert_eq!(count("self.assertEqual($A, $B)", "py"), 2); // not the 3-argument call
    assert_eq!(count("g($A, $B)", "rs"), 2);
}

#[cfg(feature = "ast")]
#[test]
fn ast_pattern_root_is_the_innermost_node_spanning_the_pattern() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("a.go"),
        "package p\nfunc a() error { return fmt.Errorf(\"x\") }\nfunc b() (int, error) { return 0, fmt.Errorf(\"y\") }\nfunc c() { g(1, fmt.Errorf(\"z\")) }\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("a.c"),
        "void f(int x){\n  if (x) { a(); } else { b(); }\n  do c(); while (x);\n}\n",
    )
    .unwrap();
    let lines = |pat: &str, lang: &str| -> Vec<usize> {
        let mut o = AstOptions::new(pat, dir.path());
        o.lang = Some(lang.into());
        search_ast(&o)
            .matches
            .iter()
            .map(|m| m.start_line)
            .collect()
    };
    // `_ = f(x)` used to make the pattern root an expression_list: only `return f(x)` matched.
    assert_eq!(lines("fmt.Errorf($$$A)", "go"), vec![2, 3, 4]);
    // `else $S` / `do $S` are statement positions too.
    assert_eq!(lines("if ($C) { $$$B } else $E", "c"), vec![2]);
    assert_eq!(lines("do $S while ($C);", "c"), vec![3]);
    assert_eq!(lines("if ($C) $S else $T", "c"), vec![2]);
}

#[test]
fn rg_mode_skips_late_nul_binaries_in_directory_searches() {
    use lancet::rg::{run, RgOptions};
    let dir = TempDir::new().unwrap();
    let mut body = b"needle text\n".repeat(2000); // NUL well past the 8 KiB sniff
    body.extend_from_slice(b"\0tail\n");
    fs::write(dir.path().join("late.pb"), &body).unwrap();
    fs::write(dir.path().join("t.txt"), "needle\n").unwrap();
    let mut o = RgOptions::new(vec!["needle".into()]);
    o.paths = vec![dir.path().to_path_buf()];
    o.index = IndexMode::Off;
    o.files_with_matches = true;
    let mut out = Vec::new();
    run(&o, &mut out).unwrap();
    let out = String::from_utf8(out).unwrap();
    assert!(
        out.ends_with("t.txt\n") && !out.contains("late.pb"),
        "{out}"
    );
    // Named explicitly, it is searched (like ripgrep).
    o.paths = vec![dir.path().join("late.pb")];
    let mut out = Vec::new();
    assert!(run(&o, &mut out).unwrap());
}

#[cfg(unix)]
#[test]
fn edits_keep_the_file_the_same_file() {
    use lancet::edit_cmd::{edit_file, EditOptions, Op, Target};
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    let d = dir.path();
    fs::write(d.join("real.txt"), "old\n").unwrap();
    std::os::unix::fs::symlink("real.txt", d.join("link.txt")).unwrap();
    fs::write(d.join("h1.txt"), "old\n").unwrap();
    fs::hard_link(d.join("h1.txt"), d.join("h2.txt")).unwrap();
    fs::write(d.join("mode.txt"), "old\n").unwrap();
    fs::set_permissions(d.join("mode.txt"), fs::Permissions::from_mode(0o640)).unwrap();
    for f in ["link.txt", "h1.txt", "mode.txt"] {
        let mut o = EditOptions::new(
            d.join(f),
            Target::Lines(1, Some(1)),
            Op::Replace("new".into()),
        );
        o.apply = true;
        assert_eq!(edit_file(&o).status, Status::Complete, "{f}");
    }
    // The symlink is still a symlink and its target changed.
    assert!(fs::symlink_metadata(d.join("link.txt"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(fs::read_to_string(d.join("real.txt")).unwrap(), "new\n");
    // Both hard-link names still see the same (new) content.
    assert_eq!(fs::read_to_string(d.join("h2.txt")).unwrap(), "new\n");
    // Permissions survive the atomic replace.
    assert_eq!(
        fs::metadata(d.join("mode.txt"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o640
    );
}

#[test]
fn rg_supports_max_depth_max_filesize_follow_and_color() {
    use lancet::rg::{parse_filesize, run, RgOptions};
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("top.txt"), "needle\n").unwrap();
    fs::create_dir_all(root.join("sub")).unwrap();
    fs::write(root.join("sub/deep.txt"), "needle\n").unwrap();
    fs::write(root.join("big.txt"), format!("needle\n{}", "x".repeat(200))).unwrap();

    #[cfg(unix)]
    let _ext = {
        let ext = TempDir::new().unwrap();
        fs::write(ext.path().join("linked.txt"), "needle via link\n").unwrap();
        std::os::unix::fs::symlink(ext.path().join("linked.txt"), root.join("sym.txt")).unwrap();
        ext
    };

    // Build index first to verify auto and trust modes also respect new filters.
    index::rebuild(root, &index::default_dir(root), &WalkOpts::default()).unwrap();

    let run_with = |mode: IndexMode, f: &dyn Fn(&mut RgOptions)| -> String {
        let mut o = RgOptions::new(vec!["needle".into()]);
        o.paths = vec![root.to_path_buf()];
        o.index = mode;
        f(&mut o);
        let mut out = Vec::new();
        let _ = run(&o, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    };

    for mode in [IndexMode::Off, IndexMode::Auto, IndexMode::Trust] {
        // max_depth=1 keeps top-level only (excludes sub/deep.txt)
        let d1 = run_with(mode, &|o| o.walk.max_depth = Some(1));
        assert!(d1.contains("top.txt"), "{mode:?}: {d1}");
        assert!(!d1.contains("sub/deep.txt"), "{mode:?}: {d1}");

        // max_filesize=50 excludes big.txt
        let sz = run_with(mode, &|o| {
            o.walk.max_filesize = Some(parse_filesize("50").unwrap())
        });
        assert!(sz.contains("top.txt"), "{mode:?}: {sz}");
        assert!(!sz.contains("big.txt"), "{mode:?}: {sz}");

        // follow=true finds symlinked file even when an index exists
        #[cfg(unix)]
        {
            let no_l = run_with(mode, &|_| {});
            assert!(!no_l.contains("sym.txt"), "{mode:?}: {no_l}");
            let with_l = run_with(mode, &|o| o.walk.follow = true);
            assert!(with_l.contains("sym.txt"), "{mode:?}: {with_l}");
        }
    }

    // Color output emits ANSI SGR only when enabled.
    let plain = run_with(IndexMode::Auto, &|_| {});
    assert!(!plain.contains("\x1b["));
    let colored = run_with(IndexMode::Auto, &|o| o.color = true);
    assert!(colored.contains("\x1b[35m") && colored.contains("\x1b[1;31mneedle\x1b[0m"));
}

/// Corrupt documents (truncated, bit-flipped, random, spliced) and a deflate bomb must
/// never crash, hang or hide the intact files next to them.
#[cfg(feature = "adapters")]
#[test]
fn corrupt_documents_are_skipped_safely() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let docx = make_zip_bytes(&[
        (
            "[Content_Types].xml",
            br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#,
        ),
        (
            "word/document.xml",
            br#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>SEEDTOKEN docx</w:t></w:r></w:p></w:body></w:document>"#,
        ),
    ]);
    let seeds: Vec<(&str, Vec<u8>, &str)> = vec![
        ("pdf", make_minimal_pdf("SEEDTOKEN pdf"), "SEEDTOKEN"),
        ("xlsx", make_minimal_xlsx(), "weft_xlsx_probe"),
        ("hwp", make_minimal_hwp("SEEDTOKEN hwp"), "SEEDTOKEN"),
        (
            "zip",
            make_zip_bytes(&[("a.log", b"SEEDTOKEN log\n")]),
            "SEEDTOKEN",
        ),
        ("docx", docx, "SEEDTOKEN"),
    ];
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut rnd = |n: usize| {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        (x % n.max(1) as u64) as usize
    };
    for (ext, seed, _) in &seeds {
        fs::write(root.join(format!("ok.{ext}")), seed).unwrap();
        for i in 0..40 {
            let mut b = seed.clone();
            match i % 4 {
                0 => b.truncate(1 + rnd(b.len() - 1)),
                1 => {
                    for _ in 0..1 + rnd(20) {
                        let at = rnd(b.len());
                        b[at] = rnd(256) as u8;
                    }
                }
                2 => b = (0..1 + rnd(4000)).map(|_| rnd(256) as u8).collect(),
                _ => {
                    let mid = b.len() / 2;
                    let junk: Vec<u8> = (0..200).map(|_| rnd(256) as u8).collect();
                    b.splice(mid..mid, junk);
                }
            }
            fs::write(root.join(format!("bad{i:02}.{ext}")), b).unwrap();
        }
    }
    // 128 MiB of one byte compresses to ~130 KB: extraction caps must hold.
    let big = vec![b'A'; 128 << 20];
    fs::write(root.join("bomb.zip"), make_zip_bytes(&[("big.txt", &big)])).unwrap();
    drop(big);

    let t = std::time::Instant::now();
    for (ext, _, token) in &seeds {
        let out = rg_probe(root, token, IndexMode::Off, docs);
        assert!(
            out.lines()
                .any(|l| rel(l).starts_with(&format!("ok.{ext}:"))),
            "intact ok.{ext} must still be found:\n{out}"
        );
    }
    for f in fs::read_dir(root).unwrap() {
        let p = f.unwrap().path();
        let _ = read_file_range(&ReadOptions::new(p.to_string_lossy().into_owned()));
    }
    let _ = build_outline(&OutlineOptions::new(root));
    assert!(t.elapsed() < Duration::from_secs(60), "{:?}", t.elapsed());
}

#[test]
fn patch_handles_git_renames_modes_and_refuses_binary() {
    use lancet::patch::{apply_patch, PatchOptions};

    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("y.txt"), "1\n2\n3\n").unwrap();
    fs::write(root.join("r.txt"), "keep\n").unwrap();
    fs::write(root.join("m.sh"), "m\n").unwrap();
    fs::write(root.join("e.txt"), "").unwrap();
    let apply = |diff: &str, write: bool| {
        let mut o = PatchOptions::new(diff, root);
        o.apply = write;
        apply_patch(&o)
    };
    // What `git diff -M` prints: a pure rename has no `---`/`+++` lines at all.
    let diff = "\
diff --git a/r.txt b/r2.txt
similarity index 100%
rename from r.txt
rename to r2.txt
diff --git a/y.txt b/sub/z.txt
similarity index 60%
rename from y.txt
rename to sub/z.txt
index 1..2 100644
--- a/y.txt
+++ b/sub/z.txt
@@ -1,3 +1,3 @@
 1
-2
+two
 3
diff --git a/m.sh b/m.sh
old mode 100644
new mode 100755
diff --git a/e.txt b/e.txt
deleted file mode 100644
index e69de29..0000000
diff --git a/new_empty b/new_empty
new file mode 100644
index 0000000..e69de29
";
    let r = apply(diff, true);
    assert!(r.applied, "{:?}", r.error);
    let actions: Vec<_> = r
        .files
        .iter()
        .map(|f| (f.action, f.path.as_str()))
        .collect();
    assert_eq!(
        actions,
        vec![
            ("rename", "r2.txt"),
            ("rename", "sub/z.txt"),
            ("mode", "m.sh"),
            ("delete", "e.txt"),
            ("create", "new_empty")
        ]
    );
    assert!(!root.join("r.txt").exists() && !root.join("y.txt").exists());
    assert_eq!(fs::read_to_string(root.join("r2.txt")).unwrap(), "keep\n");
    assert_eq!(
        fs::read_to_string(root.join("sub/z.txt")).unwrap(),
        "1\ntwo\n3\n"
    );
    assert!(!root.join("e.txt").exists());
    assert_eq!(fs::read(root.join("new_empty")).unwrap(), b"");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let m = fs::metadata(root.join("m.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(m & 0o777, 0o755);
    }

    // Renaming onto an existing file is a conflict, and nothing else is written.
    fs::write(root.join("a.txt"), "a\n").unwrap();
    let r = apply(
        "diff --git a/a.txt b/r2.txt\nrename from a.txt\nrename to r2.txt\n",
        true,
    );
    assert_eq!(r.status, Status::Conflict);
    assert!(root.join("a.txt").exists());

    // Binary changes used to be skipped silently; now the whole patch is refused.
    let r = apply(
        "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-a\n+b\ndiff --git a/x.bin b/x.bin\nindex 1..2 100644\nBinary files a/x.bin and b/x.bin differ\n",
        true,
    );
    assert_eq!(r.status, Status::InvalidQuery);
    assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "a\n");
}

#[test]
fn edit_find_replaces_exact_text_once_or_all() {
    use lancet::edit_cmd::{edit_file, parse_hunks, EditOptions, Op, Target};

    let dir = TempDir::new().unwrap();
    let f = dir.path().join("f.txt");
    fs::write(&f, "alpha x\r\nbeta x\r\ngamma\r\n").unwrap();
    let find = |text: &str, all: bool, op: Op| {
        let mut o = EditOptions::new(
            &f,
            Target::Find {
                text: text.into(),
                all,
                count: None,
            },
            op,
        );
        o.apply = true;
        edit_file(&o)
    };
    // Ambiguous text is refused, naming the lines; nothing is written.
    let r = find("x", false, Op::Replace("y".into()));
    assert_eq!(r.status, Status::InvalidQuery);
    assert!(r.error.unwrap().message.contains("lines 1, 2"));
    // `\n` in the text matches CRLF, and the replacement keeps CRLF.
    let r = find("x\nbeta", false, Op::Replace("X\nBETA".into()));
    assert!(r.applied, "{:?}", r.error);
    assert_eq!(r.hunks, vec![[1, 2]]);
    assert_eq!(
        fs::read_to_string(&f).unwrap(),
        "alpha X\r\nBETA x\r\ngamma\r\n"
    );
    let r = find("a", true, Op::Delete);
    assert!(r.applied);
    assert_eq!(
        fs::read_to_string(&f).unwrap(),
        "lph X\r\nBETA x\r\ngmm\r\n"
    );
    assert_eq!(find("zzz", false, Op::Delete).status, Status::InvalidQuery);
    // JSON hunks: find mixes with line hunks in one atomic write.
    let hunks =
        parse_hunks(r#"[{"find": "BETA", "with": "beta"}, {"lines": "3", "with": "last"}]"#)
            .unwrap();
    let mut o = EditOptions::multi(&f, hunks);
    o.apply = true;
    assert!(edit_file(&o).applied);
    assert_eq!(
        fs::read_to_string(&f).unwrap(),
        "lph X\r\nbeta x\r\nlast\r\n"
    );
    assert!(parse_hunks(r#"[{"lines": "1", "all": true, "delete": true}]"#).is_err());
}

#[test]
fn rg_passthru_and_type_add_match_ripgrep() {
    use lancet::rg::{run, RgOptions};
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("x.txt"), "a\nfoo 1\nb\nfoo 2\n").unwrap();
    fs::write(root.join("n.txt"), "none\n").unwrap();
    fs::write(root.join("k.weird"), "foo\n").unwrap();
    let rg = |f: &dyn Fn(&mut RgOptions), pat: &str| -> (bool, String) {
        let mut o = RgOptions::new(vec![pat.into()]);
        o.paths = vec![root.to_path_buf()];
        o.index = IndexMode::Off;
        f(&mut o);
        let mut out = Vec::new();
        let any = run(&o, &mut out).unwrap();
        // lancet prints `/`-separated paths on every platform.
        let base = format!("{}/", root.display()).replace('\\', "/");
        (any, String::from_utf8(out).unwrap().replace(&base, ""))
    };
    let passthru = |o: &mut RgOptions| {
        (o.passthru, o.before, o.after) = (true, usize::MAX / 4, usize::MAX / 4);
    };
    // Every line of every file, files without a match included, no `--` separators.
    let (any, out) = rg(&passthru, "foo");
    assert!(any);
    assert_eq!(
        out,
        "k.weird:1:foo\nn.txt-1-none\nx.txt-1-a\nx.txt:2:foo 1\nx.txt-3-b\nx.txt:4:foo 2\n"
    );
    // No match anywhere: still printed, but exit status 1.
    let (any, out) = rg(&passthru, "zzz");
    assert!(!any && out.contains("x.txt-2-foo 1"));
    // Past -m everything is context.
    let (_, out) = rg(
        &|o| {
            passthru(o);
            o.max_count = Some(1);
            o.paths = vec![root.join("x.txt")];
        },
        "foo",
    );
    assert!(out.ends_with("4-foo 2\n"), "{out}");

    // --type-add defines a type usable with -t / -T.
    let (_, out) = rg(
        &|o| {
            o.type_add = vec!["weird:*.weird".into()];
            o.types = vec!["weird".into()];
        },
        "foo",
    );
    assert_eq!(out, "k.weird:1:foo\n");
    let (_, out) = rg(
        &|o| {
            o.type_add = vec!["weird:*.weird".into()];
            o.types_not = vec!["weird".into()];
        },
        "foo",
    );
    assert!(!out.contains("k.weird") && out.contains("x.txt:2"));
}

#[test]
fn rename_local_bindings_exactly_or_refuses() {
    use lancet::rename::{rename, RenameOptions};
    let dir = TempDir::new().unwrap();
    let run = |file: &str, body: &str, line: usize, col: Option<usize>, old: &str, new: &str| {
        let p = dir.path().join(file);
        fs::write(&p, body).unwrap();
        let o = RenameOptions {
            path: p.clone(),
            line,
            col,
            old: old.into(),
            new: new.into(),
            rev: None,
            apply: true,
        };
        let r = rename(&o);
        (r, fs::read_to_string(&p).unwrap())
    };

    // Rust: shadowing picks the right binding; struct shorthand is expanded.
    let src = "fn f(c: u32) -> u32 {\n    let t = c;\n    let t = t * 2;\n    let p = P { t };\n    t + c\n}\n";
    let (r, out) = run("a.rs", src, 3, Some(9), "t", "acc");
    assert!(r.applied, "{:?}", r.error);
    assert_eq!(out, "fn f(c: u32) -> u32 {\n    let t = c;\n    let acc = t * 2;\n    let p = P { t: acc };\n    acc + c\n}\n");
    // Capture: `c` -> `t` would make `t + c` mean something else.
    let (r, out) = run("a.rs", src, 1, None, "c", "t");
    assert_eq!(r.status, Status::Conflict);
    assert_eq!(out, src);
    // std formatting macros: arguments and `{name}` captures are plain uses.
    let (r, out) = run(
        "m.rs",
        "fn h() { let x = 1; println!(\"{x} {:>x$} {{x}} x\", x + 1, s.x); }\n",
        1,
        None,
        "x",
        "y",
    );
    assert!(r.applied, "{:?}", r.error);
    assert_eq!(
        out,
        "fn h() { let y = 1; println!(\"{y} {:>y$} {{x}} x\", y + 1, s.x); }\n"
    );
    let (r, out) = run(
        "m.rs",
        "fn h(o: u8) { let x = 1; matches!(o + x, 1 | Some::B); }\n",
        1,
        None,
        "x",
        "y",
    );
    assert!(r.applied, "{:?}", r.error);
    assert!(
        out.contains("let y = 1; matches!(o + y, 1 | Some::B)"),
        "{out}"
    );
    // A capture of the new name would change meaning; a named argument, a closure,
    // or any other macro stays opaque.
    for (body, want) in [
        (
            "fn h(y: u8) { let x = 1; println!(\"{x}{y}\"); }\n",
            Status::Conflict,
        ),
        (
            "fn h() { let x = 1; println!(\"{x}\", x = 2); }\n",
            Status::Unsupported,
        ),
        (
            "fn h(v: &[u8]) { let x = 1; println!(\"{}\", v.iter().map(|a| a + x).count()); }\n",
            Status::Unsupported,
        ),
        ("fn h() { let x = 1; my!(x); }\n", Status::Unsupported),
        (
            "fn h(o: Option<u8>) { let x = 1; matches!(o, Some(x)); }\n",
            Status::Unsupported,
        ),
    ] {
        let (r, out) = run("m.rs", body, 1, None, "x", "y");
        assert_eq!(r.status, want, "{body}");
        assert_eq!(out, body);
    }
    // Module-level items need a compiler.
    let (r, _) = run(
        "s.rs",
        "fn lim() -> u32 { 1 }\nfn k() -> u32 { lim() }\n",
        1,
        None,
        "lim",
        "limit",
    );
    assert_eq!(r.status, Status::Unsupported);

    // Python: one local across a for-loop target; the comprehension's own `x` is untouched.
    let src = "def f(xs):\n    x = 0\n    for x in xs:\n        pass\n    ys = [x for x in xs]\n    return x, g(x=1), o.x\n";
    let (r, out) = run("a.py", src, 2, None, "x", "acc");
    assert!(r.applied, "{:?}", r.error);
    assert_eq!(out, "def f(xs):\n    acc = 0\n    for acc in xs:\n        pass\n    ys = [x for x in xs]\n    return acc, g(x=1), o.x\n");
    // Parameters can be passed by keyword from elsewhere.
    let (r, _) = run("a.py", src, 1, None, "xs", "items");
    assert_eq!(r.status, Status::Unsupported);

    // TypeScript: block scoping and object shorthand.
    let src =
        "function f(a: number) {\n  let x = a;\n  { const x = 2; use(x); }\n  return { x };\n}\n";
    let (r, out) = run("a.ts", src, 2, None, "x", "v");
    assert!(r.applied, "{:?}", r.error);
    assert_eq!(out, "function f(a: number) {\n  let v = a;\n  { const x = 2; use(x); }\n  return { x: v };\n}\n");

    // Go: `x, err :=` re-assigns an existing err; xs[i](…) parses like a generic but is an index.
    let src = "package p\n\nfunc f(fs []func()) {\n\ta, err := g()\n\tb, err := g()\n\tfor i := range fs {\n\t\tgo fs[i]()\n\t}\n\t_, _, _ = a, b, err\n}\n";
    let (r, out) = run("a.go", src, 4, None, "err", "e");
    assert!(r.applied, "{:?}", r.error);
    assert!(
        out.contains("a, e := g()\n\tb, e := g()") && out.contains("a, b, e\n"),
        "{out}"
    );
    let (r, out) = run("a.go", src, 6, None, "i", "j");
    assert!(r.applied, "{:?}", r.error);
    assert!(out.contains("for j := range fs {\n\t\tgo fs[j]()"), "{out}");

    // A stale rev writes nothing.
    let p = dir.path().join("a.rs");
    fs::write(&p, "fn f() { let a = 1; let _ = a; }\n").unwrap();
    let o = RenameOptions {
        path: p.clone(),
        line: 1,
        col: None,
        old: "a".into(),
        new: "b".into(),
        rev: Some("0000000000000000".into()),
        apply: true,
    };
    assert_eq!(rename(&o).status, Status::Conflict);
}

#[test]
fn refs_skip_other_types_members_and_shadowing_locals() {
    use lancet::refs::{find_refs, RefsOptions};
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let lines = |o: &RefsOptions| -> Vec<(String, usize)> {
        find_refs(o)
            .references
            .iter()
            .map(|r| (rel(&r.path), r.line))
            .collect()
    };
    // Rust free fn: `.rev()`, `rev:` (also inside a macro) and a local `rev` are not it.
    fs::write(
        root.join("a.rs"),
        "pub fn rev(b: &[u8]) -> u8 { b[0] }\n\
         fn g(xs: Vec<u8>) -> u8 {\n\
             let n: Vec<u8> = xs.into_iter().rev().collect();\n\
             let s = S { rev: rev(&n) };\n\
             let v = vec![S { rev: 1 }];\n\
             let rev = 3;\n\
             crate::rev(&n) + rev + s.rev\n\
         }\n",
    )
    .unwrap();
    // Python: a local of the same name is skipped, `mod.rev()` may be this function.
    fs::write(
        root.join("b.py"),
        "def rev(x):\n    return x\n\ndef h(m):\n    rev = 1\n    return rev + m.rev(2)\n\ndef k():\n    return rev(3)\n",
    )
    .unwrap();
    let mut o = RefsOptions::new("rev", root);
    o.index = IndexMode::Off;
    let r = find_refs(&o);
    assert_eq!(r.definitions.len(), 2, "{:?}", r.definitions);
    // Two definitions in two languages: only shadowing locals go (not members,
    // since the Python one isn't a Rust free fn).
    assert!(lines(&o).contains(&("b.py".into(), 6)), "{:?}", lines(&o));
    assert!(!lines(&o).contains(&("b.py".into(), 5)), "{:?}", lines(&o));
    fs::remove_file(root.join("b.py")).unwrap();
    assert_eq!(
        lines(&o),
        vec![("a.rs".into(), 4), ("a.rs".into(), 7)],
        "{:?}",
        find_refs(&o).warnings
    );
    assert!(find_refs(&o).warnings.iter().any(|w| w.contains("local")));
    // A name with no definition (only locals) still lists every use.
    let mut l = RefsOptions::new("xs", root);
    l.index = IndexMode::Off;
    assert_eq!(find_refs(&l).total_references, 2);
}

#[test]
fn refs_rank_orders_declarations_by_use() {
    use lancet::refs::{find_refs, rank_symbols, RefsOptions};
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/a.rs"),
        "pub struct UserService;\npub struct MailService;\nstruct Other;\n// MailService in a comment\nfn f(_: UserService, _: UserService) -> MailService { let _s = \"UserService\"; MailService }\n",
    )
    .unwrap();
    fs::write(root.join("src/b.rs"), "fn g(_: UserService) {}\n").unwrap();
    let mut o = RefsOptions::new("*Service", root);
    o.index = IndexMode::Off;
    let r = rank_symbols(&o, &["struct".into()]);
    let got: Vec<(&str, usize)> = r
        .entries
        .iter()
        .map(|e| (e.name.as_str(), e.references))
        .collect();
    // Comments and strings don't count; `Other` doesn't match the glob.
    assert_eq!(got, vec![("UserService", 3), ("MailService", 2)]);
    assert_eq!(rel(&r.entries[0].path), "src/a.rs");
    assert_eq!(r.entries[0].line, 1);
    // Same count as `lancet refs` for that name.
    let mut one = RefsOptions::new("UserService", root);
    one.index = IndexMode::Off;
    assert_eq!(find_refs(&one).total_references, 3);
    // Kind filter.
    assert!(rank_symbols(&o, &["function".into()]).entries.is_empty());
}

#[test]
fn cli_missing_paths_are_errors_and_fd_read_take_several_targets() {
    use std::process::Command;
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("a")).unwrap();
    fs::create_dir_all(root.join("b")).unwrap();
    fs::write(root.join("a/x.rs"), "fn x() {}\n// two\n").unwrap();
    fs::write(root.join("b/y.rs"), "fn y() {}\n").unwrap();
    let run = |args: &[&str]| {
        let o = Command::new(env!("CARGO_BIN_EXE_lancet"))
            .args(args)
            .current_dir(root)
            .output()
            .unwrap();
        (
            o.status.code().unwrap(),
            String::from_utf8_lossy(&o.stdout).into_owned(),
            String::from_utf8_lossy(&o.stderr).into_owned(),
        )
    };
    // A mistyped path is an error, not "nothing found": exit 2, or fd's 1 for fd.
    for args in [&["fd", ".", "nope"][..], &["fd", "a/nope"]] {
        let (code, _, err) = run(args);
        assert_eq!(code, 1, "{args:?}: {err}");
        assert!(err.contains("nope"), "{args:?}: {err}");
    }
    for args in [
        &["rg", "x", "nope"][..],
        &["refs", "x", "nope"],
        &["ast", "$X", "nope", "--lang", "rs"],
        &["outline", "nope"],
    ] {
        let (code, _, err) = run(args);
        assert_eq!(code, 2, "{args:?}: {err}");
        assert!(err.contains("nope"), "{args:?}: {err}");
    }
    // fd: several paths, `.` means everything.
    let (code, out, _) = run(&["fd", ".", "a", "b"]);
    assert_eq!(code, 0);
    assert_eq!(out, "a/x.rs\nb/y.rs\n");
    let (_, out, _) = run(&["fd", "-e", "rs", ".", "a", "b"]);
    assert_eq!(out, "a/x.rs\nb/y.rs\n");
    // read: several targets, each with its own header and rev.
    let (code, out, _) = run(&["read", "a/x.rs:2", "b/y.rs:1"]);
    assert_eq!(code, 0);
    assert!(
        out.starts_with("# a/x.rs rev ") && out.contains("\n\n# b/y.rs rev "),
        "{out}"
    );
    assert!(
        out.contains("2 | // two") && out.contains("1 | fn y() {}"),
        "{out}"
    );
    let (code, _, _) = run(&["read", "a/x.rs:1", "b/missing.rs"]);
    assert_eq!(code, 2);
}

#[test]
fn multi_file_edits_are_all_or_nothing() {
    use lancet::edit_cmd::{edit_files, parse_file_hunks, parse_hunks};
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.rs");
    let sub = dir.path().join("sub");
    fs::create_dir_all(&sub).unwrap();
    let b = sub.join("b.rs");
    fs::write(&a, "fn a() { old(1); }\n").unwrap();
    fs::write(&b, "fn b() { old(2); old(3); }\n").unwrap();
    let rev = |p: &Path| lancet::edit::rev(&fs::read(p).unwrap());
    // serde_json builds the JSON so Windows paths (backslashes) are escaped.
    let json = |ra: &str, rb: &str| {
        serde_json::json!([
            {"path": a, "rev": ra, "find": "old(", "with": "new("},
            {"path": b, "rev": rb, "find": "old(", "with": "new(", "all": true},
        ])
        .to_string()
    };
    // A single-file parser refuses hunks that name a file instead of misapplying them.
    assert!(parse_hunks(&json("x", "y")).is_err());

    // One stale rev: nothing is written anywhere.
    let files = parse_file_hunks(&json(&rev(&a), "0000000000000000"), None, None).unwrap();
    let r = edit_files(&files, true, false);
    assert_eq!(r.status, Status::Conflict);
    assert!(!r.applied);
    assert_eq!(fs::read_to_string(&a).unwrap(), "fn a() { old(1); }\n");

    // Fresh revs: both files change together.
    let files = parse_file_hunks(&json(&rev(&a), &rev(&b)), None, None).unwrap();
    let r = edit_files(&files, true, false);
    assert!(r.applied, "{:?}", r.error);
    assert_eq!(fs::read_to_string(&a).unwrap(), "fn a() { new(1); }\n");
    assert_eq!(
        fs::read_to_string(&b).unwrap(),
        "fn b() { new(2); new(3); }\n"
    );
    assert_eq!(r.files[1].rev_after.as_deref(), Some(rev(&b).as_str()));

    // A write that fails after the first file landed puts the first file back.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let spec = serde_json::json!([
            {"path": a, "find": "new(1)", "with": "x(1)"},
            {"path": b, "find": "new(2)", "with": "x(2)"},
        ]);
        let files = parse_file_hunks(&spec.to_string(), None, None).unwrap();
        fs::set_permissions(&sub, fs::Permissions::from_mode(0o555)).unwrap();
        let r = edit_files(&files, true, false);
        fs::set_permissions(&sub, fs::Permissions::from_mode(0o755)).unwrap();
        if !r.applied {
            // (Running as root ignores the read-only directory; then both land.)
            assert_eq!(fs::read_to_string(&a).unwrap(), "fn a() { new(1); }\n");
            assert!(r.error.unwrap().message.contains("nothing written"));
        }
    }
}

#[test]
fn builtin_tool_syntax_write_and_codex_patches() {
    use std::process::Command;
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/a.py"),
        "def a():\n    return 1\n\ndef b():\n    return 1\n",
    )
    .unwrap();
    let run = |args: &[&str], stdin: Option<&str>| {
        let mut c = Command::new(env!("CARGO_BIN_EXE_lancet"))
            .args(args)
            .current_dir(root)
            .stdin(if stdin.is_some() {
                std::process::Stdio::piped()
            } else {
                std::process::Stdio::null()
            })
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        if let Some(s) = stdin {
            use std::io::Write;
            c.stdin.take().unwrap().write_all(s.as_bytes()).unwrap();
        }
        let o = c.wait_with_output().unwrap();
        (
            o.status.code().unwrap(),
            String::from_utf8_lossy(&o.stdout).into_owned(),
            String::from_utf8_lossy(&o.stderr).into_owned(),
        )
    };
    let read = || fs::read_to_string(root.join("src/a.py")).unwrap();

    // Edit's old_string / new_string / replace_all / expected_replacements.
    let (code, _, err) = run(
        &[
            "edit",
            "src/a.py",
            "--old-string",
            "return 1",
            "--new-string",
            "return 2",
            "--expected-replacements",
            "3",
            "--apply",
        ],
        None,
    );
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("expected 3"), "{err}");
    let (code, _, _) = run(
        &[
            "edit",
            "src/a.py",
            "--old-string",
            "return 1",
            "--new-string",
            "return 2",
            "--replace-all",
            "--apply",
        ],
        None,
    );
    assert_eq!(code, 0);
    assert_eq!(read().matches("return 2").count(), 2);

    // Read's offset / limit (1-based).
    let (_, out, _) = run(&["read", "src/a.py", "--offset", "4", "--limit", "1"], None);
    assert!(out.ends_with("   4 | def b():\n"), "{out}");
    // A target's own `:range` wins over --offset / --limit.
    let (_, out, _) = run(
        &[
            "read",
            "src/a.py",
            "--offset",
            "4",
            "--limit",
            "1",
            "src/a.py:1",
        ],
        None,
    );
    assert!(
        out.contains("   4 | def b():\n") && out.ends_with("   1 | def a():\n"),
        "{out}"
    );
    // As the built-in Read: a read with no range stops at 2000 lines and very long
    // lines are cut (whole with --raw).
    let mut big: String = (1..=2500).map(|i| format!("x{i}\n")).collect();
    big.insert_str(0, &format!("{}\n", "y".repeat(2500)));
    fs::write(root.join("big.txt"), &big).unwrap();
    let (_, out, err) = run(&["read", "big.txt"], None);
    assert!(
        out.ends_with("2000 | x1999\n"),
        "{}",
        &out[out.len() - 40..]
    );
    assert!(out.contains("… [500 more characters]"));
    assert!(
        err.contains("showing lines 1-2000 of 2501: continue with --offset 2001"),
        "{err}"
    );
    let (_, out, _) = run(&["read", "big.txt", "--offset", "2001"], None);
    assert!(out.ends_with("2501 | x2500\n"));
    let (_, out, _) = run(&["read", "big.txt", "--raw", "--limit", "1"], None);
    assert!(out.contains(&"y".repeat(2500)));
    // Several paths, like rg and fd.
    let (code, out, _) = run(&["outline", "src/a.py", "big.txt"], None);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("src/a.py") && out.contains("big.txt"), "{out}");
    let (code, out, _) = run(&["refs", "b", "src/a.py", "src"], None);
    assert_eq!(code, 0, "{out}");
    assert_eq!(out.matches("[def:function] src/a.py:4").count(), 1, "{out}");

    // write: creates; an existing file needs the rev it was read at.
    let (code, _, _) = run(
        &["write", "src/n/new.py", "--content", "x = 1\n", "--apply"],
        None,
    );
    assert_eq!(code, 0);
    assert_eq!(
        fs::read_to_string(root.join("src/n/new.py")).unwrap(),
        "x = 1\n"
    );
    let (code, _, err) = run(
        &["write", "src/n/new.py", "--content", "x = 2\n", "--apply"],
        None,
    );
    assert_eq!(code, 2);
    let rev = err
        .split("rev ")
        .nth(1)
        .unwrap()
        .split(')')
        .next()
        .unwrap()
        .to_string();
    let (code, _, _) = run(
        &[
            "write",
            "src/n/new.py",
            "--with-file",
            "-",
            "--rev",
            &rev,
            "--apply",
        ],
        Some("x = 2\n"),
    );
    assert_eq!(code, 0);
    assert_eq!(
        fs::read_to_string(root.join("src/n/new.py")).unwrap(),
        "x = 2\n"
    );

    // Codex apply_patch: anchored update, add, move, delete; ambiguity refused.
    let ambiguous = "*** Begin Patch\n*** Update File: src/a.py\n@@\n-    return 2\n+    return 9\n*** End Patch\n";
    let (code, _, err) = run(&["patch", "-", "--apply"], Some(ambiguous));
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("lines 2 and 5"), "{err}");
    let patch = "*** Begin Patch\n*** Update File: src/a.py\n@@ def b():\n-    return 2\n+    return 3\n*** Add File: src/c.py\n+c = 3\n*** Update File: src/n/new.py\n*** Move to: src/n/moved.py\n@@\n-x = 2\n+x = 4\n*** End Patch\n";
    let (code, _, err) = run(&["patch", "-", "--apply"], Some(patch));
    assert_eq!(code, 0, "{err}");
    assert_eq!(read(), "def a():\n    return 2\n\ndef b():\n    return 3\n");
    assert_eq!(
        fs::read_to_string(root.join("src/c.py")).unwrap(),
        "c = 3\n"
    );
    assert!(!root.join("src/n/new.py").exists());
    assert_eq!(
        fs::read_to_string(root.join("src/n/moved.py")).unwrap(),
        "x = 4\n"
    );
    let (code, _, _) = run(
        &["patch", "-", "--apply"],
        Some("*** Begin Patch\n*** Delete File: src/c.py\n*** End Patch\n"),
    );
    assert_eq!(code, 0);
    assert!(!root.join("src/c.py").exists());

    // The index keeps itself out of git status.
    run(&["index", "."], None);
    assert_eq!(
        fs::read_to_string(root.join(".lancet/.gitignore")).unwrap(),
        "*\n"
    );
}

#[test]
fn printed_diffs_apply_back_with_patch() {
    use lancet::patch::{apply_patch, PatchOptions};
    use std::process::Command;
    let mut x: u64 = 0x2545_F491_4F6C_DD1D;
    let mut rnd = |n: usize| {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        (x % n.max(1) as u64) as usize
    };
    let dir = TempDir::new().unwrap();
    let f = dir.path().join("f.txt");
    // patch(1) as a second opinion on Unix; Windows' Git patch rewrites CRLF on its own.
    let has_patch = cfg!(unix) && Command::new("patch").arg("--version").output().is_ok();
    // Both sizes: the small one goes through the LCS, the big one (more than
    // 1414 lines on each side) through the trimmed / paired path.
    for (case, size) in (0..60).map(|c| (c, if c % 2 == 0 { 40 } else { 2000 })) {
        // Every third case is a CRLF file: the `\r`s must survive the diff.
        let eol = if case % 3 == 0 { "\r\n" } else { "\n" };
        let mut lines: Vec<String> = (0..size).map(|i| format!("line {i} {}", i % 7)).collect();
        let old = lines.join(eol) + if case % 5 == 0 { "" } else { eol };
        for _ in 0..1 + rnd(5) {
            let at = rnd(lines.len());
            match rnd(3) {
                0 => lines[at] = format!("changed {}", rnd(1000)),
                1 => {
                    for k in 0..1 + rnd(3) {
                        lines.insert(at, format!("new {k} {}", rnd(1000)));
                    }
                }
                _ => {
                    let n = (1 + rnd(3)).min(lines.len() - at);
                    lines.drain(at..at + n);
                }
            }
        }
        let new = lines.join(eol) + if case % 7 == 0 { "" } else { eol };
        let diff = lancet::edit::unified_diff("f.txt", &old, &new);
        if old == new {
            continue;
        }
        fs::write(&f, &old).unwrap();
        let mut o = PatchOptions::new(diff.clone(), dir.path());
        o.apply = true;
        o.allow_syntax_errors = true;
        let r = apply_patch(&o);
        assert!(r.applied, "case {case}: {:?}\n{diff}", r.error);
        assert_eq!(fs::read_to_string(&f).unwrap(), new, "case {case}:\n{diff}");
        if has_patch {
            fs::write(&f, &old).unwrap();
            let pf = dir.path().join("d.patch");
            fs::write(&pf, &diff).unwrap();
            let st = Command::new("patch")
                .args(["-s", "-p1", "-i"])
                .arg(&pf)
                .current_dir(dir.path())
                .status()
                .unwrap();
            assert!(st.success(), "case {case}: patch(1) refused\n{diff}");
            assert_eq!(
                fs::read_to_string(&f).unwrap(),
                new,
                "case {case}: patch(1)\n{diff}"
            );
        }
    }
}

#[test]
fn trust_index_answers_only_for_the_files_it_holds() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("a.toml"), "edition = 1\n").unwrap();
    fs::write(root.join(".hidden.toml"), "edition = 2\n").unwrap();
    let idx = index::default_dir(root);
    let glob: RgMod = |o| o.globs = vec!["*.toml".into()];
    let hidden: RgMod = |o| o.walk.hidden = true;
    // Built without hidden files: a whitelisting glob still finds the hidden one.
    index::rebuild(root, &idx, &WalkOpts::default()).unwrap();
    let want = rg_probe(root, "edition", IndexMode::Off, glob);
    assert!(want.contains(".hidden.toml"), "{want}");
    assert_eq!(rg_probe(root, "edition", IndexMode::Trust, glob), want);
    // Built with hidden files: a search without --hidden doesn't see them.
    let wopts = WalkOpts {
        hidden: true,
        ..WalkOpts::default()
    };
    index::rebuild(root, &idx, &wopts).unwrap();
    let want = rg_probe(root, "edition", IndexMode::Off, plain);
    assert!(!want.contains(".hidden.toml"), "{want}");
    assert_eq!(rg_probe(root, "edition", IndexMode::Trust, plain), want);
    assert_eq!(
        rg_probe(root, "edition", IndexMode::Trust, hidden),
        rg_probe(root, "edition", IndexMode::Off, hidden)
    );
}
