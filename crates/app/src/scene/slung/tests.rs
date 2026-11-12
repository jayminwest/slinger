use super::words::wrap;
use super::*;
use crate::config::Rgb;

const T: Theme = Theme {
    background: Rgb { r: 0, g: 0, b: 0 },
    text: Rgb { r: 1, g: 1, b: 1 },
    muted: Rgb { r: 2, g: 2, b: 2 },
    error: Rgb { r: 3, g: 3, b: 3 },
    focus: Rgb { r: 4, g: 4, b: 4 },
};

const PANE: Rect = Rect { x: 200.0, y: 0.0, width: 800.0, height: 600.0 };
/// Where the sling was made, in canvas units.
const AT: Rect = Rect { x: 100.0, y: 100.0, width: 200.0, height: 50.0 };

fn view(focused: bool) -> View<'static> {
    View { frame: PANE, canvas: "notes", page: "p1", viewport: Viewport::default(), focused }
}

fn working(pending: u32) -> Progress {
    Progress { running: true, pending, blocked: None, answer: None }
}

fn answered(answer: Option<&str>) -> Progress {
    Progress { running: true, pending: 0, blocked: None, answer: answer.map(str::to_string) }
}

/// A sling of page p1 of notes made at [`AT`] at `now`, gone to scout.
fn out(now: Instant) -> Slung {
    let mut slung = Slung::default();
    slung.aim(("notes", "p1"), Some(AT), ItemId(5), now);
    assert_eq!(slung.marks(&T, &[view(true)], now), Marks::default(), "the entry's line says it");
    assert!(slung.went("scout", now));
    slung
}

/// The pill's text and lines, top first.
fn said(slung: &Slung, views: &[View], now: Instant) -> Vec<String> {
    slung.marks(&T, views, now).labels.into_iter().map(|l| l.text).collect()
}

fn agent_item(id: u64, frame: Rect) -> (ItemId, Rect, Option<&'static str>) {
    (ItemId(id), frame, Some("agent:scout"))
}

#[test]
fn a_sling_says_it_went_and_counts_while_the_agent_works() {
    let now = Instant::now();
    let mut slung = out(now);
    assert_eq!(slung.flash().as_deref(), Some("Slung to scout"));
    assert_eq!(slung.flash(), None, "once");
    assert_eq!(said(&slung, &[view(true)], now), ["● scout working · 0s"]);
    let pill = slung.marks(&T, &[view(true)], now).labels[0].clone();
    assert_eq!(pill.frame.y, PANE.y + AT.y + AT.height + GAP, "under where it was made");
    assert_eq!(pill.frame.x, PANE.x + AT.x);
    assert!(!slung.heard(&working(1), now), "still working");
    assert!(slung.tick(now + Duration::from_secs(4)));
    assert!(!slung.tick(now + Duration::from_millis(4500)), "the same second");
    assert_eq!(said(&slung, &[view(true)], now + Duration::from_secs(4)), ["● scout working · 4s"]);
    assert_eq!(
        said(&slung, &[view(true)], now + Duration::from_secs(75)),
        ["● scout working · 1m"]
    );
    assert_eq!(slung.tapped(&[view(true)]), Tapped::Nothing, "a tap while it works");
}

#[test]
fn a_blocked_agent_says_what_it_waits_for() {
    let now = Instant::now();
    let mut slung = out(now);
    let blocked = Progress { blocked: Some("which key?".into()), ..working(1) };
    assert!(slung.heard(&blocked, now));
    let marks = slung.marks(&T, &[view(true)], now);
    assert_eq!(marks.labels[0].text, "■ scout blocked: which key?");
    assert_eq!(marks.labels[0].colour, T.error);
    assert!(slung.heard(&working(1), now), "back to work");
}

