//! Just enough of the zip format to find one stored (uncompressed) entry.
//! tldraw stores its `db.sqlite` without compression, so no inflater is needed.

use super::damaged;
use crate::Result;

const END: u32 = 0x0605_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const LOCAL: u32 = 0x0403_4b50;

/// Whether the bytes start like a zip file.
pub(super) fn is_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
}

/// The bytes of the entry called `name`.
pub(super) fn entry<'a>(zip: &'a [u8], name: &str) -> Result<&'a [u8]> {
    let end = find_end(zip)?;
    let count = u16_at(zip, end + 10)?;
    let mut at = u32_at(zip, end + 16)? as usize;
    for _ in 0..count {
        if u32_at(zip, at)? != CENTRAL {
            return Err(damaged("its zip directory is damaged"));
        }
        let method = u16_at(zip, at + 10)?;
        let size = u32_at(zip, at + 20)?;
        let name_len = u16_at(zip, at + 28)? as usize;
        let extra_len = u16_at(zip, at + 30)? as usize;
        let comment_len = u16_at(zip, at + 32)? as usize;
        let local = u32_at(zip, at + 42)? as usize;
        if slice(zip, at + 46, name_len)? == name.as_bytes() {
            if method != 0 || size == u32::MAX {
                return Err(damaged(&format!("its {name} is compressed or too large to read")));
            }
            return local_data(zip, local, size as usize);
        }
        at += 46 + name_len + extra_len + comment_len;
    }
    Err(damaged(&format!("it holds no {name}")))
}

fn local_data(zip: &[u8], at: usize, size: usize) -> Result<&[u8]> {
    if u32_at(zip, at)? != LOCAL {
        return Err(damaged("its zip entry is damaged"));
    }
    let name_len = u16_at(zip, at + 26)? as usize;
    let extra_len = u16_at(zip, at + 28)? as usize;
    slice(zip, at + 30 + name_len + extra_len, size)
}

/// The end-of-directory record sits in the last 22 bytes plus a comment of up
/// to 64 KiB.
fn find_end(zip: &[u8]) -> Result<usize> {
    let last = zip.len().checked_sub(22).ok_or_else(|| damaged("it is too short for a zip"))?;
    let first = last.saturating_sub(usize::from(u16::MAX));
    (first..=last)
        .rev()
        .find(|&i| u32_at(zip, i).is_ok_and(|sig| sig == END))
        .ok_or_else(|| damaged("it has no zip directory"))
}

fn slice(bytes: &[u8], at: usize, len: usize) -> Result<&[u8]> {
    at.checked_add(len).and_then(|end| bytes.get(at..end)).ok_or_else(|| damaged("it ends early"))
}

fn u16_at(bytes: &[u8], at: usize) -> Result<u16> {
    let b = slice(bytes, at, 2)?;
    Ok(u16::from_le_bytes([b[0], b[1]]))
}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32> {
    let b = slice(bytes, at, 4)?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// A zip with one stored entry per `(name, data, method)`.
    pub(in crate::tldraw) fn zip(entries: &[(&str, &[u8], u16)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut dir = Vec::new();
        for (name, data, method) in entries {
            let local = u32::try_from(out.len()).unwrap();
            let size = u32::try_from(data.len()).unwrap();
            let name_len = u16::try_from(name.len()).unwrap();
            out.extend(LOCAL.to_le_bytes());
            out.extend([0; 4]);
            out.extend(method.to_le_bytes());
            out.extend([0; 8]);
            out.extend(size.to_le_bytes());
            out.extend(size.to_le_bytes());
            out.extend(name_len.to_le_bytes());
            out.extend([0; 2]);
            out.extend(name.as_bytes());
            out.extend(*data);
            dir.extend(CENTRAL.to_le_bytes());
            dir.extend([0; 6]);
            dir.extend(method.to_le_bytes());
            dir.extend([0; 8]);
            dir.extend(size.to_le_bytes());
            dir.extend(size.to_le_bytes());
            dir.extend(name_len.to_le_bytes());
            dir.extend([0; 12]);
            dir.extend(local.to_le_bytes());
            dir.extend(name.as_bytes());
        }
        let at = u32::try_from(out.len()).unwrap();
        let count = u16::try_from(entries.len()).unwrap();
        let dir_len = u32::try_from(dir.len()).unwrap();
        out.extend(dir);
        out.extend(END.to_le_bytes());
        out.extend([0; 4]);
        out.extend(count.to_le_bytes());
        out.extend(count.to_le_bytes());
        out.extend(dir_len.to_le_bytes());
        out.extend(at.to_le_bytes());
        out.extend([0; 2]);
        out
    }

    #[test]
    fn finds_a_stored_entry_by_name() {
        let z = zip(&[("a.txt", b"first", 0), ("db.sqlite", b"second", 0)]);
        assert!(is_zip(&z));
        assert_eq!(entry(&z, "db.sqlite").unwrap(), b"second");
        assert_eq!(entry(&z, "a.txt").unwrap(), b"first");
    }

    #[test]
    fn a_missing_compressed_or_cut_entry_is_invalid() {
        let z = zip(&[("db.sqlite", b"data", 8)]);
        assert_eq!(entry(&z, "db.sqlite").unwrap_err().code(), "invalid");
        assert_eq!(entry(&z, "other").unwrap_err().code(), "invalid");
        let z = zip(&[("db.sqlite", b"data", 0)]);
        assert_eq!(entry(&z[..z.len() - 30], "db.sqlite").unwrap_err().code(), "invalid");
        assert_eq!(entry(b"PK\x03\x04", "db.sqlite").unwrap_err().code(), "invalid");
        let mut bad = z.clone();
        bad[0] = 0;
        assert_eq!(entry(&bad, "db.sqlite").unwrap_err().code(), "invalid");
        let dir = z.len() - 22 - 55;
        let mut bad = z;
        bad[dir] = 0;
        assert_eq!(entry(&bad, "db.sqlite").unwrap_err().code(), "invalid");
    }
}
