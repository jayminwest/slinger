//! The canvas, through its public interface only: build, apply, query.
#![cfg(test)]

use slinger_core::canvas::{
    Action, Applied, Author, Canvas, Colour, End, FIRST_PAGE, Item, ItemId, Op, Page, PageId,
    Point, Rect, Shape, Time,
};

const AT: Time = Time(1_700_000_000_000);

fn op(by: &Author, action: Action) -> Op {
    Op { by: by.clone(), at: AT, action }
}

fn agent(name: &str) -> Author {
    Author::Agent(name.into())
}

fn stroke(points: &[(f64, f64)]) -> Shape {
    Shape::Stroke {
        points: points.iter().map(|&(x, y)| Point::new(x, y)).collect(),
        colour: Colour::BLACK,
        width: 2.0,
    }
}

fn text(s: &str) -> Shape {
    Shape::Text { text: s.into(), frame: Rect::new(10.0, 20.0, 100.0, 40.0), colour: Colour::BLACK }
}

fn first_page(c: &Canvas) -> PageId {
    c.pages().next().unwrap().id()
}

fn add(c: &mut Canvas, by: &Author, shape: Shape) -> ItemId {
    let page = first_page(c);
    match c.apply(op(by, Action::Add { page, shape })).unwrap() {
        Applied::Added(id) => id,
        other => panic!("expected Added, got {other:?}"),
    }
}

fn add_page(c: &mut Canvas, name: &str) -> PageId {
    match c.apply(op(&Author::User, Action::PageAdd { name: name.into() })).unwrap() {
        Applied::PageAdded(id) => id,
        other => panic!("expected PageAdded, got {other:?}"),
    }
}

#[test]
fn a_new_canvas_has_one_empty_page() {
    let c = Canvas::new();
    let pages: Vec<_> = c.pages().map(|p| p.name().to_string()).collect();
    assert_eq!(pages, [FIRST_PAGE]);
    assert_eq!(c.items(first_page(&c)).count(), 0);
    assert_eq!(c, Canvas::default());
}

#[test]
fn add_puts_an_item_on_a_page_with_its_author_and_time() {
    let mut c = Canvas::new();
    let id = add(&mut c, &agent("claude"), text("hi"));
    let item = c.item(id).unwrap();
    assert_eq!(item.id(), id);
    assert_eq!(item.page(), first_page(&c));
    assert_eq!(item.author(), &agent("claude"));
    assert_eq!(item.time(), AT);
    assert_eq!(item.shape(), &text("hi"));
    assert_eq!(item.bounds(), Rect::new(10.0, 20.0, 100.0, 40.0));
}

#[test]
fn items_come_back_in_drawing_order_and_ids_are_never_reused() {
    let mut c = Canvas::new();
    let user = Author::User;
    let a = add(&mut c, &user, text("a"));
    let b = add(&mut c, &user, text("b"));
    c.apply(op(&user, Action::Remove { item: b })).unwrap();
    let d = add(&mut c, &user, text("d"));
    assert!(d > b && b > a);
    let order: Vec<_> = c.items(first_page(&c)).map(Item::id).collect();
    assert_eq!(order, [a, d]);
    assert_eq!(a.to_string(), format!("item {}", a.get()));
    assert_eq!(ItemId::new(a.get()), a);
}

#[test]
fn replaying_the_same_ops_builds_the_same_canvas() {
    let ops = |page: PageId| {
        vec![
            op(&Author::User, Action::Add { page, shape: stroke(&[(0.0, 0.0), (5.0, 5.0)]) }),
            op(&Author::User, Action::PageAdd { name: "two".into() }),
            op(&agent("claude"), Action::Add { page, shape: text("note") }),
            op(&Author::User, Action::Remove { item: ItemId::new(1) }),
        ]
    };
    let (mut one, mut two) = (Canvas::new(), Canvas::new());
    let page = first_page(&one);
    let a: Vec<_> = ops(page).into_iter().map(|o| one.apply(o).unwrap()).collect();
    let b: Vec<_> = ops(page).into_iter().map(|o| two.apply(o).unwrap()).collect();
    assert_eq!(a, b);
    assert_eq!(one, two);
}

