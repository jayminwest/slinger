//! Building a packet, through the public interface: a canvas's log, a page,
//! a marker and a selection in; the packet out. The real board is the main
//! fixture.
#![cfg(test)]

use std::collections::BTreeMap;

use slinger_core::canvas::{
    Action, Applied, Author, Canvas, Colour, End, Item, ItemId, Marker, Op, Page, PageId, Point,
    Rect, Shape, Time,
};
use slinger_core::packet::{Ask, Header, Packet, VERSION, build, cluster};
use slinger_core::tldraw::import;

/// The owner's real board (SPEC B8).
const BOARD: &[u8] = include_bytes!("fixtures/idea.tldraw");
/// What the screenshots of the whole board cost (Spike B).
const SCREENSHOTS: u64 = 129_000;
const T0: u64 = 1_700_000_000_000;

/// The board as a log of operations that builds it, and its pages.
fn board() -> (Vec<Op>, Vec<PageId>) {
    let canvas = import(BOARD, Time(T0)).unwrap().canvas;
    let mut log = Vec::new();
    let mut fresh = Canvas::new();
    let mut page_of = BTreeMap::new();
    for (k, page) in canvas.pages().enumerate() {
        let action = if k == 0 {
            let first = fresh.pages().next().unwrap().id();
            Action::PageRename { page: first, name: page.name().into() }
        } else {
            Action::PageAdd { name: page.name().into() }
        };
        let op = Op { by: Author::User, at: Time(T0), action };
        let id = match fresh.apply(op.clone()).unwrap() {
            Applied::PageAdded(id) => id,
            _ => fresh.pages().next().unwrap().id(),
        };
        page_of.insert(page.id(), id);
        log.push(op);
    }
    let mut item_of: BTreeMap<ItemId, ItemId> = BTreeMap::new();
    let pages: Vec<PageId> = canvas.pages().map(Page::id).collect();
    let mut items: Vec<_> = pages.iter().flat_map(|p| canvas.items(*p)).collect();
    items.sort_by_key(|i| i.id());
    for item in items {
        let mut shape = item.shape().clone();
        if let Shape::Arrow { from, to, .. } = &mut shape {
            for end in [from, to] {
                end.item = end.item.and_then(|i| item_of.get(&i).copied());
            }
        }
        let action = Action::Add { page: page_of[&item.page()], shape };
        let op = Op { by: item.author().clone(), at: item.time(), action };
        let Applied::Added(id) = fresh.apply(op.clone()).unwrap() else { panic!("not added") };
        item_of.insert(item.id(), id);
        log.push(op);
    }
    (log, pages.iter().map(|p| page_of[p]).collect())
}

fn ask(page: PageId, since: u64) -> Ask<'static> {
    Ask { canvas: "idea", page, since: Marker(since), selection: &[] }
}

#[test]
fn the_whole_board_costs_under_half_the_screenshots() {
    let (log, pages) = board();
    let packets: Vec<Packet> = pages.iter().map(|p| build(&log, &ask(*p, 0)).unwrap()).collect();
    let tokens: u64 = packets.iter().map(Packet::tokens).sum();
    assert!(tokens < SCREENSHOTS / 2, "{tokens} tokens");
    for p in &packets {
        // The header's count is the packet's cost, its own digits included.
        assert_eq!(p.header.tokens, Some(p.tokens()));
        // Everything is new, so all the ink is in `new` with its images,
        // named in order. The user's text, boxes and arrows are in `page`.
        assert!(p.made.is_empty() && p.page.iter().all(|e| e.kind != "ink"));
        let named: Vec<&str> =
            p.new.iter().flat_map(|e| e.field("img").unwrap().split(',')).collect();
        let images: Vec<&str> = p.images.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(named, images);
    }
}

