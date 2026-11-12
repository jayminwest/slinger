//! Undo and redo, through the public interface only.
#![cfg(test)]

use slinger_core::canvas::{
    Action, Applied, Author, Canvas, Colour, End, History, Item, ItemId, Op, Page, PageId, Point,
    Rect, Shape, Time,
};

const AT: Time = Time(1_700_000_000_000);
const LATER: Time = Time(1_700_000_001_000);

fn agent(name: &str) -> Author {
    Author::Agent(name.into())
}

fn stroke(x: f64) -> Shape {
    Shape::Stroke {
        points: vec![Point::new(x, 0.0), Point::new(x + 4.0, 2.5)],
        colour: Colour::BLACK,
        width: 2.0,
    }
}

fn text(s: &str) -> Shape {
    Shape::Text {
        text: s.into(),
        frame: Rect::new(10.0, 20.0, 100.0, 40.0),
        colour: Colour::BLACK,
        size: 24.0,
    }
}

fn page_of(c: &Canvas, name: &str) -> PageId {
    c.page_named(name).unwrap().id()
}

fn first(c: &Canvas) -> PageId {
    c.pages().next().unwrap().id()
}

/// Applies one action through the history and returns the new item's id, if any.
fn act(h: &mut History, c: &mut Canvas, by: &Author, action: Action) -> Option<ItemId> {
    match h.apply(c, by, AT, vec![action]).unwrap()[..] {
        [Applied::Added(id)] => Some(id),
        _ => None,
    }
}

fn add(h: &mut History, c: &mut Canvas, by: &Author, shape: Shape) -> ItemId {
    let page = first(c);
    act(h, c, by, Action::Add { page, shape }).unwrap()
}

fn ids(c: &Canvas) -> Vec<ItemId> {
    c.pages().flat_map(|p| c.items(p.id())).map(Item::id).collect()
}

/// What a canvas shows: its pages and items. Undo of an add leaves the id
/// used (ids are never reused), so it compares on what is shown.
fn shown(c: &Canvas) -> (Vec<Page>, Vec<Item>) {
    let items = c.pages().flat_map(|p| c.items(p.id())).cloned().collect();
    (c.pages().cloned().collect(), items)
}

/// A canvas with two pages, the user's ink, an agent's note and an arrow from
/// the ink to the note, and an item already removed (item 4).
fn base() -> Canvas {
    let user = Author::User;
    let mut c = Canvas::new();
    let ops = [
        (user.clone(), Action::PageAdd { name: "spec".into() }),
        (user.clone(), Action::Add { page: first(&c), shape: stroke(0.0) }),
        (agent("scout"), Action::Add { page: first(&c), shape: text("note") }),
        (user.clone(), Action::Add { page: PageId::new(2), shape: stroke(50.0) }),
        (user.clone(), Action::Add { page: PageId::new(2), shape: stroke(60.0) }),
        (user.clone(), Action::Remove { item: ItemId::new(4) }),
    ];
    for (by, action) in ops {
        c.apply(Op { by, at: AT, action }).unwrap();
    }
    let arrow = Shape::Arrow {
        from: End::on(Point::new(0.0, 0.0), ItemId::new(1)),
        to: End::on(Point::new(10.0, 20.0), ItemId::new(2)),
        colour: Colour::BLACK,
    };
    c.apply(Op { by: user, at: AT, action: Action::Add { page: first(&c), shape: arrow } })
        .unwrap();
    c
}