#[test]
fn every_kind_of_shape_has_a_bounding_box() {
    let mut c = Canvas::new();
    let user = Author::User;
    let s = add(&mut c, &user, stroke(&[(10.0, 10.0), (30.0, 50.0), (20.0, 5.0)]));
    assert_eq!(c.item(s).unwrap().bounds(), Rect::new(9.0, 4.0, 22.0, 47.0));
    let frame = Rect::new(1.0, 2.0, 3.0, 4.0);
    let b = add(&mut c, &user, Shape::Box { frame, colour: Colour::rgb(255, 0, 0) });
    assert_eq!(c.item(b).unwrap().bounds(), frame);
    let arrow = Shape::Arrow {
        from: End::free(Point::new(50.0, 0.0)),
        to: End::on(Point::new(0.0, 30.0), b),
        colour: Colour::BLACK,
    };
    let a = add(&mut c, &user, arrow);
    assert_eq!(c.item(a).unwrap().bounds(), Rect::new(0.0, 0.0, 50.0, 30.0));
}

#[test]
fn add_rejects_a_missing_page_a_missing_target_and_shapes_that_cannot_be_drawn() {
    let mut c = Canvas::new();
    let page = first_page(&c);
    let user = Author::User;
    let missing_page = op(&user, Action::Add { page: PageId::new(99), shape: text("x") });
    assert_eq!(c.apply(missing_page).unwrap_err().code(), "not_found");

    let dangling = Shape::Arrow {
        from: End::free(Point::default()),
        to: End::on(Point::new(1.0, 1.0), ItemId::new(42)),
        colour: Colour::BLACK,
    };
    let err = c.apply(op(&user, Action::Add { page, shape: dangling })).unwrap_err();
    assert_eq!((err.code(), err.to_string().as_str()), ("not_found", "item 42 not found"));

    let bad = [
        stroke(&[]),
        stroke(&[(f64::NAN, 0.0)]),
        Shape::Stroke { points: vec![Point::default()], colour: Colour::BLACK, width: 0.0 },
        Shape::Box { frame: Rect::new(0.0, 0.0, -1.0, 1.0), colour: Colour::BLACK },
        Shape::Text {
            text: "t".into(),
            frame: Rect::new(f64::INFINITY, 0.0, 1.0, 1.0),
            colour: Colour::BLACK,
        },
        Shape::Arrow {
            from: End::free(Point::new(f64::NAN, 0.0)),
            to: End::free(Point::default()),
            colour: Colour::BLACK,
        },
    ];
    for shape in bad {
        let err = c.apply(op(&user, Action::Add { page, shape: shape.clone() })).unwrap_err();
        assert_eq!(err.code(), "invalid", "{shape:?}");
        assert!(err.hint().is_some());
    }
    assert_eq!(c.items(page).count(), 0, "a failed op changes nothing");
}

#[test]
fn edit_replaces_the_shape_and_keeps_author_and_time() {
    let mut c = Canvas::new();
    let claude = agent("claude");
    let id = add(&mut c, &claude, text("draft"));
    let later = Op {
        by: claude.clone(),
        at: Time(AT.0 + 5),
        action: Action::Edit { item: id, shape: text("final") },
    };
    assert_eq!(c.apply(later).unwrap(), Applied::Changed);
    let item = c.item(id).unwrap();
    assert_eq!(item.shape(), &text("final"));
    assert_eq!((item.author(), item.time()), (&claude, AT));
}

#[test]
fn edit_of_a_missing_item_is_not_found() {
    let mut c = Canvas::new();
    let err = c
        .apply(op(&Author::User, Action::Edit { item: ItemId::new(7), shape: text("x") }))
        .unwrap_err();
    assert_eq!(err.code(), "not_found");
    assert!(err.hint().is_some());
}

#[test]
fn move_shifts_every_kind_of_shape() {
    let mut c = Canvas::new();
    let user = Author::User;
    let by = Point::new(10.0, -5.0);
    let ink = add(&mut c, &user, stroke(&[(0.0, 0.0), (4.0, 4.0)]));
    let note = add(&mut c, &user, text("t"));
    let frame = add(
        &mut c,
        &user,
        Shape::Box { frame: Rect::new(0.0, 0.0, 2.0, 2.0), colour: Colour::BLACK },
    );
    let arrow = add(
        &mut c,
        &user,
        Shape::Arrow {
            from: End::free(Point::default()),
            to: End::on(Point::new(1.0, 1.0), note),
            colour: Colour::BLACK,
        },
    );
    for id in [ink, note, frame, arrow] {
        let before = c.item(id).unwrap().bounds();
        assert_eq!(c.apply(op(&user, Action::Move { item: id, by })).unwrap(), Applied::Changed);
        let after = c.item(id).unwrap().bounds();
        assert_eq!(after, Rect::new(before.x + 10.0, before.y - 5.0, before.w, before.h));
    }
    let Shape::Arrow { to, .. } = c.item(arrow).unwrap().shape() else { panic!("not an arrow") };
    assert_eq!(*to, End::on(Point::new(11.0, -4.0), note), "the end keeps pointing at its item");
}

