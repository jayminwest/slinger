//! Just enough of the SQLite file format to read every row of one table:
//! the header, table b-tree pages, overflow pages and the record format.
//! Read only, from bytes in memory. Indexes, the freelist and WAL files are
//! never needed, so they are not read.

use super::damaged;
use crate::Result;

/// One column value. Text and blobs are both bytes; the importer knows which
/// it wants.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Field {
    Int(i64),
    Bytes(Vec<u8>),
    /// NULL or a real number: never needed by the importer.
    Other,
}

/// An SQLite database file held in memory.
pub(super) struct Db<'a> {
    bytes: &'a [u8],
    page_size: usize,
    usable: usize,
    pages: usize,
}

impl<'a> Db<'a> {
    pub(super) fn open(bytes: &'a [u8]) -> Result<Self> {
        if !bytes.starts_with(b"SQLite format 3\0") {
            return Err(damaged("it holds no SQLite database"));
        }
        let page_size = match be(bytes, 16, 2)? {
            1 => 65_536,
            n => usize::try_from(n).map_err(|_| damaged("its page size is wrong"))?,
        };
        let reserved = usize::from(byte(bytes, 20)?);
        let usable = page_size.saturating_sub(reserved);
        if !page_size.is_power_of_two() || page_size < 512 || usable < 480 {
            return Err(damaged("its database page size is wrong"));
        }
        if !matches!(be(bytes, 56, 4)?, 0 | 1) {
            return Err(damaged("its database is not UTF-8"));
        }
        Ok(Self { bytes, page_size, usable, pages: bytes.len() / page_size })
    }

    /// Every row of `table`, in rowid order.
    pub(super) fn rows(&self, table: &str) -> Result<Vec<Vec<Field>>> {
        let root = self
            .table(1)?
            .into_iter()
            .find_map(|row| match row.as_slice() {
                [Field::Bytes(kind), Field::Bytes(name), _, Field::Int(root), ..]
                    if kind == b"table" && name == table.as_bytes() =>
                {
                    u32::try_from(*root).ok()
                }
                _ => None,
            })
            .ok_or_else(|| damaged(&format!("its database has no {table} table")))?;
        self.table(root)
    }

    fn table(&self, root: u32) -> Result<Vec<Vec<Field>>> {
        let mut rows = Vec::new();
        let mut stack = vec![root];
        // A sound tree visits each page once; more visits means a loop.
        let mut visits = self.pages;
        while let Some(n) = stack.pop() {
            visits = visits.checked_sub(1).ok_or_else(|| damaged("its database loops"))?;
            let page = self.page(n)?;
            let head = if n == 1 { 100 } else { 0 };
            let cells = usize::try_from(be(page, head + 3, 2)?).unwrap_or(0);
            match byte(page, head)? {
                0x0D => {
                    for i in 0..cells {
                        let at = to_usize(be(page, head + 8 + 2 * i, 2)?);
                        rows.push(fields(&self.payload(page, at)?)?);
                    }
                }
                0x05 => {
                    stack.push(be32(page, head + 8)?);
                    for i in (0..cells).rev() {
                        let at = to_usize(be(page, head + 12 + 2 * i, 2)?);
                        stack.push(be32(page, at)?);
                    }
                }
                _ => return Err(damaged("its database has a page that is not a table")),
            }
        }
        Ok(rows)
    }

    fn page(&self, n: u32) -> Result<&'a [u8]> {
        let n = to_usize(n.into());
        if n == 0 || n > self.pages {
            return Err(damaged("its database points past its end"));
        }
        slice(self.bytes, (n - 1) * self.page_size, self.page_size)
    }

    /// A leaf cell's payload, gathered from its overflow pages if it spills.
    fn payload(&self, page: &[u8], at: usize) -> Result<Vec<u8>> {
        let (len, a) = varint(page, at)?;
        let (_rowid, b) = varint(page, at + a)?;
        let start = at + a + b;
        let len = to_usize(len);
        let u = self.usable;
        let max_local = u - 35;
        if len <= max_local {
            return Ok(slice(page, start, len)?.to_vec());
        }
        if len > self.bytes.len() {
            return Err(damaged("its database has a row longer than the file"));
        }
        let min_local = (u - 12) * 32 / 255 - 23;
        let k = min_local + (len - min_local) % (u - 4);
        let local = if k <= max_local { k } else { min_local };
        let mut out = slice(page, start, local)?.to_vec();
        let mut next = be32(page, start + local)?;
        while out.len() < len {
            let page = self.page(next)?;
            next = be32(page, 0)?;
            let take = (len - out.len()).min(u - 4);
            out.extend_from_slice(slice(page, 4, take)?);
        }
        Ok(out)
    }
}

