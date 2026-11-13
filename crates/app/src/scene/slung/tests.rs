use super::words::wrap;
use super::*;
use crate::config::Rgb;

const T: Theme = Theme {
    background: Rgb { r: 0, g: 0, b: 0 },
    text: Rgb { r: 1, g: 1, b: 1 },
    muted: Rgb { r: 2, g: 2, b: 2 },
    error: Rgb { r: 3, g: 3, b: 3 },
    focus: Rgb { r: 4, g: 4, b: 4 },
    done: Rgb { r: 5, g: 5, b: 5 },
};

const PANE: Rect = Rect { x: 200.0, y: 0.0, width: 800.0, height: 600.0 };
/// Where the sling was made, in canvas units.
const AT: Rect = Rect { x: 100.0, y: 100.0, width: 200.0, height: 50.0 };

fn view(focused: bool) -> View<'static> {
    View { frame: PANE, canvas: "notes", page: "p1", viewport: Viewport::default(), focused }
}

fn working(pending: u32) -> Progress {
    Progress { running: true, pending, blocked: None, answer: None, profile: None, unheard: false }
}

fn answered(answer: Option<&str>) -> Progress {
    let answer = answer.map(str::to_string);
    Progress { running: true, pending: 0, blocked: None, answer, profile: None, unheard: false }
}

/// A sling of page p1 of notes made at [`AT`] at `now`, gone to scout.
fn out(now: Instant) -> Slung {
    let mut slung = Slung::default();
    slung.aim(("notes", "p1"), Some(AT), None, ItemId(5), now);
    assert_eq!(slung.marks(&T, &[view(true)], now), Marks::default(), "the entry's line says it");
    assert!(slung.went("scout", None, now));
    slung
}

/// The pill's text and lines, top first.
fn said(slung: &Slung, views: &[View], now: Instant) -> Vec<String> {
    slung.marks(&T, views, now).all().into_iter().map(|l| l.text).collect()
}

fn agent_item(id: u64, frame: Rect) -> (ItemId, Rect, Option<&'static str>) {
    (ItemId(id), frame, Some("agent:scout"))
}

#[test]
fn a_sling_under_a_profile_names_it_when_it_goes() {
    let (now, mut slung) = (Instant::now(), Slung::default());
    slung.aim(("notes", "p1"), Some(AT), Some("dev"), ItemId(5), now);
    assert!(slung.went("scout", None, now));
    assert_eq!(slung.flash().as_deref(), Some("Slung to scout (dev)"));
    let dev = || Progress { profile: Some("dev".into()), ..working(1) };
    assert!(!slung.heard(&dev(), now), "the same profile");
    assert_eq!(said(&slung, &[view(true)], now), ["● scout (dev) working · 0s"]);
    let blocked = Progress { blocked: Some("which key?".into()), ..dev() };
    assert!(slung.heard(&blocked, now));
    assert_eq!(said(&slung, &[view(true)], now), ["■ scout (dev) blocked: which key?"]);
}

#[test]
fn the_pill_names_the_profile_the_agent_s_process_says_it_runs_under() {
    let now = Instant::now();
    let mut slung = Slung::default();
    slung.aim(("notes", "p1"), Some(AT), Some("old"), ItemId(5), now);
    assert!(slung.went("scout", None, now));
    let plain = slung.marks(&T, &[view(true)], now).all()[0].frame.width;
    let look = Progress { profile: Some("look".into()), ..working(1) };
    assert!(slung.heard(&look, now), "restarted under another since the sidebar heard");
    let line = &slung.marks(&T, &[view(true)], now).all()[0];
    assert_eq!(line.text, "● scout (look) working · 0s");
    let measured = glyphs::frame(Point { x: 0.0, y: 0.0 }, &line.text, TEXT, glyphs::UNWRAPPED);
    assert_eq!(line.frame.width, (measured.width + 2.0 * PAD).max(HIGH), "as wide as it says");
    assert!(line.frame.width > plain, "{} after {plain}", line.frame.width);
    assert!(slung.heard(&working(1), now), "under none now");
    assert_eq!(said(&slung, &[view(true)], now), ["● scout working · 0s"]);
    let gone = Progress { running: false, profile: None, ..look };
    assert!(slung.heard(&gone, now));
    assert_eq!(said(&slung, &[view(true)], now)[0], "✗ scout stopped before it answered");
}

