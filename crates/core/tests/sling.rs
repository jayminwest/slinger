//! The sling message, through the public interface: a packet, a prompt and a
//! selection in; text and image blocks out. Snapshots hold the first and a
//! later message.
#![cfg(test)]

use slinger_core::packet::Packet;
use slinger_core::render::Image;
use slinger_core::sling::{Block, Message, Sling, VERSION, Verb, message};

const PACKET: &str = include_str!("fixtures/packet/packet-v1.txt");
const ALL: &[Verb] = &[Verb::Put, Verb::Crop];

/// The stored v1 packet, with a stand-in image for each `img=` name.
fn packet() -> Packet {
    let mut p = Packet::read(PACKET).unwrap();
    p.images = ["c42.png", "c51.png"]
        .into_iter()
        .map(|name| Image { name: name.into(), x: 0, y: 0, width: 4, height: 2, png: vec![1] })
        .collect();
    p
}

fn sling<'a>(packet: &'a Packet, selection: &'a [String], first: bool) -> Sling<'a> {
    Sling { packet, prompt: "What is missing here?", selection, first, verbs: ALL }
}

/// The message as text, each image as an `<image name>` line.
fn show(m: &Message) -> String {
    m.blocks
        .iter()
        .map(|b| match b {
            Block::Text(t) => t.clone(),
            Block::Image(i) => format!("<image {}>\n", i.name),
        })
        .collect()
}

fn ids(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn a_first_sling_matches_its_snapshot() {
    let p = packet();
    let selection = ids(&["c51", "t12"]);
    let m = message(&sling(&p, &selection, true));
    assert_eq!(show(&m), include_str!("fixtures/sling/first.txt"));
}

#[test]
fn a_later_sling_matches_its_snapshot() {
    let p = packet();
    let selection = ids(&["c51"]);
    let m = message(&sling(&p, &selection, false));
    assert_eq!(show(&m), include_str!("fixtures/sling/later.txt"));
}

#[test]
fn a_later_sling_gets_one_line_of_note() {
    let p = packet();
    let m = show(&message(&sling(&p, &[], false)));
    let note = m.split("# reply\n").nth(1).unwrap().split("# prompt\n").next().unwrap();
    assert_eq!(note.lines().count(), 1);
    let full = show(&message(&sling(&p, &[], true)));
    assert!(full.len() > m.len());
}

#[test]
fn each_image_follows_the_line_that_names_it() {
    let p = packet();
    let m = message(&sling(&p, &[], true));
    let names: Vec<&str> = m
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Image(i) => Some(i.name.as_str()),
            Block::Text(_) => None,
        })
        .collect();
    assert_eq!(names, ["c42.png", "c51.png"]);
    for pair in m.blocks.windows(2) {
        match pair {
            [Block::Text(t), Block::Image(i)] => {
                let last = t.lines().last().unwrap();
                assert!(last.contains(&format!("img={}", i.name)), "{last}");
            }
            [Block::Text(_), Block::Text(_)] => panic!("two text blocks in a row"),
            _ => {}
        }
    }
}

#[test]
fn an_empty_prompt_with_a_selection_is_valid() {
    let p = packet();
    let selection = ids(&["c42"]);
    let m = message(&Sling { prompt: "", ..sling(&p, &selection, true) });
    let text = show(&m);
    assert!(text.starts_with(&format!("slinger-sling {VERSION} canvas=idea selected=c42\n")));
    assert!(text.ends_with("# prompt\n"));
}

#[test]
fn the_packet_inside_reads_back() {
    let p = packet();
    let m = show(&message(&sling(&p, &[], true)));
    let inside = m.split_once('\n').unwrap().1.replace("<image c42.png>\n", "");
    let inside = inside.replace("<image c51.png>\n", "");
    let back = Packet::read(inside.split("# reply\n").next().unwrap()).unwrap();
    assert_eq!(back, Packet { images: Vec::new(), ..p });
}

#[test]
fn ids_not_in_the_packet_are_not_selected() {
    let p = packet();
    let selection = ids(&["c99", "t12", "x"]);
    let text = show(&message(&sling(&p, &selection, false)));
    assert!(text.starts_with("slinger-sling 1 canvas=idea selected=t12\n"));
    let none = show(&message(&sling(&p, &ids(&["c99"]), false)));
    assert!(none.starts_with("slinger-sling 1 canvas=idea\n"));
}

/// Every `slinger` command the note names, as `noun verb`.
fn commands(text: &str) -> Vec<String> {
    let mut out: Vec<String> = text
        .split("`slinger ")
        .skip(1)
        .map(|rest| rest.split(' ').take(2).collect::<Vec<_>>().join(" "))
        .collect();
    out.sort();
    out.dedup();
    out
}

#[test]
fn the_note_names_only_the_verbs_given() {
    let p = packet();
    for first in [true, false] {
        let with = |verbs: &[Verb]| {
            let s = Sling { verbs, ..sling(&p, &[], first) };
            commands(&show(&message(&s)))
        };
        assert_eq!(with(ALL), ["canvas crop", "canvas put"]);
        assert_eq!(with(&[Verb::Put]), ["canvas put"]);
        assert_eq!(with(&[Verb::Crop]), ["canvas crop"]);
        assert_eq!(with(&[]), Vec::<String>::new());
    }
    // A later sling with nothing to name has no note at all.
    let s = Sling { verbs: &[], ..sling(&p, &[], false) };
    assert!(!show(&message(&s)).contains("# reply"));
}

#[test]
fn a_packet_with_nothing_new_is_one_text_block() {
    let mut p = packet();
    p.new.clear();
    p.images.clear();
    let m = message(&sling(&p, &[], false));
    assert!(matches!(m.blocks.as_slice(), [Block::Text(_)]));
}

#[test]
fn a_reply_lands_for_the_last_sling_and_for_none_before_the_first() {
    use slinger_core::record::{BlobId, Body, Event, ReplyLanded, SlingSent, Tags};
    use slinger_core::sling::landed;
    let sent = |number| {
        let packet = BlobId::of(b"packet");
        let body = Body::SlingSent(SlingSent { prompt: String::new(), packet });
        let mut event = Event::new(0, "slinger", Tags::on("my-laptop"), body);
        event.number = Some(number);
        event
    };
    let items = vec!["i12".to_string()];
    assert_eq!(landed(&[], items.clone()), None);
    assert_eq!(
        landed(&[sent(3), sent(9)], items.clone()),
        Some(Body::ReplyLanded(ReplyLanded { sling: 9, items }))
    );
}