#[test]
fn move_rejects_an_offset_that_is_not_a_number_and_a_missing_item() {
    let mut c = Canvas::new();
    let user = Author::User;
    let id = add(&mut c, &user, text("t"));
    let nan = op(&user, Action::Move { item: id, by: Point::new(f64::NAN, 0.0) });
    assert_eq!(c.apply(nan).unwrap_err().code(), "invalid");
    let gone = op(&user, Action::Move { item: ItemId::new(99), by: Point::new(1.0, 1.0) });
    assert_eq!(c.apply(gone).unwrap_err().code(), "not_found");
}

#[test]
fn remove_takes_an_item_off_the_canvas() {
    let mut c = Canvas::new();
    let id = add(&mut c, &Author::User, text("t"));
    assert_eq!(c.apply(op(&Author::User, Action::Remove { item: id })).unwrap(), Applied::Changed);
    assert!(c.item(id).is_none());
}

#[test]
fn removing_an_item_that_is_already_gone_succeeds() {
    let mut c = Canvas::new();
    let id = add(&mut c, &Author::User, text("t"));
    c.apply(op(&Author::User, Action::Remove { item: id })).unwrap();
    for by in [Author::User, agent("claude")] {
        assert_eq!(c.apply(op(&by, Action::Remove { item: id })).unwrap(), Applied::Unchanged);
    }
    let never = op(&Author::User, Action::Remove { item: ItemId::new(1000) });
    assert_eq!(c.apply(never).unwrap(), Applied::Unchanged);
}

#[test]
fn an_arrow_keeps_its_ends_after_the_item_it_points_at_is_removed() {
    let mut c = Canvas::new();
    let user = Author::User;
    let t = add(&mut c, &user, text("t"));
    let arrow = Shape::Arrow {
        from: End::free(Point::default()),
        to: End::on(Point::new(5.0, 5.0), t),
        colour: Colour::BLACK,
    };
    let a = add(&mut c, &user, arrow.clone());
    c.apply(op(&user, Action::Remove { item: t })).unwrap();
    assert_eq!(c.item(a).unwrap().shape(), &arrow);
}

#[test]
fn an_agent_cannot_change_the_users_ink() {
    let mut c = Canvas::new();
    let ink = add(&mut c, &Author::User, stroke(&[(0.0, 0.0), (1.0, 1.0)]));
    let before = c.clone();
    let claude = agent("claude");
    for action in [
        Action::Edit { item: ink, shape: stroke(&[(9.0, 9.0)]) },
        Action::Move { item: ink, by: Point::new(1.0, 0.0) },
        Action::Remove { item: ink },
    ] {
        let err = c.apply(op(&claude, action)).unwrap_err();
        assert_eq!(err.code(), "invalid");
        assert_eq!(err.to_string(), "agent claude may not change item 1 made by the user");
        assert!(err.hint().is_some());
    }
    assert_eq!(c, before);
}

#[test]
fn an_agent_changes_only_items_it_made() {
    let mut c = Canvas::new();
    let (claude, other) = (agent("claude"), agent("codex"));
    let mine = add(&mut c, &claude, text("mine"));
    let theirs = add(&mut c, &other, text("theirs"));
    let mv = |item| Action::Move { item, by: Point::new(1.0, 1.0) };
    assert_eq!(c.apply(op(&claude, mv(mine))).unwrap(), Applied::Changed);
    assert_eq!(c.apply(op(&claude, mv(theirs))).unwrap_err().code(), "invalid");
    assert_eq!(c.apply(op(&claude, Action::Remove { item: mine })).unwrap(), Applied::Changed);
}

#[test]
fn the_user_may_change_an_agents_items() {
    let mut c = Canvas::new();
    let id = add(&mut c, &agent("claude"), text("t"));
    let edit = Action::Edit { item: id, shape: text("fixed") };
    assert_eq!(c.apply(op(&Author::User, edit)).unwrap(), Applied::Changed);
    assert_eq!(c.item(id).unwrap().author(), &agent("claude"));
}

