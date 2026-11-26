//! Pure-Rust file adapters for documents, spreadsheets, and archives, plus
//! optional opt-in external OCR (`--ocr`) for scanned PDFs and images.
//! Enabled by the default `adapters` Cargo feature; compile with
//! `--no-default-features` for a pure-text/code core binary.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContainerMeta {
    pub kind: &'static str,
    pub name: String,
    pub detail: Option<String>,
}

#[cfg(feature = "adapters")]
mod imp {
    use crate::content;
    use calamine::{open_workbook_auto, DataType, Reader};
    use flate2::read::{DeflateDecoder, GzDecoder};
    use quick_xml::events::Event;
    use quick_xml::Reader as XmlReader;
    use std::fs::{self, File};
    use std::io::{self, Read};
    use std::panic::{self, AssertUnwindSafe};
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tar::Archive;
    use zip::ZipArchive;

    pub const ENABLED: bool = true;
    const MAX_INPUT_BYTES: u64 = 64 << 20;
    const MAX_EXTRACTED_BYTES: usize = 16 << 20;
    const MAX_ARCHIVE_ENTRIES: usize = 500;
    static OCR_SEQ: AtomicU64 = AtomicU64::new(0);

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Kind {
        Pdf,
        Docx,
        Pptx,
        Odt,
        Hwp,
        Hwpx,
        Epub,
        Spreadsheet,
        TarGz,
        Tar,
        Gz,
        Zip,
        Image,
    }