/// The values in one record.
fn fields(record: &[u8]) -> Result<Vec<Field>> {
    let (head_len, mut at) = varint(record, 0)?;
    let head_len = to_usize(head_len);
    let mut body = head_len;
    let mut out = Vec::new();
    while at < head_len {
        let (kind, n) = varint(record, at)?;
        at += n;
        let (field, size) = match kind {
            1..=6 => {
                let size = [1, 2, 3, 4, 6, 8][to_usize(kind) - 1];
                let raw = be(record, body, size)?;
                // Sign-extend from `size` bytes.
                let shift = 64 - 8 * u32::try_from(size).unwrap_or(8);
                (Field::Int((raw << shift).cast_signed() >> shift), size)
            }
            8 | 9 => (Field::Int(i64::from(kind == 9)), 0),
            12.. => {
                let size = to_usize((kind - 12) / 2);
                (Field::Bytes(slice(record, body, size)?.to_vec()), size)
            }
            0 => (Field::Other, 0),
            7 => (Field::Other, 8),
            _ => return Err(damaged("its database has a record of an unknown kind")),
        };
        body += size;
        out.push(field);
    }
    Ok(out)
}

/// A variable-length integer and the number of bytes it took.
fn varint(bytes: &[u8], at: usize) -> Result<(u64, usize)> {
    let mut value = 0u64;
    for i in 0..8 {
        let b = byte(bytes, at + i)?;
        value = (value << 7) | u64::from(b & 0x7F);
        if b & 0x80 == 0 {
            return Ok((value, i + 1));
        }
    }
    // The ninth byte keeps all eight bits.
    Ok(((value << 8) | u64::from(byte(bytes, at + 8)?), 9))
}

fn to_usize(n: u64) -> usize {
    usize::try_from(n).unwrap_or(usize::MAX)
}

fn slice(bytes: &[u8], at: usize, len: usize) -> Result<&[u8]> {
    at.checked_add(len)
        .and_then(|end| bytes.get(at..end))
        .ok_or_else(|| damaged("its database ends early"))
}

fn byte(bytes: &[u8], at: usize) -> Result<u8> {
    Ok(slice(bytes, at, 1)?[0])
}

/// A big-endian unsigned integer of `len` bytes (at most 8).
fn be(bytes: &[u8], at: usize, len: usize) -> Result<u64> {
    Ok(slice(bytes, at, len)?.iter().fold(0, |n, &b| (n << 8) | u64::from(b)))
}

fn be32(bytes: &[u8], at: usize) -> Result<u32> {
    Ok(u32::try_from(be(bytes, at, 4)?).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varints_take_one_to_nine_bytes() {
        assert_eq!(varint(&[0x05], 0).unwrap(), (5, 1));
        assert_eq!(varint(&[0x81, 0x00], 0).unwrap(), (128, 2));
        assert_eq!(varint(&[0xFF; 9], 0).unwrap(), (u64::MAX, 9));
        assert!(varint(&[0x81], 0).is_err());
    }

    #[test]
    fn records_hold_ints_text_and_nulls() {
        // Header: length 6, then NULL, 1-byte int, 2-byte int, 0, 1, text of 2.
        let rec = [7, 0, 1, 2, 8, 9, 17, 0xFF, 0x01, 0x00, b'h', b'i'];
        assert_eq!(
            fields(&rec).unwrap(),
            [
                Field::Other,
                Field::Int(-1),
                Field::Int(256),
                Field::Int(0),
                Field::Int(1),
                Field::Bytes(b"hi".to_vec())
            ]
        );
        assert!(fields(&[2, 10]).is_err());
        assert!(fields(&[2, 17]).is_err());
    }

    #[test]
    fn a_file_that_is_not_a_database_is_invalid() {
        assert_eq!(Db::open(b"not sqlite").err().unwrap().code(), "invalid");
        let mut head = b"SQLite format 3\0".to_vec();
        head.resize(100, 0);
        head[16] = 0x03; // page size 768: not a power of two
        assert!(Db::open(&head).is_err());
        head[16] = 0x10; // 4096
        head[59] = 2; // UTF-16
        assert!(Db::open(&head).is_err());
        head[59] = 1;
        let db = Db::open(&head).unwrap();
        // The file is shorter than one page, so it has no pages to read.
        assert!(db.rows("documents").is_err());
    }

    #[test]
    fn a_page_that_is_not_a_table_or_loops_is_invalid() {
        let mut file = b"SQLite format 3\0".to_vec();
        file.resize(1024, 0);
        file[16] = 0x02; // 512-byte pages
        file[100] = 0x0A; // an index leaf, not a table
        let db = Db::open(&file).unwrap();
        assert!(db.rows("documents").is_err());
        // An interior page whose right child is itself.
        file[100] = 0x05;
        file[111] = 1;
        let db = Db::open(&file).unwrap();
        assert!(db.rows("documents").is_err());
    }
}
