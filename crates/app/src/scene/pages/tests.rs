use super::*;
use crate::layout::Axis;

const PANE: Rect = Rect { x: 0.0, y: 0.0, width: 600.0, height: 400.0 };
use crate::panes::{Kind, Pane, Panes};

fn pages(names: &[&str]) -> Vec<Page> {
    (1..)
        .zip(names)
        .map(|(n, name)| Page { id: format!("p{n}"), name: (*name).to_string() })
        .collect()
}

/// Two panes' ids: the first and the one split off it.
fn two_panes() -> (PaneId, PaneId) {
    let mut panes = Panes::new(Pane { kind: Kind::Canvas, name: "main".into() });
    let first = panes.focus();
    panes.split(Axis::Row);
    (first, panes.focus())
}

#[test]
fn next_and_previous_go_round_and_a_lost_page_counts_as_the_first() {
    let three = pages(&["a", "b", "c"]);
    assert_eq!(step(&three, "p1", true), Some("p2"));
    assert_eq!(step(&three, "p3", true), Some("p1"), "after the last, the first");
    assert_eq!(step(&three, "p1", false), Some("p3"), "before the first, the last");
    assert_eq!(step(&three, "p2", false), Some("p1"));
    assert_eq!(step(&three, "p9", true), Some("p2"));
    assert_eq!(step(&pages(&["a"]), "p1", false), Some("p1"));
    assert_eq!(step(&[], "p1", true), None);
}

#[test]
fn a_new_page_is_named_after_the_count_or_the_next_free_number() {
    assert_eq!(new_name(&pages(&["Page 1"])), "Page 2");
    assert_eq!(new_name(&pages(&["ideas", "spec"])), "Page 3");
    assert_eq!(new_name(&pages(&["Page 1", "Page 3"])), "Page 4");
    assert_eq!(new_name(&pages(&["x", "Page 3", "Page 4"])), "Page 5");
    assert_eq!(new_name(&[]), "Page 1");
}

#[test]
fn a_removed_page_sends_its_pane_to_the_page_before_or_else_after() {
    let three = pages(&["a", "b", "c"]);
    assert_eq!(after_remove(&three, "p2"), Some("p1"));
    assert_eq!(after_remove(&three, "p1"), Some("p2"));
    assert_eq!(after_remove(&three, "p3"), Some("p2"));
    assert_eq!(after_remove(&pages(&["a"]), "p1"), None, "the last page stays");
    assert_eq!(after_remove(&three, "p9"), None);
}

#[test]
fn the_line_names_the_page_and_where_it_is_among_the_pages() {
    assert_eq!(line(&pages(&["Page 1"]), "p1"), "Page 1 · 1/1");
    assert_eq!(line(&pages(&["ideas", "spec"]), "p2"), "spec · 2/2");
    assert_eq!(line(&pages(&["ideas"]), "p7"), "");
}

#[test]
fn removing_a_page_with_items_asks_once_and_an_empty_page_goes_at_once() {
    let (here, there) = two_panes();
    let [empty, full] = &pages(&["a", "b"])[..] else { unreachable!() };
    let mut asks = Pages::default();
    let gone = Some(("main".to_string(), PageOp::Remove { page: "p1".into() }));
    assert_eq!(asks.remove(here, "main", empty, 0), gone);
    assert!(!asks.asking());

    assert_eq!(asks.remove(here, "main", full, 3), None, "the first press asks");
    assert!(asks.asking());
    let asked = asks.prompt(here).unwrap();
    assert!(asked.contains("“b”") && asked.contains("3 items"), "{asked}");
    assert_eq!(asks.prompt(there), None, "only the pane that asked says it");
    let gone = Some(("main".to_string(), PageOp::Remove { page: "p2".into() }));
    assert_eq!(asks.remove(here, "main", full, 3), gone, "the second removes it");
    assert_eq!(asks.prompt(here), None);
}

#[test]
fn a_removal_asked_is_answered_by_return_or_kept_by_escape() {
    let (here, _) = two_panes();
    let full = &pages(&["a", "b"])[1];
    let mut asks = Pages::default();
    let gone = Some(("main".to_string(), PageOp::Remove { page: "p2".into() }));
    asks.remove(here, "main", full, 1);
    assert!(asks.prompt(here).unwrap().contains("1 item?"));
    assert_eq!(asks.typed("x"), None, "other keys wait");
    assert_eq!(asks.typed("\r"), gone, "return removes it");

    asks.remove(here, "main", full, 1);
    assert_eq!(asks.typed("\u{1b}"), None);
    assert!(!asks.asking(), "escape keeps it");
    asks.remove(here, "main", full, 1);
    asks.cancel();
    assert_eq!(asks.remove(here, "main", full, 1), None, "a closed question asks again");
    assert_eq!(asks.remove(here, "other", full, 1), None, "another canvas's page asks again");
}