    fn detect(path: &Path) -> Option<Kind> {
        let name = path.file_name()?.to_str()?.to_ascii_lowercase();
        if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
            return Some(Kind::TarGz);
        }
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "pdf" => Some(Kind::Pdf),
            "docx" => Some(Kind::Docx),
            "pptx" => Some(Kind::Pptx),
            "odt" => Some(Kind::Odt),
            "hwp" => Some(Kind::Hwp),
            "hwpx" => Some(Kind::Hwpx),
            "epub" => Some(Kind::Epub),
            "xlsx" | "xls" | "xlsm" | "xlsb" | "ods" => Some(Kind::Spreadsheet),
            "tar" => Some(Kind::Tar),
            "gz" => Some(Kind::Gz),
            "zip" => Some(Kind::Zip),
            "png" | "jpg" | "jpeg" | "tif" | "tiff" | "bmp" | "webp" => Some(Kind::Image),
            _ => None,
        }
    }

    thread_local! {
        /// Set while a third-party extractor runs on this thread.
        static IN_ADAPTER: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    /// Extractor panics (a malformed PDF, say) are caught and the file is treated as
    /// unreadable; don't also print a panic message for them. Other panics still print.
    fn quiet_adapter_panics() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            let prev = panic::take_hook();
            panic::set_hook(Box::new(move |info| {
                if !IN_ADAPTER.with(|c| c.get()) {
                    prev(info);
                }
            }));
        });
    }

    #[inline]
    pub fn is_adapted_path(path: &Path, ocr: bool) -> bool {
        match detect(path) {
            Some(Kind::Image) => ocr,
            Some(_) => true,
            None => false,
        }
    }

    fn truncate_utf8(s: &mut Vec<u8>, max_len: usize) {
        if s.len() <= max_len {
            return;
        }
        let mut n = max_len;
        while n > 0 && (s[n] & 0xC0) == 0x80 {
            n -= 1;
        }
        s.truncate(n);
    }

    pub fn extract(path: &Path, ocr: bool) -> io::Result<Option<Vec<u8>>> {
        let Some(kind) = detect(path) else {
            return Ok(None);
        };
        if kind == Kind::Image && !ocr {
            return Ok(None);
        }
        let meta = fs::metadata(path)?;
        if meta.len() == 0 || meta.len() > MAX_INPUT_BYTES {
            return Ok(None);
        }
        quiet_adapter_panics();
        IN_ADAPTER.with(|c| c.set(true));
        let res = panic::catch_unwind(AssertUnwindSafe(|| match kind {
            Kind::Pdf => {
                let txt = extract_pdf(path)?;
                if txt.is_none() && ocr {
                    extract_pdf_ocr(path)
                } else {
                    Ok(txt)
                }
            }
            Kind::Docx => extract_docx(path),
            Kind::Pptx => extract_pptx(path),
            Kind::Odt => extract_odt(path),
            Kind::Hwp => extract_hwp(path),
            Kind::Hwpx => extract_hwpx(path),
            Kind::Epub => extract_epub(path),
            Kind::Spreadsheet => extract_spreadsheet(path),
            Kind::TarGz => extract_targz(path),
            Kind::Tar => extract_tar(path),
            Kind::Gz => extract_gz(path),
            Kind::Zip => extract_zip(path),
            Kind::Image => Ok(run_tesseract(path).map(|s| s.into_bytes())),
        }));
        IN_ADAPTER.with(|c| c.set(false));
        match res {
            Ok(Ok(Some(mut out))) => {
                truncate_utf8(&mut out, MAX_EXTRACTED_BYTES);
                if out.is_empty() || content::is_binary(&out) {
                    Ok(None)
                } else {
                    Ok(Some(out))
                }
            }
            Ok(Ok(None)) | Ok(Err(_)) | Err(_) => Ok(None),
        }
    }

    fn extract_pdf(path: &Path) -> io::Result<Option<Vec<u8>>> {
        let Ok(pages) = pdf_extract::extract_text_by_pages(path) else {
            return Ok(None);
        };
        let mut out = String::new();
        for (i, page) in pages.into_iter().enumerate() {
            let trimmed = page.trim();
            if trimmed.is_empty() {
                continue;
            }
            out.push_str(&format!("[Page {}]\n{}\n\n", i + 1, trimmed));
            if out.len() >= MAX_EXTRACTED_BYTES {
                break;
            }
        }
        Ok((!out.is_empty()).then(|| out.into_bytes()))
    }

    fn run_tesseract(path: &Path) -> Option<String> {
        for lang in ["eng+kor", "eng"] {
            let Ok(out) = Command::new("tesseract")
                .arg(path)
                .arg("stdout")
                .arg("-l")
                .arg(lang)
                .output()
            else {
                return None;
            };
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !s.is_empty() {
                    return Some(s);
                }
                return None;
            }
        }
        None
    }

    struct TempCleanup(PathBuf);
    impl Drop for TempCleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn extract_pdf_ocr(path: &Path) -> io::Result<Option<Vec<u8>>> {
        let seq = OCR_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lancet-ocr-{}-{seq}", std::process::id()));
        fs::create_dir_all(&dir)?;
        let _guard = TempCleanup(dir.clone());

        let prefix = dir.join("page");
        let Ok(status) = Command::new("pdftoppm")
            .arg("-png")
            .arg("-f")
            .arg("1")
            .arg("-l")
            .arg("10")
            .arg(path)
            .arg(&prefix)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
        else {
            return Ok(None);
        };
        if !status.success() {
            return Ok(None);
        }

        let mut imgs: Vec<PathBuf> = fs::read_dir(&dir)?
            .filter_map(|e| e.ok().map(|x| x.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "png"))
            .collect();
        imgs.sort();

        let mut out = String::new();
        for (idx, img) in imgs.into_iter().enumerate() {
            if let Some(txt) = run_tesseract(&img) {
                out.push_str(&format!("[Page {} (OCR)]\n{txt}\n\n", idx + 1));
                if out.len() >= MAX_EXTRACTED_BYTES {
                    break;
                }
            }
        }
        Ok((!out.is_empty()).then(|| out.into_bytes()))
    }

    fn decode_hwp_para_text(bytes: &[u8], out: &mut String) {
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| u16::from_le_bytes(c))
            .collect();
        let mut plain: Vec<u16> = Vec::new();
        let flush_plain = |plain: &mut Vec<u16>, out: &mut String| {
            if !plain.is_empty() {
                for ch in char::decode_utf16(plain.drain(..)).flatten() {
                    out.push(ch);
                }
            }
        };
        let mut i = 0;
        while i < units.len() {
            let u = units[i];
            if u < 32 {
                flush_plain(&mut plain, out);
                match u {
                    9 => {
                        out.push('\t');
                        i += 8;
                    }
                    1..=8 | 11..=12 | 14..=23 => {
                        i += 8;
                    }
                    10 | 13 => {
                        out.push('\n');
                        i += 1;
                    }
                    24 => {
                        out.push('-');
                        i += 1;
                    }
                    30 | 31 => {
                        out.push(' ');
                        i += 1;
                    }
                    _ => i += 1,
                }
            } else {
                plain.push(u);
                i += 1;
            }
        }
        flush_plain(&mut plain, out);
    }

    fn extract_hwp(path: &Path) -> io::Result<Option<Vec<u8>>> {
        let mut cf = cfb::open(path)?;
        let mut hdr = Vec::new();
        if let Ok(stream) = cf.open_stream("FileHeader") {
            stream.take(512).read_to_end(&mut hdr)?;
        }
        if hdr.len() < 40 || &hdr[..17] != b"HWP Document File" {
            return Ok(None);
        }
        let flags = u32::from_le_bytes(hdr[36..40].try_into().unwrap());
        let compressed = (flags & 1) != 0;
        let encrypted = (flags & 2) != 0;

        let mut out = String::new();
        if !encrypted {
            let mut sections: Vec<(u32, PathBuf)> = Vec::new();
            for entry in cf.walk() {
                if !entry.is_stream() {
                    continue;
                }
                let normalized = entry.path().to_string_lossy().replace('\\', "/");
                let trimmed = normalized.trim_start_matches('/');
                let low = trimmed.to_ascii_lowercase();
                if let Some(rest) = low.strip_prefix("bodytext/section") {
                    if let Ok(num) = rest.parse::<u32>() {
                        sections.push((num, entry.path().to_path_buf()));
                    }
                }
            }
            sections.sort_by_key(|(num, _)| *num);

            for (_, stream_path) in sections {
                if out.len() >= MAX_EXTRACTED_BYTES {
                    break;
                }
                let Ok(stream) = cf.open_stream(&stream_path) else {
                    continue;
                };
                let mut raw = Vec::new();
                let read_res = if compressed {
                    DeflateDecoder::new(stream)
                        .take(MAX_EXTRACTED_BYTES as u64)
                        .read_to_end(&mut raw)
                } else {
                    stream
                        .take(MAX_EXTRACTED_BYTES as u64)
                        .read_to_end(&mut raw)
                };
                if read_res.is_err() {
                    continue;
                }
                let mut cur = 0usize;
                while cur + 4 <= raw.len() {
                    let h = u32::from_le_bytes(raw[cur..cur + 4].try_into().unwrap());
                    cur += 4;
                    let tag_id = h & 0x3FF;
                    let mut size = ((h >> 20) & 0xFFF) as usize;
                    if size == 0xFFF {
                        if cur + 4 > raw.len() {
                            break;
                        }
                        size = u32::from_le_bytes(raw[cur..cur + 4].try_into().unwrap()) as usize;
                        cur += 4;
                    }
                    if cur + size > raw.len() {
                        break;
                    }
                    if tag_id == 67 {
                        decode_hwp_para_text(&raw[cur..cur + size], &mut out);
                        if !out.ends_with('\n') {
                            out.push('\n');
                        }
                    }
                    cur += size;
                    if out.len() >= MAX_EXTRACTED_BYTES {
                        break;
                    }
                }
            }
        }

        if out.trim().is_empty() {
            if let Ok(stream) = cf.open_stream("PrvText") {
                let mut prv = Vec::new();
                if stream
                    .take(MAX_EXTRACTED_BYTES as u64)
                    .read_to_end(&mut prv)
                    .is_ok()
                {
                    let units = prv
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|&c| u16::from_le_bytes(c));
                    for ch in char::decode_utf16(units).flatten() {
                        if ch != '\0' {
                            out.push(ch);
                        }
                    }
                }
            }
        }

        Ok((!out.trim().is_empty()).then(|| out.into_bytes()))
    }

    fn extract_spreadsheet(path: &Path) -> io::Result<Option<Vec<u8>>> {
        let Ok(mut wb) = open_workbook_auto(path) else {
            return Ok(None);
        };
        let mut out = String::new();
        let names = wb.sheet_names();
        for name in names {
            let Ok(range) = wb.worksheet_range(&name) else {
                continue;
            };
            out.push_str(&format!("[Sheet: {name}]\n"));
            for row in range.rows() {
                if row.iter().all(|c| c.is_empty()) {
                    continue;
                }
                for (ci, cell) in row.iter().enumerate() {
                    if ci > 0 {
                        out.push('\t');
                    }
                    if !cell.is_empty() {
                        out.push_str(&cell.to_string());
                    }
                }
                out.push('\n');
                if out.len() >= MAX_EXTRACTED_BYTES {
                    return Ok(Some(out.into_bytes()));
                }
            }
            out.push('\n');
        }
        Ok((!out.is_empty()).then(|| out.into_bytes()))
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum XmlMode {
        OfficeRuns,
        Odt,
        Html,
    }

    fn push_entity(out: &mut String, name: &str) {
        match name {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "apos" => out.push('\''),
            "quot" => out.push('"'),
            "nbsp" => out.push(' '),
            _ => {}
        }
    }

    fn extract_xml_text(xml_bytes: &[u8], mode: XmlMode, out: &mut String) {
        let mut reader = XmlReader::from_reader(xml_bytes);
        reader.config_mut().trim_text(false);
        let mut buf = Vec::new();
        let mut in_text_run = false;
        let mut skip_depth = 0usize;
        let mut text_depth = 0usize;

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    let lname = e.local_name();
                    let tag = lname.as_ref();
                    match mode {
                        XmlMode::OfficeRuns => {
                            if tag == b"t" {
                                in_text_run = true;
                            } else if tag == b"br" {
                                out.push('\n');
                            }
                        }
                        XmlMode::Odt => {
                            if matches!(tag, b"p" | b"h") {
                                text_depth += 1;
                            }
                        }
                        XmlMode::Html => {
                            if matches!(tag, b"script" | b"style") {
                                skip_depth += 1;
                            } else if matches!(
                                tag,
                                b"p" | b"div"
                                    | b"h1"
                                    | b"h2"
                                    | b"h3"
                                    | b"h4"
                                    | b"h5"
                                    | b"h6"
                                    | b"li"
                                    | b"tr"
                                    | b"section"
                                    | b"article"
                                    | b"br"
                            ) && !out.ends_with('\n')
                            {
                                out.push('\n');
                            }
                        }
                    }
                }
                Ok(Event::Empty(e)) => {
                    let lname = e.local_name();
                    let tag = lname.as_ref();
                    if matches!(mode, XmlMode::OfficeRuns | XmlMode::Html) && tag == b"br" {
                        out.push('\n');
                    } else if mode == XmlMode::OfficeRuns && tag == b"tab" {
                        out.push('\t');
                    }
                }
                Ok(Event::End(e)) => {
                    let lname = e.local_name();
                    let tag = lname.as_ref();
                    match mode {
                        XmlMode::OfficeRuns => {
                            if tag == b"t" {
                                in_text_run = false;
                            } else if tag == b"tc" {
                                if !out.ends_with('\t') && !out.ends_with('\n') {
                                    out.push('\t');
                                }
                            } else if tag == b"p" && !out.ends_with('\n') {
                                out.push('\n');
                            }
                        }
                        XmlMode::Odt => {
                            if matches!(tag, b"p" | b"h") {
                                text_depth = text_depth.saturating_sub(1);
                                if !out.ends_with('\n') {
                                    out.push('\n');
                                }
                            }
                        }
                        XmlMode::Html => {
                            if matches!(tag, b"script" | b"style") {
                                skip_depth = skip_depth.saturating_sub(1);
                            } else if matches!(
                                tag,
                                b"p" | b"div"
                                    | b"h1"
                                    | b"h2"
                                    | b"h3"
                                    | b"h4"
                                    | b"h5"
                                    | b"h6"
                                    | b"li"
                                    | b"tr"
                                    | b"section"
                                    | b"article"
                            ) && !out.ends_with('\n')
                            {
                                out.push('\n');
                            }
                        }
                    }
                }
                Ok(Event::Text(t)) => {
                    let active = match mode {
                        XmlMode::OfficeRuns => in_text_run,
                        XmlMode::Odt => text_depth > 0,
                        XmlMode::Html => skip_depth == 0,
                    };
                    if active {
                        if let Ok(s) = t.decode() {
                            if mode == XmlMode::Html {
                                let trimmed = s.trim();
                                if !trimmed.is_empty() {
                                    if !out.is_empty() && !out.ends_with(['\n', ' ', '\t']) {
                                        out.push(' ');
                                    }
                                    out.push_str(trimmed);
                                }
                            } else {
                                out.push_str(&s);
                            }
                        }
                    }
                }
                Ok(Event::CData(t)) => {
                    let active = match mode {
                        XmlMode::OfficeRuns => in_text_run,
                        XmlMode::Odt => text_depth > 0,
                        XmlMode::Html => skip_depth == 0,
                    };
                    if active {
                        if let Ok(s) = t.decode() {
                            out.push_str(&s);
                        }
                    }
                }
                Ok(Event::GeneralRef(r)) => {
                    let active = match mode {
                        XmlMode::OfficeRuns => in_text_run,
                        XmlMode::Odt => text_depth > 0,
                        XmlMode::Html => skip_depth == 0,
                    };
                    if active {
                        if let Ok(Some(ch)) = r.resolve_char_ref() {
                            out.push(ch);
                        } else if let Ok(name) = r.decode() {
                            push_entity(out, &name);
                        }
                    }
                }
                Ok(Event::Eof) | Err(_) => break,
                _ => {}
            }
            buf.clear();
            if out.len() >= MAX_EXTRACTED_BYTES {
                break;
            }
        }
    }

    fn read_zip_entry(
        archive: &mut ZipArchive<File>,
        name: &str,
        limit: usize,
    ) -> io::Result<Vec<u8>> {
        let Ok(entry) = archive.by_name(name) else {
            return Ok(Vec::new());
        };
        let mut buf = Vec::new();
        entry.take(limit as u64).read_to_end(&mut buf)?;
        Ok(buf)
    }

    fn extract_docx(path: &Path) -> io::Result<Option<Vec<u8>>> {
        let mut archive = ZipArchive::new(File::open(path)?)?;
        let mut names: Vec<String> = (0..archive.len())
            .filter_map(|i| {
                let n = archive.name_for_index(i)?.to_string();
                let low = n.to_ascii_lowercase();
                (low == "word/document.xml"
                    || (low.starts_with("word/header") && low.ends_with(".xml"))
                    || (low.starts_with("word/footer") && low.ends_with(".xml")))
                .then_some(n)
            })
            .collect();
        names.sort_by_key(|n| (n != "word/document.xml", n.clone()));

        let mut out = String::new();
        for name in names {
            let rem = MAX_EXTRACTED_BYTES.saturating_sub(out.len());
            if rem == 0 {
                break;
            }
            let raw = read_zip_entry(&mut archive, &name, MAX_EXTRACTED_BYTES)?;
            extract_xml_text(&raw, XmlMode::OfficeRuns, &mut out);
        }
        Ok((!out.is_empty()).then(|| out.into_bytes()))
    }

    fn trailing_number(s: &str) -> u32 {
        let stem = s.rsplit('.').nth(1).unwrap_or(s);
        let digits: String = stem
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        let rev: String = digits.chars().rev().collect();
        rev.parse().unwrap_or(u32::MAX)
    }

    fn extract_pptx(path: &Path) -> io::Result<Option<Vec<u8>>> {
        let mut archive = ZipArchive::new(File::open(path)?)?;
        let mut slides: Vec<(u32, String)> = (0..archive.len())
            .filter_map(|i| {
                let n = archive.name_for_index(i)?.to_string();
                let low = n.to_ascii_lowercase();
                (low.starts_with("ppt/slides/slide") && low.ends_with(".xml"))
                    .then(|| (trailing_number(&low), n))
            })
            .collect();
        slides.sort();

        let mut out = String::new();
        for (idx, (num, name)) in slides.into_iter().enumerate() {
            if out.len() >= MAX_EXTRACTED_BYTES {
                break;
            }
            let raw = read_zip_entry(&mut archive, &name, MAX_EXTRACTED_BYTES)?;
            let mut slide_txt = String::new();
            extract_xml_text(&raw, XmlMode::OfficeRuns, &mut slide_txt);
            if !slide_txt.trim().is_empty() {
                let label = if num == u32::MAX {
                    idx + 1
                } else {
                    num as usize
                };
                out.push_str(&format!("[Slide {label}]\n{slide_txt}\n"));
            }
        }
        Ok((!out.is_empty()).then(|| out.into_bytes()))
    }

    fn extract_odt(path: &Path) -> io::Result<Option<Vec<u8>>> {
        let mut archive = ZipArchive::new(File::open(path)?)?;
        let raw = read_zip_entry(&mut archive, "content.xml", MAX_EXTRACTED_BYTES)?;
        if raw.is_empty() {
            return Ok(None);
        }
        let mut out = String::new();
        extract_xml_text(&raw, XmlMode::Odt, &mut out);
        Ok((!out.is_empty()).then(|| out.into_bytes()))
    }

    fn extract_hwpx(path: &Path) -> io::Result<Option<Vec<u8>>> {
        let mut archive = ZipArchive::new(File::open(path)?)?;
        let mut sections: Vec<(u32, String)> = (0..archive.len())
            .filter_map(|i| {
                let n = archive.name_for_index(i)?.to_string();
                let low = n.to_ascii_lowercase();
                (low.starts_with("contents/section") && low.ends_with(".xml"))
                    .then(|| (trailing_number(&low), n))
            })
            .collect();
        sections.sort();

        let mut out = String::new();
        for (_, name) in sections {
            if out.len() >= MAX_EXTRACTED_BYTES {
                break;
            }
            let raw = read_zip_entry(&mut archive, &name, MAX_EXTRACTED_BYTES)?;
            extract_xml_text(&raw, XmlMode::OfficeRuns, &mut out);
        }
        Ok((!out.is_empty()).then(|| out.into_bytes()))
    }

    fn extract_epub(path: &Path) -> io::Result<Option<Vec<u8>>> {
        let mut archive = ZipArchive::new(File::open(path)?)?;
        let mut chapters: Vec<String> = (0..archive.len())
            .filter_map(|i| {
                let n = archive.name_for_index(i)?.to_string();
                let low = n.to_ascii_lowercase();
                (low.ends_with(".xhtml") || low.ends_with(".html") || low.ends_with(".htm"))
                    .then_some(n)
            })
            .collect();
        chapters.sort();

        let mut out = String::new();
        for name in chapters {
            if out.len() >= MAX_EXTRACTED_BYTES {
                break;
            }
            let raw = read_zip_entry(&mut archive, &name, MAX_EXTRACTED_BYTES)?;
            extract_xml_text(&raw, XmlMode::Html, &mut out);
            if !out.ends_with('\n') {
                out.push('\n');
            }
        }
        Ok((!out.is_empty()).then(|| out.into_bytes()))
    }

    fn extract_gz(path: &Path) -> io::Result<Option<Vec<u8>>> {
        let f = File::open(path)?;
        let mut dec = GzDecoder::new(f).take(MAX_EXTRACTED_BYTES as u64);
        let mut buf = Vec::new();
        dec.read_to_end(&mut buf)?;
        Ok((!buf.is_empty() && !content::is_binary(&buf)).then_some(buf))
    }

    fn extract_tar_reader<R: Read>(r: R) -> io::Result<Option<Vec<u8>>> {
        let mut ar = Archive::new(r);
        let mut out = Vec::new();
        let Ok(entries) = ar.entries() else {
            return Ok(None);
        };
        for (idx, ent) in entries.enumerate() {
            if idx >= MAX_ARCHIVE_ENTRIES || out.len() >= MAX_EXTRACTED_BYTES {
                break;
            }
            let Ok(ent) = ent else { continue };
            if !ent.header().entry_type().is_file() {
                continue;
            }
            let name = ent
                .path()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            let rem = MAX_EXTRACTED_BYTES.saturating_sub(out.len());
            let mut body = Vec::new();
            if ent.take(rem as u64).read_to_end(&mut body).is_err() {
                continue;
            }
            if body.is_empty() || content::is_binary(&body) {
                continue;
            }
            out.extend_from_slice(format!("=== {name} ===\n").as_bytes());
            out.extend_from_slice(&body);
            if !body.ends_with(b"\n") {
                out.push(b'\n');
            }
        }
        Ok((!out.is_empty()).then_some(out))
    }

    fn extract_tar(path: &Path) -> io::Result<Option<Vec<u8>>> {
        extract_tar_reader(File::open(path)?)
    }

    fn extract_targz(path: &Path) -> io::Result<Option<Vec<u8>>> {
        extract_tar_reader(GzDecoder::new(File::open(path)?))
    }

    fn extract_zip(path: &Path) -> io::Result<Option<Vec<u8>>> {
        let mut archive = ZipArchive::new(File::open(path)?)?;
        let mut out = Vec::new();
        let limit = archive.len().min(MAX_ARCHIVE_ENTRIES);
        for i in 0..limit {
            if out.len() >= MAX_EXTRACTED_BYTES {
                break;
            }
            let Ok(entry) = archive.by_index(i) else {
                continue;
            };
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().to_string();
            let rem = MAX_EXTRACTED_BYTES.saturating_sub(out.len());
            let mut body = Vec::new();
            if entry.take(rem as u64).read_to_end(&mut body).is_err() {
                continue;
            }
            if body.is_empty() || content::is_binary(&body) {
                continue;
            }
            out.extend_from_slice(format!("=== {name} ===\n").as_bytes());
            out.extend_from_slice(&body);
            if !body.ends_with(b"\n") {
                out.push(b'\n');
            }
        }
        Ok((!out.is_empty()).then_some(out))
    }

    fn inspect_tar<R: Read>(r: R) -> Option<Vec<super::ContainerMeta>> {
        let mut ar = Archive::new(r);
        let entries = ar.entries().ok()?;
        let mut out = Vec::new();
        for (idx, ent) in entries.enumerate() {
            if idx >= MAX_ARCHIVE_ENTRIES {
                break;
            }
            let Ok(ent) = ent else { continue };
            if !ent.header().entry_type().is_file() {
                continue;
            }
            let name = ent
                .path()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            let sz = ent.header().size().unwrap_or(0);
            out.push(super::ContainerMeta {
                kind: "entry",
                name,
                detail: Some(format!("{sz} B")),
            });
        }
        Some(out)
    }

    pub fn inspect_container(path: &Path) -> Option<Vec<super::ContainerMeta>> {
        let kind = detect(path)?;
        match kind {
            Kind::Spreadsheet => {
                let mut wb = open_workbook_auto(path).ok()?;
                let mut out = Vec::new();
                for name in wb.sheet_names() {
                    let detail = wb.worksheet_range(&name).ok().map(|r| {
                        let (rows, cols) = r.get_size();
                        let hdr: Vec<String> = r
                            .rows()
                            .find(|row| row.iter().any(|c| !c.is_empty()))
                            .map(|row| {
                                row.iter()
                                    .take(6)
                                    .map(|c| c.to_string())
                                    .filter(|s| !s.is_empty())
                                    .collect()
                            })
                            .unwrap_or_default();
                        if hdr.is_empty() {
                            format!("{rows}x{cols}")
                        } else {
                            format!("{rows}x{cols} | {}", hdr.join(", "))
                        }
                    });
                    out.push(super::ContainerMeta {
                        kind: "sheet",
                        name,
                        detail,
                    });
                }
                Some(out)
            }
            Kind::Zip => {
                let mut archive = ZipArchive::new(File::open(path).ok()?).ok()?;
                let mut out = Vec::new();
                let limit = archive.len().min(MAX_ARCHIVE_ENTRIES);
                for i in 0..limit {
                    let Ok(ent) = archive.by_index(i) else {
                        continue;
                    };
                    if ent.is_dir() {
                        continue;
                    }
                    out.push(super::ContainerMeta {
                        kind: "entry",
                        name: ent.name().to_string(),
                        detail: Some(format!("{} B", ent.size())),
                    });
                }
                Some(out)
            }
            Kind::Tar => inspect_tar(File::open(path).ok()?),
            Kind::TarGz => inspect_tar(GzDecoder::new(File::open(path).ok()?)),
            _ => None,
        }
    }
}

#[cfg(not(feature = "adapters"))]
mod imp {
    use std::io;
    use std::path::Path;

    pub const ENABLED: bool = false;
    pub fn is_adapted_path(_path: &Path, _ocr: bool) -> bool {
        false
    }
    pub fn extract(_path: &Path, _ocr: bool) -> io::Result<Option<Vec<u8>>> {
        Ok(None)
    }
    pub fn inspect_container(_path: &Path) -> Option<Vec<super::ContainerMeta>> {
        None
    }
}

pub use imp::{extract, inspect_container, is_adapted_path, ENABLED};