#[test]
fn a_sling_says_it_went_and_counts_while_the_agent_works() {
    let now = Instant::now();
    let mut slung = out(now);
    assert_eq!(slung.flash().as_deref(), Some("Slung to scout"));
    assert_eq!(slung.flash(), None, "once");
    assert_eq!(said(&slung, &[view(true)], now), ["● scout working · 0s"]);
    let pill = slung.marks(&T, &[view(true)], now).all()[0].clone();
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
    assert_eq!(marks.all()[0].text, "■ scout blocked: which key?");
    assert_eq!(marks.all()[0].colour, T.error);
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
fn items_that_land_after_the_answer_count_and_it_never_says_in_text() {
    let now = Instant::now();
    let mut slung = out(now);
    slung.flash();
    assert!(slung.heard(&answered(Some("done")), now));
    assert!(!slung.tick(now + SETTLE / 2));
    assert_eq!(slung.flash(), None, "not yet");
    let mine = agent_item(7, Rect { x: 150.0, y: 300.0, width: 100.0, height: 40.0 });
    assert!(slung.landed(("notes", "p1"), [mine]));
    assert!(slung.tick(now + SETTLE / 2));
    assert_eq!(slung.flash().as_deref(), Some("scout answered"));
    assert!(!slung.tick(now + SETTLE), "once");
    assert_eq!(slung.flash(), None);
    assert_eq!(said(&slung, &[view(true)], now), ["✓ scout answered · 1 new item"]);
}

#[test]
fn an_answer_in_words_opens_under_the_pill_and_a_tap_closes_it() {
    let now = Instant::now();
    let mut slung = out(now);
    assert!(slung.heard(&answered(Some("It is two.\nAnd three.")), now));
    slung.flash();
    assert_eq!(slung.flash(), None, "its items may be on their way");
    assert_eq!(said(&slung, &[view(true)], now), ["✓ scout answered"]);
    assert!(slung.tick(now + SETTLE), "none landed");
    assert_eq!(slung.flash().as_deref(), Some("scout answered in text"));
    let marks = slung.marks(&T, &[view(true)], now);
    assert_eq!(marks.all()[0].text, "✓ scout answered in text · tap to read");
    assert_eq!(marks.panel, None);
    assert_eq!(marks.targets, [(marks.all()[0].frame, Tap::Slung)]);
    assert_eq!(slung.tapped(&[view(true)]), Tapped::Changed);
    let marks = slung.marks(&T, &[view(true)], now);
    let panel = marks.panel.expect("the answer");
    let texts = panel.lines.iter().map(|l| l.label.text.as_str()).filter(|t| !t.is_empty());
    let texts: Vec<&str> = texts.collect();
    assert_eq!(texts, ["It is two.", "And three.", "tap to close"]);
    assert!(marks.targets[0].0.height > HIGH, "the answer takes the tap too");
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
    let at = Point { x: 0.0, y: 0.0 };
    for line in &lines[..MOST - 1] {
        let wide = glyphs::frame(at, line, TEXT, glyphs::UNWRAPPED).width;
        assert!(wide <= PANEL - 2.0 * PAD, "each line fits as measured: {line:?}");
        assert!(wide > PANEL - 2.0 * PAD - 40.0, "and fills the panel: {line:?}");
    }
}

#[test]
fn the_pill_says_what_the_sling_costs_when_the_daemon_counts_it() {
    let now = Instant::now();
    let mut slung = Slung::default();
    slung.aim(("notes", "p1"), Some(AT), None, ItemId(5), now);
    assert!(slung.went("scout", Some(12_345), now));
    assert_eq!(said(&slung, &[view(true)], now), ["● scout working · 0s · ~12k"]);
    let blocked = Progress { blocked: Some("which key?".into()), ..working(1) };
    slung.heard(&blocked, now);
    assert_eq!(said(&slung, &[view(true)], now), ["■ scout blocked: which key? · ~12k"]);
    slung.heard(&answered(None), now);
    assert!(!said(&slung, &[view(true)], now)[0].contains('~'), "only while it is out");
    let costs = [840, 1_234, 9_960, 999_499, 1_250_000].map(|n| words::cost(Some(n)));
    assert_eq!(costs, [" · ~840", " · ~1.2k", " · ~10k", " · ~999k", " · ~1.3M"]);
    assert_eq!(words::cost(None), "", "an older daemon does not count");
}

#[test]
fn an_agent_done_with_no_answer_says_so_and_it_all_clears_in_time() {
    let now = Instant::now();
    let mut slung = out(now);
    assert!(slung.heard(&answered(None), now));
    slung.flash();
    slung.tick(now + SETTLE);
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
    let texts: Vec<String> = marks.all().into_iter().map(|l| l.text).collect();
    assert_eq!(
        texts,
        ["✗ scout stopped before it answered", "start it with 'slinger session start <WORKSPACE>'"]
    );
    assert_eq!(marks.all()[0].colour, T.error);
    assert!(slung.tick(now + CLEAR));
}

#[test]
fn a_sling_that_did_not_go_says_why_unless_its_entry_does() {
    let now = Instant::now();
    let problem = Problem { message: "scout is not running".into(), hint: Some("start it".into()) };
    let mut slung = Slung::default();
    slung.aim(("notes", "p1"), Some(AT), None, ItemId(5), now);
    assert!(slung.failed(problem.clone(), now));
    assert_eq!(said(&slung, &[view(true)], now), ["✗ scout is not running", "start it"]);
    assert_eq!(slung.tapped(&[view(true)]), Tapped::Changed, "a tap clears it");
    assert!(!slung.failed(problem, now), "nothing out");

    slung.aim(("notes", "p1"), Some(AT), None, ItemId(5), now);
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
    slung.aim(("notes", "p1"), None, None, ItemId(0), now);
    slung.went("scout", None, now);
    let pill = slung.marks(&T, &[view(true)], now).all()[0].frame;
    assert_eq!((pill.x, pill.y), (PANE.x + GAP, PANE.y + GAP), "the pane's top left");

    let mut low = Slung::default();
    low.aim(("notes", "p1"), Some(Rect { x: 790.0, y: 590.0, ..AT }), None, ItemId(0), now);
    low.went("scout", None, now);
    let pill = low.marks(&T, &[view(true)], now).all()[0].frame;
    assert!(pill.x + pill.width <= PANE.x + PANE.width - GAP, "{pill:?}");
    assert!(pill.y + pill.height <= PANE.y + PANE.height - GAP, "{pill:?}");
}

#[test]
fn a_sling_at_the_pane_s_top_left_puts_the_pill_under_it_its_text_on_its_fill() {
    let now = Instant::now();
    let mut slung = Slung::default();
    let corner = Rect { x: 0.0, y: 0.0, width: 40.0, height: 30.0 };
    slung.aim(("notes", "p1"), Some(corner), None, ItemId(0), now);
    slung.went("scout", None, now);
    let line = slung.marks(&T, &[view(true)], now).pill.expect("the pill");
    let pill = &line.label;
    let under = (PANE.x + GAP, PANE.y + corner.height + GAP);
    assert_eq!((pill.frame.x, pill.frame.y), under, "under the selection, inside the pane");
    // One frame holds both the fill and the words: the window centres the
    // words in it both ways, and rounds its corners as a field's.
    assert_eq!((pill.fill, pill.frame.height), (Some(style::panel(&T)), HIGH));
    assert_eq!((line.align, line.radius, line.size), (Align::Centre, style::FIELD_RADIUS, TEXT));
    let words = glyphs::frame(Point { x: 0.0, y: 0.0 }, &pill.text, TEXT, glyphs::UNWRAPPED);
    assert_eq!(pill.frame.width, words.width + 2.0 * PAD, "as wide as its words measure");
    assert!(pill.frame.width >= HIGH && pill.text.starts_with("● scout working"));
}

#[test]
fn the_pill_shows_in_the_pane_with_the_sling_s_page_else_the_focused_one() {
    let now = Instant::now();
    let slung = out(now);
    let left = View { frame: Rect { x: 0.0, width: 200.0, ..PANE }, page: "p2", ..view(true) };
    let pill = slung.marks(&T, &[left, view(false)], now).all()[0].frame;
    assert!(pill.x >= PANE.x, "the pane that shows p1: {pill:?}");
    let pill = slung.marks(&T, &[left], now).all()[0].frame;
    assert_eq!((pill.x, pill.y), (GAP, GAP), "pinned in the focused pane: {pill:?}");
    assert_eq!(slung.marks(&T, &[], now), Marks::default(), "no canvas pane");
}

#[test]
fn a_new_sling_takes_the_last_one_s_place() {
    let now = Instant::now();
    let mut slung = out(now);
    slung.heard(&answered(Some("ok")), now);
    slung.aim(("notes", "p2"), None, None, ItemId(9), now);
    assert_eq!(slung.marks(&T, &[view(true)], now), Marks::default());
    assert_eq!(slung.watching(), None, "on its way");
}

#[test]
fn wrap_breaks_between_words_and_keeps_each_line() {
    let room = |n: usize| move |line: &str| line.chars().count() <= n;
    assert_eq!(wrap("one two three", room(7)), ["one two", "three"]);
    assert_eq!(wrap("a\n\nb", room(10)), ["a", "", "b"]);
    assert_eq!(wrap("abcdefghij", room(4)), ["abcd", "efgh", "ij"]);
    assert_eq!(wrap("ab cd", room(0)), ["a", "b", "c", "d"], "a character at least");
}

#[test]
fn a_sling_nobody_listens_for_says_how_to_listen_until_one_takes_it() {
    let now = Instant::now();
    let mut slung = out(now);
    assert!(slung.heard(&Progress { unheard: true, ..working(1) }, now));
    let marks = slung.marks(&T, &[view(true)], now);
    let texts: Vec<String> = marks.all().into_iter().map(|l| l.text).collect();
    let hint = "run /slinger-listen in a Claude Code session";
    assert_eq!(texts, ["○ scout: nobody listens · 0s", hint]);
    assert!(slung.tick(now + CLEAR), "its time counts");
    assert!(slung.out(), "it waits, and is not cleared");
    // A listener takes it, then answers.
    assert!(slung.heard(&working(1), now));
    assert_eq!(said(&slung, &[view(true)], now), ["● scout working · 0s"]);
    assert!(slung.heard(&answered(None), now));
}
