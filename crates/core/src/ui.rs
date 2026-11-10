//! Agent UIs (docs/design/live-items.md, SPEC D19): a UI's code is one HTML
//! file stored as a blob, under a name, in versions that are never lost.
//!
//! This module owns the rules: what a name and the code may be, how the
//! versions are rebuilt from `ui_put` events, what a reference such as
//! `pick-slot@3` means and what an export is called. It does no IO: the
//! daemon hands it the events and stores what it returns.

use std::collections::BTreeMap;

use crate::record::{BlobId, Body, Event, UiPut};
use crate::{Error, Result};

/// The most bytes a UI's code may have. The bytes travel as a file, so this
/// bounds the record, not the control line: room for a page with its images
/// inline, and a stop for a file put by mistake.
pub const MAX_BYTES: usize = 1 << 20;

/// The most characters in a name.
const MAX_NAME: usize = 64;

const NAME_HINT: &str = "a name is 1 to 64 of a-z, 0-9 and -, starting with a letter";
const LIST_HINT: &str = "list them with 'slinger ui list'";

/// One version of a UI, as the record holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub name: String,
    pub version: u64,
    /// The code's blob id.
    pub hash: BlobId,
    /// The code's size.
    pub bytes: u64,
    /// Who put it: the event's source.
    pub source: String,
    /// When it was put, in milliseconds since the Unix epoch, UTC.
    pub time: u64,
    /// The `ui_put` event's number in the log.
    pub number: u64,
}

impl Version {
    /// The version a `ui_put` event records, or `None` for any other event.
    #[must_use]
    pub fn of(event: &Event) -> Option<Self> {
        let Body::UiPut(put) = &event.body else { return None };
        Some(Self {
            name: put.name.clone(),
            version: put.version,
            hash: put.hash,
            bytes: put.bytes,
            source: event.source.clone(),
            time: event.time,
            number: event.number.unwrap_or(0),
        })
    }

    /// The file `ui export` writes: `<name>.v<version>.html`.
    #[must_use]
    pub fn file_name(&self) -> String {
        format!("{}.v{}.html", self.name, self.version)
    }
}

/// What a put does: nothing, when the code is the name's latest version, or
/// a new version to record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Put {
    /// The name's latest version already holds these bytes.
    Same(Version),
    /// The payload of the `ui_put` event that makes the next version.
    New(UiPut),
}

/// Checks a name and its code before anything is stored.
///
/// # Errors
///
/// [`Error::Invalid`] for a name that is not 1 to 64 of `a-z 0-9 -`
/// starting with a letter, or code that is over [`MAX_BYTES`], is not UTF-8,
/// or holds a control character other than tab, LF and CR.
pub fn check(name: &str, code: &[u8]) -> Result<()> {
    check_name(name)?;
    let invalid = |message: String| Error::Invalid {
        message,
        hint: Some(
            "a UI is one self-contained HTML file: inline its CSS, scripts and images".into(),
        ),
    };
    if code.len() > MAX_BYTES {
        return Err(invalid(format!("UI code is {} bytes, over {MAX_BYTES}", code.len())));
    }
    let text =
        std::str::from_utf8(code).map_err(|e| invalid(format!("UI code is not UTF-8: {e}")))?;
    if let Some(c) = text.chars().find(|c| c.is_control() && !matches!(c, '\t' | '\n' | '\r')) {
        return Err(invalid(format!("UI code holds the control character {c:?}")));
    }
    Ok(())
}

fn check_name(name: &str) -> Result<()> {
    let ok = name.len() <= MAX_NAME
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if ok {
        return Ok(());
    }
    Err(Error::Invalid {
        message: format!("`{name}` is not a UI name"),
        hint: Some(NAME_HINT.into()),
    })
}

/// Every UI and its versions, rebuilt from the record's events.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    /// Each name's versions, in log order.
    names: BTreeMap<String, Vec<Version>>,
}

impl Catalog {
    /// The catalog `events` make, read in log order. Events of other kinds
    /// are skipped.
    #[must_use]
    pub fn new(events: &[Event]) -> Self {
        let mut names: BTreeMap<String, Vec<Version>> = BTreeMap::new();
        for version in events.iter().filter_map(Version::of) {
            names.entry(version.name.clone()).or_default().push(version);
        }
        Self { names }
    }

    /// The latest version of each name, by name.
    #[must_use]
    pub fn latest(&self) -> Vec<&Version> {
        self.names.values().filter_map(|versions| versions.last()).collect()
    }

    /// Every version of `name`, oldest first.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] for a string that is not a name, and
    /// [`Error::NotFound`] for a name with no versions.
    pub fn versions(&self, name: &str) -> Result<&[Version]> {
        check_name(name)?;
        self.names.get(name).map(Vec::as_slice).ok_or_else(|| not_found(name))
    }

    /// What putting the code `hash` of `bytes` under `name` does. Bytes
    /// equal to the latest version's add nothing; any others, an older
    /// version's included, make the next version.
    #[must_use]
    pub fn put(&self, name: &str, hash: BlobId, bytes: u64) -> Put {
        let last = self.names.get(name).and_then(|versions| versions.last());
        match last {
            Some(last) if last.hash == hash => Put::Same(last.clone()),
            _ => Put::New(UiPut {
                name: name.to_owned(),
                version: last.map_or(1, |v| v.version + 1),
                hash,
                bytes,
            }),
        }
    }

