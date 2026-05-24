//! sha256 verification against the mirror's `sha256sum.txt`.

use anyhow::{anyhow, Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::Path;

/// Hex-encoded sha256 of a file, streamed.
pub fn sha256_of(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Verify `file` matches the digest listed for its basename in a
/// coreutils-style `sha256sum.txt` body. Errors if there is no entry.
pub fn verify_against_sumfile(file: &Path, sumfile_body: &str) -> Result<()> {
    let name = file
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow!("non-utf8 filename: {}", file.display()))?;

    let expected = sumfile_body
        .lines()
        .find_map(|line| {
            let mut it = line.splitn(2, char::is_whitespace);
            let hash = it.next()?.trim();
            let fname = it.next()?.trim_start();
            (fname == name).then(|| hash.to_string())
        })
        .ok_or_else(|| anyhow!("no entry for {name} in sha256sum.txt"))?;

    let actual = sha256_of(file)?;
    if actual.eq_ignore_ascii_case(&expected) {
        tracing::debug!(%name, "sha256 ok");
        Ok(())
    } else {
        Err(anyhow!(
            "sha256 mismatch for {name}: expected {expected}, got {actual}"
        ))
    }
}
