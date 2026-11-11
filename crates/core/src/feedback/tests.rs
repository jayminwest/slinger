use super::*;

/// 2026-10-09 18:31:01 UTC.
const TS: i64 = 1_791_570_661;

fn entry(ts: &str, text: &str) -> Entry {
    note(text, Context::default(), ts.into(), "dev").unwrap()
}

fn file(lines: &[String]) -> String {
    lines.concat()
}

#[test]
fn times_are_local_rfc3339_with_the_offset() {
    assert_eq!(timestamp(TS, 0), "2026-10-09T18:31:01+00:00");
    assert_eq!(timestamp(TS, -25_200), "2026-10-09T11:31:01-07:00");
    assert_eq!(timestamp(TS, 19_800), "2026-10-10T00:01:01+05:30");
    assert_eq!(timestamp(0, -3600), "1969-12-31T23:00:00-01:00");
    assert_eq!(timestamp(951_782_400, 0), "2000-02-29T00:00:00+00:00");
}

#[test]
fn an_offset_is_what_date_prints() {
    assert_eq!(offset("-0800\n"), Some(-28_800));
    assert_eq!(offset("+0530"), Some(19_800));
    assert_eq!(offset("+0000"), Some(0));
    for bad in ["", "0800", "+08", "+08:00", "+2400", "+0060", "-08a0", "+08000"] {
        assert_eq!(offset(bad), None, "{bad:?}");
    }
}

#[test]
fn the_build_is_the_sha_dirty_or_dev() {
    let sha = "0123456789ABCDEF0123456789abcdef01234567";
    assert_eq!(build(Some(sha), Some("0")), sha.to_lowercase());
    assert_eq!(build(Some(sha), None), sha.to_lowercase());
    assert_eq!(build(Some(sha), Some("1")), format!("{}-dirty", sha.to_lowercase()));
    assert_eq!(build(Some("unknown"), Some("1")), "dev");
    assert_eq!(build(Some("abc"), None), "dev");
    assert_eq!(build(None, None), "dev");
}

#[test]
fn a_note_is_trimmed_and_leaves_out_what_it_has_not() {
    let context = Context {
        workspace: Some("notes".into()),
        canvas: Some(" ".into()),
        page: Some("Page 2".into()),
        tool: Some("lasso".into()),
        selected: Some(0),
    };
    let entry = note("  the lasso is slow\n", context, "t".into(), "abc1234").unwrap();
    assert_eq!(
        entry.line(),
        "{\"ts\":\"t\",\"text\":\"the lasso is slow\",\"build\":\"abc1234\",\
         \"workspace\":\"notes\",\"page\":\"Page 2\",\"tool\":\"lasso\"}\n"
    );
    let selected = Context { selected: Some(3), ..Context::default() };
    let entry = note("x", selected, "t".into(), "dev").unwrap();
    assert_eq!(entry.line(), "{\"ts\":\"t\",\"text\":\"x\",\"build\":\"dev\",\"selected\":3}\n");
    let err = note(" \n ", Context::default(), "t".into(), "dev").unwrap_err();
    assert_eq!((err.code(), err.hint().is_some()), ("invalid", true));
}

#[test]
fn entries_read_back_newest_first_skipping_lines_that_are_not_ones() {
    let (a, b) = (entry("a", "one\nline two"), entry("b", "two"));
    let text = file(&[a.line(), "not json\n".into(), "{\"x\":1}\n".into(), b.line()]);
    assert_eq!(read(&text, 5, false), [b.clone(), a]);
    assert_eq!(read(&text, 1, false), [b]);
    assert_eq!(read("", 5, true), []);
}

#[test]
fn a_resolution_hides_its_entry_unless_all_and_says_how() {
    let (a, b) = (entry("a", "one"), entry("b", "two"));
    let mut text = file(&[a.line(), b.line()]);
    let done = resolve(&text, "a", "c".into(), Some(" slinger-1234 ".into())).unwrap();
    assert_eq!(done.line(), "{\"resolves\":\"a\",\"ts\":\"c\",\"note\":\"slinger-1234\"}\n");
    text.push_str(&done.line());
    assert_eq!(read(&text, 5, false), std::slice::from_ref(&b));
    let resolved = Entry { resolved: Some(done.resolved), ..a };
    assert_eq!(read(&text, 5, true), [b, resolved.clone()]);
    assert!(!resolved.line().contains("resolved"), "an entry's line never holds it");

    let again = resolve(&text, "a", "d".into(), None).unwrap_err();
    assert_eq!(again.code(), "conflict");
    let missing = resolve(&text, "z", "d".into(), None).unwrap_err();
    assert_eq!(
        (missing.code(), missing.to_string()),
        ("not_found", "feedback at z not found".into())
    );
    let blank = resolve(&text, "b", "d".into(), Some("  ".into())).unwrap();
    assert_eq!(blank.line(), "{\"resolves\":\"b\",\"ts\":\"d\"}\n");
}

#[test]
fn a_note_reads_back_with_its_context() {
    let context = Context { canvas: Some("idea".into()), selected: Some(2), ..Context::default() };
    let saved = note("x", context, "t".into(), "dev").unwrap();
    assert_eq!(read(&saved.line(), 1, false), [saved]);
}