#[test]
fn undo_then_redo_returns_the_same_canvas_for_every_operation_kind() {
    let c = base();
    let ink = c.item(ItemId::new(1)).unwrap().clone();
    let mut without_ink = c.clone();
    without_ink
        .apply(Op { by: Author::User, at: AT, action: Action::Remove { item: ink.id() } })
        .unwrap();
    let spec = page_of(&c, "spec");
    let spec_page = c.page(spec).unwrap().clone();
    let spec_items: Vec<_> = c.items(spec).cloned().collect();
    let without_spec = {
        let mut w = c.clone();
        w.apply(Op { by: Author::User, at: AT, action: Action::PageRemove { page: spec } })
            .unwrap();
        w
    };
    let cases: Vec<(&str, Canvas, Action)> = vec![
        ("add", c.clone(), Action::Add { page: first(&c), shape: stroke(9.0) }),
        ("edit", c.clone(), Action::Edit { item: ItemId::new(1), shape: stroke(30.0) }),
        ("move", c.clone(), Action::Move { item: ItemId::new(1), by: Point::new(0.1, -0.7) }),
        (
            "move an arrow",
            c.clone(),
            Action::Move { item: ItemId::new(5), by: Point::new(1.0, 1.0) },
        ),
        ("remove", c.clone(), Action::Remove { item: ItemId::new(1) }),
        ("page add", c.clone(), Action::PageAdd { name: "more".into() }),
        ("page rename", c.clone(), Action::PageRename { page: spec, name: "plan".into() }),
        ("page remove", c.clone(), Action::PageRemove { page: spec }),
        ("restore", without_ink, Action::Restore { item: ink }),
        (
            "page restore",
            without_spec,
            Action::PageRestore { page: spec_page, index: 1, items: spec_items },
        ),
    ];
    for (kind, before, action) in cases {
        let user = Author::User;
        let mut h = History::new();
        let mut c = before.clone();
        let mut log = vec![Op { by: user.clone(), at: AT, action: action.clone() }];
        h.apply(&mut c, &user, AT, vec![action]).unwrap();
        let after = c.clone();
        assert_ne!(after, before, "{kind}: the op changes the canvas");

        let undone = h.undo(&mut c, &user, LATER);
        assert!(!undone.is_empty(), "{kind}: undo writes ops");
        assert_eq!(shown(&c), shown(&before), "{kind}: undo returns the canvas before");
        log.extend(undone);

        let redone = h.redo(&mut c, &user, LATER);
        assert!(!redone.is_empty(), "{kind}: redo writes ops");
        assert_eq!(c, after, "{kind}: redo returns the canvas after");
        log.extend(redone);

        // The log stays append-only: replaying it builds the same canvas.
        let mut replay = before.clone();
        for op in log {
            replay.apply(op).unwrap();
        }
        assert_eq!(replay, after, "{kind}: the log replays to the same canvas");

        assert!(!h.undo(&mut c, &user, LATER).is_empty(), "{kind}: undo again after redo");
        assert_eq!(shown(&c), shown(&before), "{kind}: and back to before");
    }
}

#[test]
fn undo_is_per_author() {
    let (user, scout, other) = (Author::User, agent("scout"), agent("other"));
    let mut h = History::new();
    let mut c = Canvas::new();
    let ink = add(&mut h, &mut c, &user, stroke(0.0));
    let note = add(&mut h, &mut c, &scout, text("note"));
    let more = add(&mut h, &mut c, &user, stroke(9.0));

    // The agent's undo takes back the agent's note, and only that.
    let undone = h.undo(&mut c, &scout, LATER);
    assert_eq!(undone.len(), 1);
    assert_eq!(undone[0].by, scout);
    assert_eq!(ids(&c), [ink, more]);
    assert!(!h.can_undo(&scout));
    assert!(h.undo(&mut c, &scout, LATER).is_empty(), "nothing left for the agent");
    assert!(h.undo(&mut c, &other, LATER).is_empty(), "another agent has nothing to undo");
    assert_eq!(ids(&c), [ink, more]);

    // The user's undo takes back the user's work, newest first.
    h.redo(&mut c, &scout, LATER);
    assert_eq!(ids(&c), [ink, note, more]);
    h.undo(&mut c, &user, LATER);
    assert_eq!(ids(&c), [ink, note]);
    h.undo(&mut c, &user, LATER);
    assert_eq!(ids(&c), [note]);
    assert_eq!(h.undo(&mut c, &user, LATER), []);
    assert_eq!(ids(&c), [note], "the user's undo never takes the agent's note");

    h.redo(&mut c, &user, LATER);
    assert_eq!(ids(&c), [ink, note]);
}