#[test]
fn no_raw_stroke_points_are_in_the_packet() {
    let (log, pages) = board();
    let mut fresh = Canvas::new();
    for op in &log {
        fresh.apply(op.clone()).unwrap();
    }
    let known = ["colour", "strokes", "by", "from", "to", "in", "img"];
    for page in pages {
        let p = build(&log, &ask(page, 0)).unwrap();
        let strokes = fresh.items(page).filter(|i| matches!(i.shape(), Shape::Stroke { .. }));
        let clusters = cluster(&fresh, strokes.map(Item::id)).len();
        let others =
            fresh.items(page).filter(|i| !matches!(i.shape(), Shape::Stroke { .. })).count();
        // One line a cluster or item, each with only the designed fields.
        assert_eq!(p.new.len() + p.made.len() + p.page.len(), clusters + others);
        for e in p.new.iter().chain(&p.made).chain(&p.page) {
            assert!(e.fields.iter().all(|(k, _)| known.contains(&k.as_str())), "{e:?}");
            assert!(e.fields.len() <= known.len(), "{e:?}");
        }
    }
}

/// A small page: an old note, then since the marker a new note near it and
/// a box, with each op its own number.
fn small() -> (Vec<Op>, PageId, u64) {
    let page = Canvas::new().pages().next().unwrap().id();
    let stroke = |x: f64, colour| Shape::Stroke {
        points: vec![Point::new(x, 0.0), Point::new(x + 20.0, 30.0)],
        colour,
        width: 2.0,
    };
    let add = |shape, minutes: u64| Op {
        by: Author::User,
        at: Time(T0 + minutes * 60_000),
        action: Action::Add { page, shape },
    };
    let mut log = vec![add(stroke(0.0, Colour::BLACK), 0), add(stroke(30.0, Colour::BLACK), 0)];
    let marker = log.len() as u64;
    log.push(add(stroke(600.0, Colour::rgb(255, 0, 0)), 60));
    let frame = Shape::Box { frame: Rect::new(-10.0, -10.0, 100.0, 60.0), colour: Colour::BLACK };
    log.push(add(frame, 60));
    let arrow = Shape::Arrow {
        from: End::on(Point::new(600.0, 0.0), ItemId::new(3)),
        to: End::on(Point::new(10.0, 0.0), ItemId::new(1)),
        colour: Colour::BLACK,
    };
    log.push(Op {
        by: Author::Agent("claude".into()),
        at: Time(T0),
        action: Action::Add { page, shape: arrow },
    });
    (log, page, marker)
}

#[test]
fn only_the_new_ink_has_images_and_the_header_names_the_range() {
    let (log, page, marker) = small();
    let p = build(&log, &ask(page, marker)).unwrap();
    let want = "\
slinger-packet 1 canvas=idea page=Page%201 from=3 to=5 scale=0.35 tokens=68
# new
c3 ink 599,-1 22x32 colour=red strokes=1 to=c1 img=c3.png
# made
a5 arrow 10,0 590x0 by=claude from=c3 to=c1
# page
b4 box -10,-10 100x60
c1 ink -1,-1 52x32 colour=black strokes=2 in=b4
";
    assert_eq!(p.text(), want);
    // Text and the one image, counted the way Packet::tokens counts.
    assert_eq!(p.header.tokens, Some(p.tokens()));
    let names: Vec<&str> = p.images.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, ["c3.png"]);
    assert!(p.images[0].png.starts_with(b"\x89PNG"));
}

#[test]
fn a_packet_with_nothing_new_is_valid() {
    let (log, page, _) = small();
    let p = build(&log, &ask(page, log.len() as u64)).unwrap();
    assert!(p.header.from > p.header.to);
    assert!(p.new.is_empty() && p.made.is_empty() && p.images.is_empty());
    assert_eq!(p.page.len(), 4);
    assert_eq!(Packet::read(&p.text()).unwrap(), p);
    // A marker past the end of the log is at the end.
    let past = build(&log, &ask(page, 99)).unwrap();
    assert_eq!(past, p);
}

#[test]
fn a_selection_narrows_the_page_to_it_plus_what_is_new() {
    let (log, page, marker) = small();
    // The second stroke of the old note selects the whole note.
    let selection = [ItemId::new(2)];
    let p = build(&log, &Ask { selection: &selection, ..ask(page, marker) }).unwrap();
    let ids =
        |es: &[slinger_core::packet::Entry]| es.iter().map(|e| e.id.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&p.new), ["c3"]);
    assert_eq!(ids(&p.made), ["a5"]);
    assert_eq!(ids(&p.page), ["c1"]);
    // A stroke is selected as its cluster, once; the new arrow by its own id.
    let selection = [ItemId::new(2), ItemId::new(5), ItemId::new(1)];
    let p = build(&log, &Ask { selection: &selection, ..ask(page, marker) }).unwrap();
    assert_eq!(p.selected, ["c1", "a5"]);
    assert_eq!(p.new_strokes(), 1);
    // Selecting something not on the page leaves the page section empty.
    let p = build(&log, &Ask { selection: &[ItemId::new(99)], ..ask(page, marker) }).unwrap();
    assert!(p.page.is_empty() && !p.new.is_empty() && p.selected.is_empty());
}