#[test]
fn an_agent_with_a_blank_name_is_rejected() {
    let mut c = Canvas::new();
    let page = first_page(&c);
    let err = c.apply(op(&agent("  "), Action::Add { page, shape: text("t") })).unwrap_err();
    assert_eq!(err.code(), "invalid");
}

#[test]
fn page_add_appends_a_named_page() {
    let mut c = Canvas::new();
    let id = add_page(&mut c, "  ideas ");
    let names: Vec<_> = c.pages().map(Page::name).collect();
    assert_eq!(names, [FIRST_PAGE, "ideas"]);
    assert_eq!(c.page(id).unwrap().name(), "ideas");
    assert_eq!(c.page_named("ideas").unwrap().id(), id);
    assert_eq!(id.to_string(), format!("page {}", id.get()));
}

#[test]
fn page_names_are_not_blank_and_not_taken() {
    let mut c = Canvas::new();
    let blank = op(&Author::User, Action::PageAdd { name: " ".into() });
    assert_eq!(c.apply(blank).unwrap_err().code(), "invalid");
    let taken = op(&Author::User, Action::PageAdd { name: FIRST_PAGE.into() });
    let err = c.apply(taken).unwrap_err();
    assert_eq!(err.code(), "conflict");
    assert!(err.hint().is_some());
}

#[test]
fn page_rename_changes_the_name() {
    let mut c = Canvas::new();
    let page = first_page(&c);
    let user = Author::User;
    let rename = |name: &str| op(&user, Action::PageRename { page, name: name.into() });
    assert_eq!(c.apply(rename("spec")).unwrap(), Applied::Changed);
    assert_eq!(c.page(page).unwrap().name(), "spec");
    assert_eq!(c.apply(rename("spec")).unwrap(), Applied::Unchanged);
    add_page(&mut c, "other");
    assert_eq!(c.apply(rename("other")).unwrap_err().code(), "conflict");
    let missing = op(&user, Action::PageRename { page: PageId::new(99), name: "x".into() });
    assert_eq!(c.apply(missing).unwrap_err().code(), "not_found");
}

#[test]
fn page_remove_takes_the_page_and_its_items() {
    let mut c = Canvas::new();
    let first = first_page(&c);
    let second = add_page(&mut c, "two");
    let user = Author::User;
    let on_second =
        match c.apply(op(&user, Action::Add { page: second, shape: text("t") })).unwrap() {
            Applied::Added(id) => id,
            other => panic!("{other:?}"),
        };
    let on_first = add(&mut c, &user, text("stay"));
    assert_eq!(c.apply(op(&user, Action::PageRemove { page: second })).unwrap(), Applied::Changed);
    assert!(c.page(second).is_none());
    assert!(c.item(on_second).is_none());
    assert_eq!(c.items(second).count(), 0);
    assert!(c.item(on_first).is_some());
    assert_eq!(c.pages().map(Page::id).collect::<Vec<_>>(), [first]);
}

#[test]
fn removing_a_page_that_is_already_gone_succeeds() {
    let mut c = Canvas::new();
    let second = add_page(&mut c, "two");
    let remove = op(&Author::User, Action::PageRemove { page: second });
    assert_eq!(c.apply(remove.clone()).unwrap(), Applied::Changed);
    assert_eq!(c.apply(remove).unwrap(), Applied::Unchanged);
}

#[test]
fn the_last_page_cannot_be_removed() {
    let mut c = Canvas::new();
    let page = first_page(&c);
    let err = c.apply(op(&Author::User, Action::PageRemove { page })).unwrap_err();
    assert_eq!(err.code(), "conflict");
    assert!(err.hint().is_some());
}

#[test]
fn an_agent_cannot_remove_a_page_that_holds_the_users_ink() {
    let mut c = Canvas::new();
    let claude = agent("claude");
    let second = add_page(&mut c, "two");
    let add_to = |by: &Author| op(by, Action::Add { page: second, shape: text("t") });
    c.apply(add_to(&claude)).unwrap();
    c.apply(add_to(&Author::User)).unwrap();
    let err = c.apply(op(&claude, Action::PageRemove { page: second })).unwrap_err();
    assert_eq!(err.code(), "invalid");
    assert!(c.page(second).is_some());

    let third = add_page(&mut c, "three");
    c.apply(op(&claude, Action::Add { page: third, shape: text("t") })).unwrap();
    let remove = op(&claude, Action::PageRemove { page: third });
    assert_eq!(c.apply(remove).unwrap(), Applied::Changed);
}