#[test]
fn undo_and_redo_with_nothing_to_do_succeed_and_change_nothing() {
    let mut h = History::new();
    let mut c = base();
    let before = c.clone();
    for by in [Author::User, agent("scout")] {
        assert!(!h.can_undo(&by) && !h.can_redo(&by));
        assert_eq!(h.undo(&mut c, &by, LATER), []);
        assert_eq!(h.redo(&mut c, &by, LATER), []);
    }
    assert_eq!(c, before);

    // Redo after everything was redone also does nothing.
    let user = Author::User;
    add(&mut h, &mut c, &user, stroke(1.0));
    let after = c.clone();
    h.undo(&mut c, &user, LATER);
    h.redo(&mut c, &user, LATER);
    assert_eq!(h.redo(&mut c, &user, LATER), []);
    assert_eq!(c, after);
}

#[test]
fn a_lasso_move_and_an_erase_gesture_are_one_step_each() {
    let user = Author::User;
    let mut h = History::new();
    let mut c = Canvas::new();
    let picked: Vec<_> = (0..3).map(|i| add(&mut h, &mut c, &user, stroke(f64::from(i)))).collect();
    let made = c.clone();

    let by = Point::new(5.0, 5.0);
    let lasso = picked.iter().map(|&item| Action::Move { item, by }).collect();
    h.apply(&mut c, &user, AT, lasso).unwrap();
    let moved = c.clone();
    let erase = vec![Action::Remove { item: picked[0] }, Action::Remove { item: picked[2] }];
    h.apply(&mut c, &user, AT, erase).unwrap();
    assert_eq!(ids(&c), [picked[1]]);

    assert_eq!(h.undo(&mut c, &user, LATER).len(), 2, "the erase comes back in one undo");
    assert_eq!(c, moved);
    assert_eq!(h.undo(&mut c, &user, LATER).len(), 3, "the lasso move goes back in one undo");
    assert_eq!(c, made);
}

#[test]
fn a_group_that_fails_changes_nothing_and_records_no_step() {
    let user = Author::User;
    let mut h = History::new();
    let mut c = Canvas::new();
    let ink = add(&mut h, &mut c, &user, stroke(0.0));
    h.undo(&mut c, &user, LATER);
    let before = c.clone();
    let page = first(&c);
    let group = vec![
        Action::Add { page, shape: stroke(1.0) },
        Action::Move { item: ItemId::new(99), by: Point::new(1.0, 1.0) },
    ];
    let err = h.apply(&mut c, &user, AT, group).unwrap_err();
    assert_eq!(err.code(), "not_found");
    assert_eq!(c, before, "no item and no used id");
    assert!(h.can_redo(&user), "a failed change keeps what the user could redo");
    h.redo(&mut c, &user, LATER);
    assert_eq!(ids(&c), [ink]);
}

#[test]
fn a_new_change_clears_redo() {
    let user = Author::User;
    let mut h = History::new();
    let mut c = Canvas::new();
    add(&mut h, &mut c, &user, stroke(0.0));
    h.undo(&mut c, &user, LATER);
    assert!(h.can_redo(&user));
    add(&mut h, &mut c, &user, stroke(1.0));
    assert!(!h.can_redo(&user));
}