#[test]
fn a_page_not_on_the_canvas_is_not_found() {
    let (log, _, _) = small();
    let err = build(&log, &ask(PageId::new(7), 0)).unwrap_err();
    assert_eq!(err.code(), "not_found");
}

#[test]
fn a_packet_round_trips_through_its_text() {
    let (log, page, marker) = small();
    let mut p = build(&log, &ask(page, marker)).unwrap();
    p.header.canvas = "my \"canvas\" 100%".into();
    p.header.at = Some("/tmp/a dir/packet".into());
    p.made[0].fields.push(("by".into(), "Claude Code\n".into()));
    let text = p.text();
    assert!(text.starts_with("slinger-packet 1 canvas=my%20%22canvas%22%20100%25 page=Page%201"));
    let back = Packet::read(&text).unwrap();
    // A packet read back has no images, but keeps the count it was sent with.
    assert_eq!(back.header.tokens, p.header.tokens);
    assert_eq!(back, Packet { images: Vec::new(), ..p });
    assert_eq!(back.text(), text);
}

#[test]
fn the_stored_v1_packet_keeps_decoding() {
    let text = include_str!("fixtures/packet/packet-v1.txt");
    let p = Packet::read(text).unwrap();
    let header = Header {
        canvas: "idea".into(),
        page: "fable-5.2".into(),
        from: 41,
        to: 58,
        scale: 0.35,
        tokens: None,
        at: None,
    };
    assert_eq!(p.header, header);
    assert_eq!((p.new.len(), p.made.len(), p.page.len()), (2, 2, 4));
    let red = &p.new[1];
    assert_eq!((red.id.as_str(), red.kind.as_str()), ("c51", "ink"));
    assert_eq!((red.x, red.y, red.w, red.h), (120, 260, 300, 90));
    assert_eq!(red.field("to"), Some("t12"));
    assert_eq!(p.made[1].text.as_deref(), Some("Packet: images of new ink plus a text index."));
    assert_eq!(p.text(), text);
}

#[test]
fn a_reader_skips_what_a_newer_v1_writer_added() {
    let text = "\
slinger-packet 1 canvas=idea page=p from=1 to=2 scale=0.35 mood=calm
# new
c1 ink 0,0 1x1 colour=black strokes=1 shade=dark img=c1.png
# sketches
anything at all \"here
# made
s2 sticker 5,5 2x2
# page
";
    let p = Packet::read(text).unwrap();
    assert_eq!(p.new[0].field("shade"), Some("dark"));
    assert_eq!(p.made[0].kind, "sticker");
    assert_eq!(p.page, []);
}

#[test]
fn another_version_is_unsupported_and_garbage_is_invalid() {
    let newer = "slinger-packet 2 canvas=idea\n# new\n";
    let err = Packet::read(newer).unwrap_err();
    assert_eq!(err.code(), "unsupported_version");
    assert_eq!(VERSION, 1);
    let head = "slinger-packet 1 canvas=a page=b from=1 to=0 scale=0.35\n";
    for bad in [
        "",
        "hello\n",
        "slinger-packet 1 canvas=a\n# new\n# made\n# page\n",
        &format!("{head}# new\n# made\n"),
        &format!("{head}c1 ink 0,0 1x1\n# new\n# made\n# page\n"),
        &format!("{head}# new\nc1 ink 0,0\n# made\n# page\n"),
        &format!("{head}# new\nc1 ink 0,0 1x1 \"open\n# made\n# page\n"),
        &format!("{head}# new\nc1 ink 0,0 1x1 k=%zz\n# made\n# page\n"),
    ] {
        let err = Packet::read(bad).unwrap_err();
        assert_eq!(err.code(), "invalid", "{bad:?}");
    }
}
