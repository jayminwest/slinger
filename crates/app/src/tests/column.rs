//! A text with more rows than its frame shows (docs/design/writing-column.md,
//! 3A): the pane draws "N more lines" in its last row; two quick taps on
//! that row, or `step-in` with it selected, show it whole; escape steps out.
//! Its context menu offers Step in (slinger-69f995).

use super::step::{STEP_IN, apply, select_all};
use super::*;
use crate::commands::context::{Kind, Subject};
use crate::layout::Rect;
use crate::scene::canvas::{Draw, Mark};

const ESCAPE: Chord = Chord::new(0, '\u{1b}');
const NOTE: &str = "21 more lines";

/// Text `i1` of 100 rows at (100, 100) in an 80-cell column at the usual
/// size, and stroke `i2` beside it.
fn page(daemon: &control::Client) {
    let text = (1..=100).map(|i| format!("row {i}")).collect::<Vec<_>>().join("\n");
    apply(
        daemon,
        &serde_json::json!({"page": "p1", "kind": "text", "color": "#000000",
            "x": 1000, "y": 1000, "w": 9660, "text": text}),
    );
    apply(daemon, &super::step::stroke(20000, 1000));
}

/// The first pane's frame, and its draws.
fn shown(w: &mut World) -> (Rect, Vec<Draw>) {
    w.send(Event::Tick);
    let area = w.sidebar.panes(w.shared.size.rect());
    let theme = w.settings.config.theme;
    let panes = w.canvases.show(&w.shared.panes, area, &theme);
    (panes[0].frame, panes[0].scene.draws.clone())
}

/// The draw that says how many rows are hidden, if any.
fn note(draws: &[Draw]) -> Option<&Draw> {
    draws.iter().find(|d| matches!(&d.mark, Mark::Text { text, .. } if text == NOTE))
}

/// Whether the pane draws text `i1` whole: one mark with all its rows.
fn whole(draws: &[Draw]) -> bool {
    let rows = |d: &Draw| matches!(&d.mark, Mark::Text { text, .. } if text.lines().count() == 100);
    draws.iter().any(|d| d.id.0 == 1 && rows(d))
}

#[test]
fn a_long_text_says_how_many_more_lines_and_steps_in_by_two_taps_or_step_in() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        page(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        wait_until(|| note(&shown(&mut w).1).is_some());

        // Zoomed out until the note's row shows, then two quick taps on it.
        let (frame, draws) = shown(&mut w);
        let mut row = note(&draws).unwrap().bounds;
        for _ in 0..12 {
            if row.y + row.height < frame.height - 10.0 {
                break;
            }
            w.key(Chord::new(CMD, '-'), "-");
            row = note(&shown(&mut w).1).unwrap().bounds;
        }
        w.key(Chord::new(CMD, '7'), "7");
        let (x, y) = (frame.x + row.x + 10.0, frame.y + row.y + row.height / 2.0);
        for when in [1.0, 1.2] {
            assert!(w.send(Event::Press(at(x, y, when))));
            w.send(Event::Lift(at(x, y, when + 0.05)));
        }
        let (_, inside) = shown(&mut w);
        assert!(whole(&inside) && note(&inside).is_none(), "stepped in: every row");
        assert!(!inside.iter().any(|d| d.id.0 == 2), "the text alone");
        assert!(w.send(Event::Key(ESCAPE)), "escape steps out");
        let (_, out) = shown(&mut w);
        assert!(note(&out).is_some() && !whole(&out), "out: 80 rows again");

        // Selected with the stroke: step-in takes the text, as no box is selected.
        select_all(&mut w, 2);
        assert!(w.send(Event::Key(STEP_IN)));
        let (_, inside) = shown(&mut w);
        assert!(whole(&inside) && !inside.iter().any(|d| d.id.0 == 2), "{inside:?}");
        assert!(w.send(Event::Key(ESCAPE)));
        assert!(note(&shown(&mut w).1).is_some());
    });
}

#[test]
fn a_long_text_s_context_menu_offers_step_in_and_its_pick_steps_in() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        page(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        wait_until(|| note(&shown(&mut w).1).is_some());
        let (frame, draws) = shown(&mut w);
        let text = draws.iter().find(|d| d.id.0 == 1).expect("the text").bounds;
        let popup = w.context(frame.x + text.x + 10.0, frame.y + text.y + 10.0).expect("a menu");
        assert_eq!(popup.subject, Subject::Item { kind: Kind::LongText });
        let step = popup.items.iter().find(|i| i.title == "Step in").expect("offered");
        assert!(step.enabled);
        assert!(w.send(Event::Picked(Command::StepIn, popup.subject)), "the pick");
        let (_, inside) = shown(&mut w);
        assert!(whole(&inside) && !inside.iter().any(|d| d.id.0 == 2), "stepped in: {inside:?}");
    });
}
