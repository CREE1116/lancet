use memmap2::Mmap;
use std::fs::File;
use std::io::Read;
use std::ops::Deref;
use std::path::Path;

const MMAP_MIN: u64 = 4 << 20;

pub enum Content {
    Owned(Vec<u8>),
    Mapped(Mmap),
}

impl Deref for Content {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            Content::Owned(v) => v,
            Content::Mapped(m) => m,
        }
    }
}

pub fn read(path: &Path) -> std::io::Result<Content> {
    let mut f = File::open(path)?;
    let len = f.metadata()?.len();
    if len >= MMAP_MIN {
        // SAFETY: read-only mapping; a concurrent truncation can SIGBUS, same
        // trade-off ripgrep makes for large files.
        let m = unsafe { Mmap::map(&f)? };
        Ok(Content::Mapped(m))
    } else {
        let mut v = Vec::with_capacity(len as usize + 1);
        f.read_to_end(&mut v)?;
        Ok(Content::Owned(v))
    }
}

/// Same heuristic family as grep/ripgrep: a NUL byte in the first 8 KiB means binary.
pub fn is_binary(b: &[u8]) -> bool {
    memchr::memchr(0, &b[..b.len().min(8192)]).is_some()
}

/// Load searchable UTF-8/text bytes for `path`, routing container/document
/// formats through `adapters` and returning `Ok(None)` for binary/unreadable
/// containers. Regular files keep the zero-overhead fast path.
pub fn load_searchable(path: &Path, ocr: bool) -> std::io::Result<Option<Content>> {
    if crate::adapters::is_adapted_path(path, ocr) {
        return Ok(crate::adapters::extract(path, ocr)?.map(Content::Owned));
    }
    let c = read(path)?;
    if is_binary(&c) {
        Ok(None)
    } else {
        Ok(Some(c))
    }
}