    /// The version `reference` names: `pick-slot` (its latest version),
    /// `pick-slot@3` (that version) or a blob id (the latest version that
    /// holds those bytes).
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] for a reference of none of those forms, and
    /// [`Error::NotFound`] when nothing matches it.
    pub fn resolve(&self, reference: &str) -> Result<&Version> {
        if reference.contains(':') {
            let hash: BlobId = reference.parse()?;
            return self
                .names
                .values()
                .flatten()
                .filter(|v| v.hash == hash)
                .max_by_key(|v| v.number)
                .ok_or_else(|| not_found(reference));
        }
        let Some((name, number)) = reference.split_once('@') else {
            return self.versions(reference)?.last().ok_or_else(|| not_found(reference));
        };
        let number: u64 = number.parse().map_err(|_| Error::Invalid {
            message: format!("`{reference}` is not a UI reference"),
            hint: Some("use <name>, <name>@<version> or a blob id".into()),
        })?;
        self.versions(name)?
            .iter()
            .find(|v| v.version == number)
            .ok_or_else(|| not_found(reference))
    }
}

fn not_found(what: &str) -> Error {
    Error::NotFound { what: format!("UI {what}"), hint: Some(LIST_HINT.into()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::Tags;

    fn put(time: u64, name: &str, version: u64, code: &[u8]) -> Event {
        let body = UiPut { name: name.into(), version, hash: BlobId::of(code), bytes: 0 };
        let mut event = Event::new(time, "scout", Tags::on("m"), Body::UiPut(body));
        event.number = Some(time);
        event
    }

    fn catalog() -> Catalog {
        let other =
            Event::new(2, "x", Tags::on("m"), Body::Other { kind: "k".into(), payload: 0.into() });
        Catalog::new(&[
            put(1, "pick", 1, b"a"),
            other,
            put(3, "zoo", 1, b"z"),
            put(4, "pick", 2, b"b"),
            put(5, "pick", 3, b"a"),
        ])
    }

    #[test]
    fn names_follow_the_rule() {
        for good in ["a", "pick-slot", "a9-", &"a".repeat(64)] {
            assert!(check(good, b"").is_ok(), "{good}");
        }
        for bad in ["", "9a", "-a", "Pick", "a_b", "a b", "é", &"a".repeat(65)] {
            let err = check(bad, b"").unwrap_err();
            assert_eq!((err.code(), err.hint()), ("invalid", Some(NAME_HINT)), "{bad}");
        }
    }

    #[test]
    fn code_is_bounded_text() {
        assert!(check("a", b"<p>\tok\r\n</p>").is_ok());
        assert!(check("a", &vec![b' '; MAX_BYTES]).is_ok());
        for bad in [&vec![b' '; MAX_BYTES + 1][..], b"\xff", b"a\0b", b"\x1b[0m"] {
            let err = check("a", bad).unwrap_err();
            assert_eq!(err.code(), "invalid");
            assert!(err.hint().is_some());
        }
    }

    #[test]
    fn versions_are_rebuilt_per_name_in_log_order() {
        let catalog = catalog();
        let latest: Vec<_> =
            catalog.latest().iter().map(|v| (v.name.as_str(), v.version)).collect();
        assert_eq!(latest, [("pick", 3), ("zoo", 1)]);
        let pick = catalog.versions("pick").unwrap();
        assert_eq!(pick.iter().map(|v| v.time).collect::<Vec<_>>(), [1, 4, 5]);
        assert_eq!(
            (pick[0].source.as_str(), pick[0].file_name()),
            ("scout", "pick.v1.html".into())
        );
        assert_eq!(catalog.versions("nope").unwrap_err().code(), "not_found");
        assert_eq!(catalog.versions("No").unwrap_err().code(), "invalid");
        assert_eq!(Catalog::new(&[]).latest().len(), 0);
    }

    #[test]
    fn a_put_of_the_latest_bytes_adds_nothing_and_any_other_adds_one() {
        let catalog = catalog();
        let Put::Same(same) = catalog.put("pick", BlobId::of(b"a"), 1) else { panic!() };
        assert_eq!(same.version, 3);
        let older = catalog.put("pick", BlobId::of(b"b"), 1);
        let new = UiPut { name: "pick".into(), version: 4, hash: BlobId::of(b"b"), bytes: 1 };
        assert_eq!(older, Put::New(new));
        let Put::New(first) = catalog.put("fresh", BlobId::of(b"a"), 1) else { panic!() };
        assert_eq!(first.version, 1);
    }

    #[test]
    fn references_name_the_latest_a_number_or_a_hash() {
        let catalog = catalog();
        assert_eq!(catalog.resolve("pick").unwrap().version, 3);
        assert_eq!(catalog.resolve("pick@2").unwrap().hash, BlobId::of(b"b"));
        let by_hash = catalog.resolve(&BlobId::of(b"a").to_string()).unwrap();
        assert_eq!((by_hash.name.as_str(), by_hash.version), ("pick", 3));
        for (reference, code) in [
            ("pick@4", "not_found"),
            ("nope", "not_found"),
            (&BlobId::of(b"c").to_string(), "not_found"),
            ("pick@two", "invalid"),
            ("pick@", "invalid"),
            ("Pick@1", "invalid"),
            ("sha256:00", "invalid"),
        ] {
            let err = catalog.resolve(reference).unwrap_err();
            assert_eq!(err.code(), code, "{reference}");
            assert!(err.hint().is_some(), "{reference}");
        }
    }
}