#[test]
fn a_step_whose_item_is_gone_is_skipped() {
    let (user, scout) = (Author::User, agent("scout"));
    let mut h = History::new();
    let mut c = Canvas::new();
    let keep = add(&mut h, &mut c, &scout, text("keep"));
    let note = add(&mut h, &mut c, &scout, text("note"));
    act(&mut h, &mut c, &scout, Action::Move { item: note, by: Point::new(3.0, 3.0) });
    act(&mut h, &mut c, &user, Action::Remove { item: note });

    // The agent's move and add of the note no longer apply; its undo goes on
    // to the step before them, and never brings the note back.
    let undone = h.undo(&mut c, &scout, LATER);
    assert_eq!(undone.len(), 1);
    assert_eq!(undone[0].action, Action::Remove { item: keep });
    assert!(c.item(note).is_none() && c.item(keep).is_none());

    // The user's own undo still puts the note back where the agent left it.
    h.undo(&mut c, &user, LATER);
    assert_eq!(c.item(note).unwrap().bounds().x, 13.0);
}

#[test]
fn restore_puts_back_only_what_was_removed() {
    let user = Author::User;
    let mut c = base();
    let ink = c.item(ItemId::new(1)).unwrap().clone();
    let restore = |item| Op { by: user.clone(), at: AT, action: Action::Restore { item } };
    assert_eq!(c.apply(restore(ink.clone())).unwrap_err().code(), "conflict");

    // An id the canvas never handed out names nothing to put back.
    let mut fresh = Canvas::new();
    assert_eq!(fresh.apply(restore(ink.clone())).unwrap_err().code(), "invalid");

    // An agent may not put back the user's ink.
    c.apply(Op { by: user.clone(), at: AT, action: Action::Remove { item: ink.id() } }).unwrap();
    let by_agent = Op { by: agent("scout"), at: AT, action: Action::Restore { item: ink.clone() } };
    assert_eq!(c.apply(by_agent).unwrap_err().code(), "invalid");
    assert_eq!(c.apply(restore(ink)).unwrap(), Applied::Changed);
}

#[test]
fn page_restore_puts_back_only_a_removed_page() {
    let user = Author::User;
    let mut c = base();
    let spec = page_of(&c, "spec");
    let page = c.page(spec).unwrap().clone();
    let items: Vec<_> = c.items(spec).cloned().collect();
    let op = |items: Vec<_>| Op {
        by: user.clone(),
        at: AT,
        action: Action::PageRestore { page: page.clone(), index: 9, items },
    };
    assert_eq!(c.apply(op(items.clone())).unwrap_err().code(), "conflict");
    c.apply(Op { by: user.clone(), at: AT, action: Action::PageRemove { page: spec } }).unwrap();

    // Items from another page do not come back with it.
    let stranger = c.item(ItemId::new(1)).unwrap().clone();
    assert_eq!(c.apply(op(vec![stranger])).unwrap_err().code(), "invalid");

    // A page that never existed cannot be restored.
    assert_eq!(Canvas::new().apply(op(Vec::new())).unwrap_err().code(), "invalid");

    // The index is clamped to the end.
    c.apply(op(items)).unwrap();
    assert_eq!(c.pages().last().unwrap().id(), spec);
    assert_eq!(c.items(spec).count(), 1);
}

#[test]
fn an_edit_may_keep_an_arrow_on_a_removed_item_but_not_point_it_at_one() {
    let user = Author::User;
    let mut c = base();
    let arrow = ItemId::new(5);
    c.apply(Op { by: user.clone(), at: AT, action: Action::Remove { item: ItemId::new(2) } })
        .unwrap();
    let shape = c.item(arrow).unwrap().shape().clone();
    let edit = |shape| Op { by: user.clone(), at: AT, action: Action::Edit { item: arrow, shape } };
    assert_eq!(c.apply(edit(shape)).unwrap(), Applied::Changed);

    let pointed = Shape::Arrow {
        from: End::free(Point::new(0.0, 0.0)),
        to: End::on(Point::new(1.0, 1.0), ItemId::new(4)),
        colour: Colour::BLACK,
    };
    assert_eq!(c.apply(edit(pointed)).unwrap_err().code(), "not_found");
}
