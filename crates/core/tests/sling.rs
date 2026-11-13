//! The sling message, through the public interface: a packet, a prompt and a
//! selection in; text and image blocks out. Snapshots hold the first, a
//! later and a listener's first message; the CLI's tests hold the commands
//! they show to the CLI (crates/cli/src/tests/guide.rs).
#![cfg(test)]

use slinger_core::packet::Packet;
use slinger_core::render::Image;
use slinger_core::sling::{Block, Message, Sling, VERSION, Verb, message};

const PACKET: &str = include_str!("fixtures/packet/packet-v1.txt");
const ALL: &[Verb] = &[Verb::Put, Verb::Crop, Verb::Ui];

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
    Sling { packet, prompt: "What is missing here?", selection, first, verbs: ALL, listener: None }
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
fn a_listeners_first_sling_matches_its_snapshot() {
    use slinger_core::sling::Listener;
    let mut p = packet();
    p.header.at = Some("/data/packets/ab".into());
    let listener = Some(Listener { agent: "scout", workspace: "notes" });
    let m = message(&Sling { listener, ..sling(&p, &ids(&["c51"]), true) });
    assert_eq!(m.text(), include_str!("fixtures/sling/listener.txt"));
}

#[test]
fn a_message_counts_its_wrapper_and_its_images_by_the_packets_rule() {
    let mut p = packet();
    p.count();
    let selection = ids(&["c51"]);
    let first = message(&sling(&p, &selection, true));
    let later = message(&sling(&p, &selection, false));
    let chars = show(&first).chars().count() - "<image c42.png>\n<image c51.png>\n".len();
    // Each stand-in image is 8 pixels: one token.
    assert_eq!(first.tokens(), (chars as u64).div_ceil(4) + 2);
    assert!(first.tokens() > later.tokens(), "the whole note costs more");
    assert!(later.tokens() > p.tokens(), "the wrapper counts");
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

/// Every `slinger` command the note names, as `noun verb` (`guide` alone).
fn commands(text: &str) -> Vec<String> {
    let mut out: Vec<String> = text
        .split("`slinger ")
        .skip(1)
        .map(|rest| rest.split('`').next().unwrap_or(rest))
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
        // The whole note ends by pointing at the guide, when there is a verb.
        let guide = |mut names: Vec<&'static str>| {
            if first {
                names.push("guide");
            }
            names.sort_unstable();
            names
        };
        assert_eq!(with(ALL), guide(vec!["canvas crop", "canvas put", "ui open", "ui put"]));
        assert_eq!(with(&[Verb::Put]), guide(vec!["canvas put"]));
        assert_eq!(with(&[Verb::Crop]), guide(vec!["canvas crop"]));
        assert_eq!(with(&[Verb::Ui]), guide(vec!["ui open", "ui put"]));
        assert_eq!(with(&[]), Vec::<String>::new());
    }
    // A later sling with nothing to name has no note at all.
    let s = Sling { verbs: &[], ..sling(&p, &[], false) };
    assert!(!show(&message(&s)).contains("# reply"));
}

#[test]
fn a_listeners_sling_names_its_agent_and_says_what_a_listener_does_itself() {
    use slinger_core::sling::Listener;
    let mut p = packet();
    p.header.at = Some("/data/packets/ab".into());
    let listener = Some(Listener { agent: "scout", workspace: "notes" });
    for first in [true, false] {
        let m = message(&Sling { listener, ..sling(&p, &ids(&["c51"]), first) });
        let text = m.text();
        assert!(text.starts_with("slinger-sling 1 canvas=idea agent=scout selected=c51\n"));
        assert!(text.lines().nth(1).unwrap().contains(" at=/data/packets/ab"), "{text}");
        // The images are files: the text names them and holds none.
        assert_eq!(
            text,
            show(&m).replace("<image c42.png>\n", "").replace("<image c51.png>\n", "")
        );
        assert!(text.contains("img=c42.png") && text.ends_with("# prompt\nWhat is missing here?"));
        let mut named =
            vec!["canvas crop", "canvas put", "session listen", "status set", "ui open", "ui put"];
        if first {
            named.insert(2, "guide");
        }
        assert_eq!(commands(&text), named, "{text}");
        assert!(text.contains("`slinger status set done --agent scout`"));
        assert!(text.contains("`slinger session listen notes --pid $PPID`"));
        assert!(!first || text.contains("`--agent scout` to each `canvas put`, `ui open` and"));
    }
    // With no verb to name, a later sling still tells a listener what to do.
    let bare = Sling { listener, verbs: &[], ..sling(&p, &[], false) };
    let text = message(&bare).text();
    assert!(text.contains("# reply\nYou took this sling by listening, as agent `scout`."));
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
        let body = Body::SlingSent(SlingSent {
            prompt: String::new(),
            packet,
            images: Vec::new(),
            session: None,
        });
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

#[test]
fn a_sling_is_first_in_a_conversation_none_of_the_agents_slings_went_into() {
    use slinger_core::record::{BlobId, Body, Event, SlingSent, Tags};
    use slinger_core::sling::first;
    let sent = |session: Option<&str>| {
        let body = Body::SlingSent(SlingSent {
            prompt: String::new(),
            packet: BlobId::of(b"packet"),
            images: Vec::new(),
            session: session.map(str::to_string),
        });
        Event::new(0, "slingerd", Tags::on("my-laptop"), body)
    };
    assert!(first(&[], "s1"));
    assert!(first(&[sent(None), sent(Some("s0"))], "s1"), "another conversation, or none named");
    assert!(!first(&[sent(Some("s0")), sent(Some("s1"))], "s1"));
}