#[test]
fn a_rename_takes_typed_keys_and_return_keeps_a_new_name() {
    let (here, _) = two_panes();
    let page = &pages(&["Page 1"])[0];
    let mut asks = Pages::default();
    assert_eq!(asks.typed("x"), None, "no question, no answer");
    asks.rename((here, PANE), "main", page);
    let entry = asks.entry(&Theme::default()).unwrap();
    assert_eq!(entry.text, "Page 1", "it starts from the old name");
    assert_eq!(asks.prompt(here), None, "the entry asks, not the footer");
    asks.typed("\u{7f}\u{7f}\u{7f}\u{7f}\u{7f}\u{7f}\u{f700}\u{1}");
    asks.typed("ideas");
    assert_eq!(asks.entry(&Theme::default()).unwrap().text, "ideas");
    let renamed = Some(("main".into(), PageOp::Rename { page: "p1".into(), name: "ideas".into() }));
    assert_eq!(asks.typed(" \r"), renamed);
    assert!(!asks.asking());

    asks.rename((here, PANE), "main", page);
    asks.typed("\u{7f}\u{7f}\u{7f}\u{7f}\u{7f}\u{7f}  ");
    assert_eq!(asks.typed("\r"), None, "a blank name changes nothing");
    asks.rename((here, PANE), "main", page);
    assert_eq!(asks.typed("zz\u{1b}more"), None, "escape keeps the old name");
    assert!(!asks.asking());
}

#[test]
fn a_paste_goes_into_a_new_name_on_one_line_and_a_removal_waits() {
    let (here, _) = two_panes();
    let [a, b] = &pages(&["a", "b"])[..] else { unreachable!() };
    let mut asks = Pages::default();
    assert!(!asks.pasted("x"), "no question takes it");
    asks.rename((here, PANE), "main", a);
    assert!(asks.pasted(" plan\r\nfor\u{1b} monday"), "a pasted escape is not a key");
    assert!(asks.asking(), "a pasted line end does not answer");
    let renamed = PageOp::Rename { page: "p1".into(), name: "a plan for monday".into() };
    assert_eq!(asks.typed("\r"), Some(("main".into(), renamed)));

    asks.remove(here, "main", b, 2);
    assert!(asks.pasted("\r"));
    assert!(asks.asking(), "only return removes it");
}

#[test]
fn each_page_command_says_what_it_did_once() {
    let (here, _) = two_panes();
    let three = pages(&["Page 1", "ideas", "Page 3"]);
    let mut asks = Pages::default();
    assert_eq!(asks.said(), None);
    asks.turned(&three, "p2");
    assert_eq!(asks.said().as_deref(), Some("ideas · 2/3"));
    assert_eq!(asks.said(), None, "said once");
    asks.turned(&three, "p9");
    assert_eq!(asks.said(), None, "a page not among them says nothing");

    asks.changed(&three, &PageOp::Add { name: "Page 4".into() });
    assert_eq!(asks.said().as_deref(), Some("New page: Page 4 · 4/4"));
    asks.changed(&three, &PageOp::Rename { page: "p2".into(), name: "plan".into() });
    assert_eq!(asks.said().as_deref(), Some("Renamed: plan"));
    asks.changed(&three, &PageOp::Remove { page: "p2".into() });
    assert_eq!(asks.said().as_deref(), Some("Removed: ideas"));
    asks.kept_last();
    assert_eq!(asks.said().as_deref(), Some("The last page stays"));

    asks.rename((here, PANE), "main", &three[1]);
    assert_eq!(asks.said().as_deref(), Some("Rename “ideas”"));
    asks.remove(here, "main", &three[2], 4);
    assert_eq!(asks.said().as_deref(), Some("Remove “Page 3”?"));
}

#[test]
fn a_new_name_is_typed_in_an_entry_like_the_sling_s_in_its_pane() {
    let (here, _) = two_panes();
    let page = &pages(&["Page 1"])[0];
    let theme = Theme::default();
    let mut asks = Pages::default();
    assert_eq!((asks.entry(&theme), asks.renaming()), (None, false));
    asks.rename((here, PANE), "main", page);
    let entry = asks.entry(&theme).unwrap();
    let same =
        Entry::framed(entry.frame, &theme, "Rename page".into(), entry.keys.label.text.clone());
    assert_eq!(Entry { text: String::new(), placeholder: String::new(), ..entry.clone() }, same);
    assert_eq!(entry.frame, centred(PANE));
    assert!(entry.keys.label.text.ends_with("renames · esc cancels"));
    let inside = Point { x: entry.frame.x + 4.0, y: entry.frame.y + 4.0 };
    assert!(asks.covers(inside) && !asks.covers(Point { x: 1.0, y: 1.0 }));

    asks.named("plans");
    let renamed = PageOp::Rename { page: "p1".into(), name: "plans".into() };
    assert_eq!(asks.typed("\r"), Some(("main".into(), renamed)), "return keeps the new name");
    assert!(!asks.renaming() && !asks.cancel_rename());
    asks.rename((here, PANE), "main", page);
    assert!(asks.cancel_rename() && !asks.asking(), "the old name stays");
    asks.remove(here, "main", page, 3);
    assert!(!asks.cancel_rename() && asks.asking(), "a removal's question is not an entry");
}
