use anyhow::{Context, Result, anyhow, bail};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const MAX_FILE_BYTES: usize = 20 * 1024 * 1024;
const BOM: &[u8] = b"\xef\xbb\xbf";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Newline {
    Lf,
    CrLf,
    Cr,
}

impl Newline {
    fn as_str(self) -> &'static str {
        match self {
            Newline::Lf => "\n",
            Newline::CrLf => "\r\n",
            Newline::Cr => "\r",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub path: PathBuf,
    pub digest: [u8; 32],
    pub newline: Newline,
    pub bom: bool,
}

pub fn digest(raw: &[u8]) -> [u8; 32] {
    Sha256::digest(raw).into()
}

pub fn read_document(path: &Path) -> Result<(String, Snapshot)> {
    let path = canonical(path);
    let raw = fs::read(&path).with_context(|| format!("Could not read {}", path.display()))?;
    if raw.len() > MAX_FILE_BYTES {
        bail!("File exceeds the {} MiB editing limit.", MAX_FILE_BYTES / (1024 * 1024));
    }
    if raw.contains(&0) {
        bail!("This looks like a binary file. Glyph only opens UTF-8 text.");
    }
    let bom = raw.starts_with(BOM);
    let body = if bom { &raw[BOM.len()..] } else { &raw[..] };
    let text = std::str::from_utf8(body).map_err(|_| anyhow!("The file is not valid UTF-8 text."))?;
    let crlf = text.matches("\r\n").count();
    let lf = text.matches('\n').count() - crlf;
    let cr = text.matches('\r').count() - crlf;
    let newline = if crlf > 0 && crlf >= lf.max(cr) {
        Newline::CrLf
    } else if cr > lf {
        Newline::Cr
    } else {
        Newline::Lf
    };
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    Ok((normalized, Snapshot { path, digest: digest(&raw), newline, bom }))
}

pub fn encode(text: &str, newline: Newline, bom: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + 3);
    if bom {
        out.extend_from_slice(BOM);
    }
    match newline {
        Newline::Lf => out.extend_from_slice(text.as_bytes()),
        other => out.extend_from_slice(text.replace('\n', other.as_str()).as_bytes()),
    }
    out
}

pub fn atomic_write(path: &Path, data: &[u8], expected: Option<[u8; 32]>) -> Result<[u8; 32]> {
    let parent = path
        .parent()
        .filter(|p| p.is_dir())
        .ok_or_else(|| anyhow!("Parent directory does not exist: {}", path.display()))?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("document");
    let temp = parent.join(format!(".{name}.{}.tmp", std::process::id()));
    let result = write_and_swap(path, &temp, data, expected);
    let _ = fs::remove_file(&temp);
    result
}

fn write_and_swap(path: &Path, temp: &Path, data: &[u8], expected: Option<[u8; 32]>) -> Result<[u8; 32]> {
    let mut file = fs::File::create(temp)?;
    file.write_all(data)?;
    file.sync_all()?;
    drop(file);
    if let Some(expected) = expected {
        match fs::read(path) {
            Ok(current) if digest(&current) != expected => bail!("The file was changed outside Glyph."),
            Err(_) => bail!("The file was removed or renamed outside Glyph."),
            _ => {}
        }
    }
    fs::rename(temp, path)?;
    Ok(digest(data))
}

pub fn save_document(snapshot: &Snapshot, text: &str, force: bool) -> Result<Snapshot> {
    let raw = encode(text, snapshot.newline, snapshot.bom);
    let digest = atomic_write(&snapshot.path, &raw, (!force).then_some(snapshot.digest))?;
    Ok(Snapshot { digest, ..snapshot.clone() })
}

pub fn save_new(path: &Path, text: &str) -> Result<Snapshot> {
    let newline = if cfg!(windows) { Newline::CrLf } else { Newline::Lf };
    let raw = encode(text, newline, false);
    let digest = atomic_write(path, &raw, None)?;
    Ok(Snapshot { path: canonical(path), digest, newline, bom: false })
}

pub fn canonical(path: &Path) -> PathBuf {
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(stripped) => PathBuf::from(stripped),
        None => path,
    }
}

pub fn is_text_document(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(),
        Some("md" | "markdown" | "mdown" | "mkd" | "mkdn" | "mdwn" | "txt" | "mmd" | "mermaid" | "text")
    )
}
