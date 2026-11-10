//! A whole canvas stored as a new file (`Log::import`): what an import
//! becomes, read back with its pages, items, ids and arrow ends.
#![cfg(test)]

use serde_json::json;
use slinger_core::canvas::file::{Log, check_id, id_from_name};
use slinger_core::canvas::{
    Action, Applied, Author, Canvas, Colour, End, Op, PageId, Point, Rect, Shape, Time,
};
use slinger_core::tldraw;

const AT: Time = Time(1_762_790_400_000);
const BOARD: &[u8] = include_bytes!("fixtures/idea.tldraw");

fn apply(canvas: &mut Canvas, by: Author, action: Action) -> Applied {
    canvas.apply(Op { by, at: AT, action }).unwrap()
}

#[test]
fn the_real_board_is_stored_whole() {
    let imported = tldraw::import(BOARD, AT).unwrap().canvas;
    let (log, bytes) = Log::import("idea", &imported, AT).unwrap();
    let (reread, torn) = Log::open(bytes.as_bytes()).unwrap();
    assert_eq!(torn, 0);
    assert_eq!(reread.canvas(), log.canvas());
    let canvas = log.canvas();

    let pages = |c: &Canvas| c.pages().map(|p| (p.id(), p.name().to_owned())).collect::<Vec<_>>();
    assert_eq!(pages(canvas), pages(&imported));
    let ids = |c: &Canvas| {
        c.pages().flat_map(|p| c.items(p.id()).map(|i| (i.id(), i.page()))).collect::<Vec<_>>()
    };
    assert_eq!(ids(canvas), ids(&imported));
    let bound = canvas
        .pages()
        .flat_map(|p| canvas.items(p.id()))
        .filter_map(|i| match i.shape() {
            Shape::Arrow { from, to, .. } => Some(from.item.iter().chain(&to.item).count()),
            _ => None,
        })
        .sum::<usize>();
    assert_eq!(bound, 91);
    // Every line is numbered: 4 page lines, the items, and a set per bound arrow.
    let lines = u64::try_from(bytes.lines().count()).unwrap();
    assert_eq!(log.next(), lines);
}

#[test]
fn ids_makers_times_arrow_ends_and_a_removed_first_page_survive() {
    let mut canvas = Canvas::new();
    let Applied::PageAdded(notes) =
        apply(&mut canvas, Author::User, Action::PageAdd { name: "notes".into() })
    else {
        panic!("no page")
    };
    let free = |x| End::free(Point::new(x, 0.0));
    let arrow = Shape::Arrow { from: free(0.0), to: free(10.0), colour: Colour::BLACK };
    let ada = Author::Agent("ada".into());
    let Applied::Added(a) =
        apply(&mut canvas, ada.clone(), Action::Add { page: notes, shape: arrow })
    else {
        panic!("no arrow")
    };
    let frame = Rect::new(20.0, 0.0, 5.5, 3.0);
    let bx = Shape::Box { frame, colour: Colour::rgb(200, 0, 0) };
    let Applied::Added(b) =
        apply(&mut canvas, Author::User, Action::Add { page: notes, shape: bx })
    else {
        panic!("no box")
    };
    // The arrow points at an item made after it.
    let to = End::on(Point::new(10.0, 0.0), b);
    let bound = Shape::Arrow { from: free(0.0), to, colour: Colour::BLACK };
    apply(&mut canvas, ada, Action::Edit { item: a, shape: bound });
    apply(&mut canvas, Author::User, Action::PageRemove { page: PageId::new(1) });

    let (log, _) = Log::import("copy", &canvas, Time(5)).unwrap();
    assert_eq!(log.canvas(), &canvas);
    let shown = log.show(None).unwrap();
    assert_eq!(shown.pages, [json!({ "id": "p2", "name": "notes" })]);
    assert_eq!(shown.items[0]["by"], "agent:ada");
    assert_eq!(shown.items[0]["to"], "i2");
}

#[test]
fn an_import_needs_a_canvas_id() {
    assert_eq!(Log::import("../x", &Canvas::new(), AT).unwrap_err().code(), "invalid");
}

#[test]
fn a_name_becomes_a_canvas_id() {
    for (name, id) in [
        ("idea", "idea"),
        ("My Board (2)", "my-board-2"),
        ("..hidden.v1", "hidden.v1"),
        ("Ünïcode", "n-code"),
        ("", "canvas"),
        ("!!!", "canvas"),
        (&"x".repeat(80), &"x".repeat(64)),
    ] {
        assert_eq!(id_from_name(name), id, "{name}");
        check_id(&id_from_name(name)).unwrap();
    }
}
