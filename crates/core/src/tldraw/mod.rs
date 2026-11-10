//! Reads a tldraw file into a canvas (SPEC D7). Import only: Slinger never
//! writes tldraw files.
//!
//! The interface is one function, [`import`]: bytes in, a canvas and a report
//! out. It reads both kinds of tldraw file: the current one (a zip holding an
//! SQLite database of records) and the older `.tldr` (JSON with a `records`
//! list). The records map onto canvas items:
//!
//! - draw shapes become strokes at a constant width (pressure is dropped, D6);
//! - text becomes text; a rectangle becomes a box, plus text for its label;
//! - arrows become arrows, and an arrow bound to a shape points at its item;
//! - pages become pages, in tldraw's page order, with their names.
//!
//! Colour is kept (as Slinger's own value for each tldraw colour name), and
//! items are added in tldraw's drawing order, so a later item draws on top.
//! Children of groups and frames are placed in page space; the group itself
//! has no item. A shape with no mapping is skipped and counted in the
//! [`Report`]. Built from tldraw's file format only (SPEC K3).

mod map;
mod path;
mod sqlite;
mod zip;

use std::collections::BTreeMap;

use serde_json::Value;

use crate::canvas::{Canvas, Time};
use crate::{Error, Result};

/// A canvas read from a tldraw file, and what the import did.
#[derive(Debug, Clone, PartialEq)]
pub struct Import {
    pub canvas: Canvas,
    pub report: Report,
}

/// What an import made and what it skipped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub pages: usize,
    pub strokes: usize,
    /// Text items, including rectangle labels.
    pub texts: usize,
    pub boxes: usize,
    pub arrows: usize,
    /// Shapes with no item, by kind: the tldraw shape type (`line`, `group`,
    /// `geo:ellipse`), `<type> (damaged)` for a shape whose data cannot be
    /// read, and `orphan` for a shape on no page.
    pub skipped: BTreeMap<String, usize>,
}

/// Reads a tldraw file. Every item is the user's, made at `at`.
///
/// # Errors
///
/// [`Error::Invalid`] when the bytes are not a tldraw file, or the file is too
/// damaged to read. A damaged shape inside a sound file is skipped and counted
/// instead.
pub fn import(bytes: &[u8], at: Time) -> Result<Import> {
    Ok(map::build(&records(bytes)?, at))
}

/// Every record in the file, as JSON.
fn records(bytes: &[u8]) -> Result<Vec<Value>> {
    if zip::is_zip(bytes) {
        let db = sqlite::Db::open(zip::entry(bytes, "db.sqlite")?)?;
        return db
            .rows("documents")?
            .into_iter()
            .map(|row| match row.get(1) {
                Some(sqlite::Field::Bytes(state)) => {
                    serde_json::from_slice(state).map_err(|_| damaged("a record is not JSON"))
                }
                _ => Err(damaged("a record has no state")),
            })
            .collect();
    }
    let file: Value =
        serde_json::from_slice(bytes).map_err(|_| damaged("it is not a zip or JSON"))?;
    match file.get("records") {
        Some(Value::Array(records)) => Ok(records.clone()),
        _ => Err(damaged("its JSON has no records list")),
    }
}

/// The error for a file this module cannot read.
fn damaged(why: &str) -> Error {
    Error::Invalid {
        message: format!("not a readable tldraw file: {why}"),
        hint: Some("give a .tldraw or .tldr file saved by tldraw".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_that_are_not_a_tldraw_file_are_invalid() {
        for bytes in [&b""[..], b"hello", b"{\"pages\": []}", b"PK\x03\x04 cut short"] {
            let err = import(bytes, Time(0)).unwrap_err();
            assert_eq!(err.code(), "invalid", "{bytes:?}");
            assert!(err.hint().is_some());
        }
    }

    #[test]
    fn a_zip_without_a_database_or_with_bad_rows_is_invalid() {
        let z = zip::tests::zip(&[("db.sqlite", b"not a database", 0)]);
        assert_eq!(import(&z, Time(0)).unwrap_err().code(), "invalid");
        let z = zip::tests::zip(&[("metadata.json", b"{}", 0)]);
        assert_eq!(import(&z, Time(0)).unwrap_err().code(), "invalid");
    }
}