#[test]
fn an_answer_with_items_counts_them_and_a_tap_pans_to_those_out_of_sight() {
    let now = Instant::now();
    let mut slung = out(now);
    assert_eq!(slung.watching(), Some(("notes", "p1")));
    let mine = agent_item(7, Rect { x: 2000.0, y: 120.0, width: 100.0, height: 40.0 });
    let others = [
        (ItemId(4), AT, Some("agent:scout")),
        (ItemId(6), AT, Some("user")),
        (ItemId(8), AT, Some("agent:bo")),
    ];
    assert!(slung.landed(("notes", "p1"), others.into_iter().chain([mine])));
    assert!(!slung.landed(("notes", "p2"), [mine]), "another page");
    slung.flash();
    assert!(slung.heard(&answered(Some("done")), now));
    assert_eq!(slung.flash().as_deref(), Some("scout answered"));
    assert_eq!(said(&slung, &[view(true)], now), ["✓ scout answered · 1 new item →"]);
    let Tapped::Pan(0, dx, _) = slung.tapped(&[view(true)]) else { panic!("a pan") };
    assert!(dx < -1000.0, "toward the item: {dx}");

    let near = agent_item(9, Rect { x: 150.0, y: 300.0, width: 100.0, height: 40.0 });
    slung.landed(("notes", "p1"), [mine, near]);
    assert_eq!(said(&slung, &[view(true)], now), ["✓ scout answered · 2 new items"]);
    assert_eq!(slung.tapped(&[view(true)]), Tapped::Changed, "in sight: the tap clears it");
    assert_eq!(said(&slung, &[view(true)], now), Vec::<String>::new());
}

#[test]
fn an_answer_in_words_opens_under_the_pill_and_a_tap_closes_it() {
    let now = Instant::now();
    let mut slung = out(now);
    assert!(slung.heard(&answered(Some("It is two.\nAnd three.")), now));
    assert_eq!(slung.flash().as_deref(), Some("scout answered in text"));
    let marks = slung.marks(&T, &[view(true)], now);
    assert_eq!(marks.labels[0].text, "✓ scout answered in text · tap to read");
    assert_eq!(marks.panel, None);
    let (target, tap) = marks.target.unwrap();
    assert_eq!((target, tap), (marks.labels[0].frame, Tap::Slung));
    assert_eq!(slung.tapped(&[view(true)]), Tapped::Changed);
    let marks = slung.marks(&T, &[view(true)], now);
    let panel = marks.panel.expect("the answer");
    let texts = panel.lines.iter().map(|l| l.label.text.as_str()).filter(|t| !t.is_empty());
    let texts: Vec<&str> = texts.collect();
    assert_eq!(texts, ["It is two.", "And three.", "tap to close"]);
    assert!(marks.target.unwrap().0.height > HIGH, "the answer takes the tap too");
    assert!(!slung.tick(now + 2 * CLEAR), "an open answer stays");
    assert_eq!(slung.tapped(&[view(true)]), Tapped::Changed);
    assert_eq!(slung.marks(&T, &[view(true)], now), Marks::default());
}

#[test]
fn a_long_answer_shows_its_start_and_where_the_rest_is() {
    let lines = page(&"word ".repeat(400), PANEL, "scout");
    assert_eq!(lines.len(), MOST + 1);
    assert_eq!(lines[MOST - 1], "… the rest: slinger session show scout");
    assert_eq!(lines[MOST], "tap to close");
}

#[test]
fn an_agent_done_with_no_answer_says_so_and_it_all_clears_in_time() {
    let now = Instant::now();
    let mut slung = out(now);
    assert!(slung.heard(&answered(None), now));
    assert_eq!(slung.flash().as_deref(), Some("scout is done"));
    assert_eq!(said(&slung, &[view(true)], now), ["✓ scout is done, with nothing on the canvas"]);
    assert!(!slung.tick(now + CLEAR / 2));
    assert!(slung.tick(now + CLEAR), "nothing stays for good");
    assert_eq!(slung.marks(&T, &[view(true)], now), Marks::default());
}

#[test]
fn an_agent_that_stops_says_how_to_start_it() {
    let now = Instant::now();
    let mut slung = out(now);
    assert!(slung.heard(&Progress { running: false, ..working(1) }, now));
    let marks = slung.marks(&T, &[view(true)], now);
    let texts: Vec<&str> = marks.labels.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(
        texts,
        ["✗ scout stopped before it answered", "start it with 'slinger session start <WORKSPACE>'"]
    );
    assert_eq!(marks.labels[0].colour, T.error);
    assert!(slung.tick(now + CLEAR));
}

#[test]
fn a_sling_that_did_not_go_says_why_unless_its_entry_does() {
    let now = Instant::now();
    let problem = Problem { message: "scout is not running".into(), hint: Some("start it".into()) };
    let mut slung = Slung::default();
    slung.aim(("notes", "p1"), Some(AT), ItemId(5), now);
    assert!(slung.failed(problem.clone(), now));
    assert_eq!(said(&slung, &[view(true)], now), ["✗ scout is not running", "start it"]);
    assert_eq!(slung.tapped(&[view(true)]), Tapped::Changed, "a tap clears it");
    assert!(!slung.failed(problem, now), "nothing out");

    slung.aim(("notes", "p1"), Some(AT), ItemId(5), now);
    slung.forget();
    assert_eq!(slung.watching(), None);
    assert_eq!(slung.marks(&T, &[view(true)], now), Marks::default());
}

#[test]
fn a_lost_daemon_while_the_agent_works_says_so() {
    let now = Instant::now();
    let mut slung = out(now);
    let lost = Problem { message: "no daemon".into(), hint: None };
    assert!(slung.failed(lost, now));
    assert_eq!(said(&slung, &[view(true)], now), ["✗ no daemon"]);
    assert!(!slung.heard(&answered(None), now), "it no longer follows");
}

#[test]
fn a_page_sling_pins_the_pill_and_it_stays_inside_its_pane() {
    let now = Instant::now();
    let mut slung = Slung::default();
    slung.aim(("notes", "p1"), None, ItemId(0), now);
    slung.went("scout", now);
    let pill = slung.marks(&T, &[view(true)], now).labels[0].frame;
    assert_eq!((pill.x, pill.y), (PANE.x + GAP, PANE.y + GAP), "the pane's top left");

    let mut low = Slung::default();
    low.aim(("notes", "p1"), Some(Rect { x: 790.0, y: 590.0, ..AT }), ItemId(0), now);
    low.went("scout", now);
    let pill = low.marks(&T, &[view(true)], now).labels[0].frame;
    assert!(pill.x + pill.width <= PANE.x + PANE.width - GAP, "{pill:?}");
    assert!(pill.y + pill.height <= PANE.y + PANE.height - GAP, "{pill:?}");
}

#[test]
fn a_sling_at_the_pane_s_top_left_puts_the_pill_under_it_its_text_on_its_fill() {
    let now = Instant::now();
    let mut slung = Slung::default();
    let corner = Rect { x: 0.0, y: 0.0, width: 40.0, height: 30.0 };
    slung.aim(("notes", "p1"), Some(corner), ItemId(0), now);
    slung.went("scout", now);
    let labels = slung.marks(&T, &[view(true)], now).labels;
    let pill = &labels[0];
    let under = (PANE.x + GAP, PANE.y + corner.height + GAP);
    assert_eq!((pill.frame.x, pill.frame.y), under, "under the selection, inside the pane");
    // One frame holds both the fill and the words: the window centres the
    // words in it both ways.
    assert_eq!((pill.fill, pill.frame.height), (Some(style::panel(&T)), HIGH));
    assert!(pill.frame.width >= HIGH && pill.text.starts_with("● scout working"));
}

#[test]
fn the_pill_shows_in_the_pane_with_the_sling_s_page_else_the_focused_one() {
    let now = Instant::now();
    let slung = out(now);
    let left = View { frame: Rect { x: 0.0, width: 200.0, ..PANE }, page: "p2", ..view(true) };
    let pill = slung.marks(&T, &[left, view(false)], now).labels[0].frame;
    assert!(pill.x >= PANE.x, "the pane that shows p1: {pill:?}");
    let pill = slung.marks(&T, &[left], now).labels[0].frame;
    assert_eq!((pill.x, pill.y), (GAP, GAP), "pinned in the focused pane: {pill:?}");
    assert_eq!(slung.marks(&T, &[], now), Marks::default(), "no canvas pane");
}

#[test]
fn a_new_sling_takes_the_last_one_s_place() {
    let now = Instant::now();
    let mut slung = out(now);
    slung.heard(&answered(Some("ok")), now);
    slung.aim(("notes", "p2"), None, ItemId(9), now);
    assert_eq!(slung.marks(&T, &[view(true)], now), Marks::default());
    assert_eq!(slung.watching(), None, "on its way");
}

#[test]
fn wrap_breaks_between_words_and_keeps_each_line() {
    assert_eq!(wrap("one two three", 7), ["one two", "three"]);
    assert_eq!(wrap("a\n\nb", 10), ["a", "", "b"]);
    assert_eq!(wrap("abcdefghij", 4), ["abcd", "efgh", "ij"]);
}
