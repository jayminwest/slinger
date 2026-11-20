//! Lets an agent drive the window (`slinger-app --drive SOCKET`,
//! slinger-338dfc). Each connection to the socket carries one request, one
//! line of JSON with an `op`, and gets one line back: `ok` and what the
//! window `said`, or the `error`. A request becomes the events `platform`
//! hands in for real input, so it enters where a key or the pen does; the
//! scene comes back as text and the window as a PNG. `parse` decides; the
//! thread only reads and writes.

mod outline;

use std::ffi::CStr;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use slinger_control::{DriveAsk, DriveButton, DriveReply, PenAt, PenFlags};

pub use self::outline::{menu, outline};
use crate::commands::context::Popup;
use crate::commands::menu::Action;
use crate::commands::{
    Chord, Command, DELETE, DOWN, END, FORWARD_DELETE, HOME, LEFT, RIGHT, SHIFT, UP,
};
use crate::layout::{Point, Size};
use crate::scene::Scene;
use crate::scene::ink::Pen;
use crate::scene::paste::{Clip, Edit};
use crate::scene::secondary::{Button, Phase};
use crate::{Event, Step};

/// What a request asks of the window.
#[derive(Debug, PartialEq)]
pub enum Ask {
    /// Hand these events in, in order. One with a text is a key press: the
    /// text goes in as typed when the key was passed on.
    Send(Vec<(Event, Option<String>)>),
    /// The scene the window shows, as text: its outline, or all of it as
    /// Rust prints it (`debug`).
    Scene { debug: bool },
    /// The window's content as a PNG in this file.
    Shot(PathBuf),
    /// Make the window's content this size.
    Resize(Size),
    /// Paste what the driven window last cut or copied: its own clipboard,
    /// never the user's.
    Paste,
    /// The text the driven window last cut or copied.
    Clip,
    /// Choose the item of this title in the context menu the window would
    /// have popped, as a pick in the real menu does.
    Pick(String),
}

/// Keeps `last`, the scene a driven window shows, up to date with `step`,
/// the answer to an event: a scene drawn replaces it. An entry's field
/// shows its own typing with no draw, so the text typed into it (`typed`,
/// an [`Event::Typed`]'s) goes into `last`'s entry (slinger-16d634).
pub fn shown(last: &mut Scene, typed: Option<String>, step: &Step) {
    match step {
        Step::Draw(scene) | Step::Through(scene) | Step::Copy(_, scene) => {
            last.clone_from(scene);
        }
        _ => {
            if let (Some(text), Some(entry)) = (typed, last.entry.as_mut()) {
                entry.text = text;
            }
        }
    }
}

/// What a key's text does in an entry's field that holds the keyboard.
#[derive(Debug, PartialEq, Eq)]
pub enum Field<'a> {
    /// Goes in as typed.
    Insert(&'a str),
    /// Runs the field editor's command of this name, as the key does.
    Command(&'static CStr),
    /// Nothing: the key types no character and the field has no command for it.
    Nothing,
}

/// The field editor's command for each key that types no character.
const FIELD_COMMANDS: [(char, &CStr); 8] = [
    (DELETE, c"deleteBackward:"),
    (FORWARD_DELETE, c"deleteForward:"),
    (LEFT, c"moveLeft:"),
    (RIGHT, c"moveRight:"),
    (UP, c"moveUp:"),
    (DOWN, c"moveDown:"),
    (HOME, c"moveToBeginningOfLine:"),
    (END, c"moveToEndOfLine:"),
];

/// What `text`, a passed-on key's, does in an entry's field: delete or an
/// arrow runs its command, never a control character typed in
/// (slinger-02f671).
pub fn into_field(text: &str) -> Field<'_> {
    let mut chars = text.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && let Some((_, command)) = FIELD_COMMANDS.iter().find(|(key, _)| *key == c)
    {
        return Field::Command(command);
    }
    // Control characters, and the code points macOS gives function keys.
    let typeless = |c: char| c.is_control() || ('\u{f700}'..='\u{f8ff}').contains(&c);
    if text.is_empty() || text.chars().any(typeless) { Field::Nothing } else { Field::Insert(text) }
}

/// What choosing the item titled `title` in `popup`, the context menu a
/// driven window would have popped, does: the item's action; else why
/// it cannot (no menu, no such item, a grey one).
pub fn pick(popup: Option<&Popup>, title: &str) -> Result<Action, String> {
    let popup = popup.ok_or("no context menu is open: a right click opens one")?;
    let mut named = popup.items.iter().filter(|i| i.action != Action::Separator);
    let Some(item) = named.clone().find(|i| i.title == title) else {
        let titles: Vec<&str> = named.by_ref().map(|i| i.title).collect();
        return Err(format!("no item {title:?} in the menu: {}", titles.join(", ")));
    };
    if !item.enabled {
        return Err(format!("{title:?} is grey: it cannot act now"));
    }
    Ok(item.action)
}

/// Seconds between the points of a `stroke`.
const STEP: f64 = 0.01;

/// What the request `line` asks, its pen events made at `when` (seconds);
/// else what is wrong with it.
pub fn parse(line: &str, when: f64) -> Result<Ask, String> {
    let one = |event| Ok(Ask::Send(vec![(event, None)]));
    match DriveAsk::read(line)? {
        DriveAsk::Scene { debug } => Ok(Ask::Scene { debug }),
        DriveAsk::Shot { path } => Ok(Ask::Shot(path)),
        DriveAsk::Resize { w, h } => Ok(Ask::Resize(Size { width: w, height: h })),
        DriveAsk::Key { chord, text } => Ok(Ask::Send(vec![key(&chord, text.as_deref())?])),
        DriveAsk::Type { text } => Ok(Ask::Send(text.chars().map(typed).collect())),
        DriveAsk::Clip => Ok(Ask::Clip),
        DriveAsk::Pick { title } => Ok(Ask::Pick(title)),
        DriveAsk::Paste { text: None } => Ok(Ask::Paste),
        DriveAsk::Paste { text: Some(text) } => {
            one(Event::Edit(Edit::Paste(Clip::new(Some(text), None))))
        }
        DriveAsk::Stroke { points, pen: flags } => {
            stroke(&points, |at, i| pen(flags, at, when + STEP * i))
        }
        DriveAsk::Press(at) => one(penned(&at, Phase::Down, when)),
        DriveAsk::Drag(at) => one(penned(&at, Phase::Drag, when)),
        DriveAsk::Lift(at) => one(penned(&at, Phase::Up, when)),
        DriveAsk::Tick => one(Event::Tick),
        DriveAsk::Move { x, y } => one(Event::Moved(Point { x, y })),
        DriveAsk::Scroll { x, y, dx, dy } => one(Event::Scroll { at: Point { x, y }, dx, dy }),
        DriveAsk::Zoom { x, y, factor } => one(Event::Zoom { at: Point { x, y }, factor }),
        DriveAsk::Space { down } => one(Event::Space(down)),
        DriveAsk::Proximity { near, eraser } => one(Event::Proximity { near, eraser }),
        DriveAsk::Copy => one(Event::Edit(Edit::Copy)),
        DriveAsk::Cut => one(Event::Edit(Edit::Cut)),
        DriveAsk::Drop { x, y, path } => one(Event::Dropped(Point { x, y }, vec![path])),
        DriveAsk::Command { name } => one(Event::Command(
            Command::named(&name).ok_or_else(|| format!("no command {name:?}"))?,
        )),
    }
}

/// A pen event at `at` with what `flags` say of it: a mouse's unless `tablet`.
fn pen(flags: PenFlags, at: Point, when: f64) -> Pen {
    let PenFlags { tablet, shift, touch, eraser } = flags;
    Pen { at, when, touch, eraser, shift, tablet }
}

/// The press, drag or lift (`phase`) at `ask`: a button's for the right or
/// middle one, or the left with control held, as macOS takes a ctrl+click;
/// else the pen's.
fn penned(ask: &PenAt, phase: Phase, when: f64) -> Event {
    let at = Point { x: ask.x, y: ask.y };
    let button = match (ask.button, ask.control) {
        (DriveButton::Left, false) => None,
        (DriveButton::Left | DriveButton::Right, _) => Some(Button::Secondary),
        (DriveButton::Middle, _) => Some(Button::Middle),
    };
    let pen = pen(ask.pen, at, when);
    match (button, phase) {
        (Some(button), phase) => Event::Button { button, phase, at },
        (None, Phase::Down) => Event::Press(pen),
        (None, Phase::Drag) => Event::Drag(pen),
        (None, Phase::Up) => Event::Lift(pen),
    }
}

/// The press of `chord` ("cmd+j") and the `text` it types when passed on:
/// by default the key's character (upper case with shift), and none with
/// command or control held.
fn key(chord: &str, text: Option<&str>) -> Result<(Event, Option<String>), String> {
    let parsed = Chord::parse(chord).ok_or_else(|| format!("no chord {chord:?}"))?;
    let shift = chord.split('+').any(|part| part == "shift");
    let own = || {
        let key = parsed.key();
        let key = if shift { key.to_ascii_uppercase() } else { key };
        parsed.types().then(|| key.to_string())
    };
    Ok((Event::Key(parsed), text.map(str::to_string).or_else(own)))
}

/// The key press that types `c`; a new line is the return key's.
fn typed(c: char) -> (Event, Option<String>) {
    let c = if c == '\n' { '\r' } else { c };
    let mods = if c.is_uppercase() { SHIFT } else { 0 };
    (Event::Key(Chord::new(mods, c)), Some(c.to_string()))
}

/// A press at the first of `points`, a drag to each other, and a lift at
/// the last.
fn stroke(points: &[[f64; 2]], pen: impl Fn(Point, f64) -> Pen) -> Result<Ask, String> {
    if points.is_empty() {
        return Err("stroke needs \"points\": [[x, y], ...]".into());
    }
    let points: Vec<Point> = points.iter().map(|&[x, y]| Point { x, y }).collect();
    let mut events = Vec::new();
    let mut i = 0.0;
    for (n, &at) in points.iter().enumerate() {
        events.push(if n == 0 { Event::Press(pen(at, i)) } else { Event::Drag(pen(at, i)) });
        i += 1.0;
    }
    events.extend(points.last().map(|&at| Event::Lift(pen(at, i))));
    Ok(Ask::Send(events.into_iter().map(|e| (e, None)).collect()))
}

/// Listens on `socket` (a file there is replaced; a path of any length) on a thread of its own
/// and hands each request to `hand`, which answers what the window said.
///
/// # Errors
///
/// The socket cannot be made.
pub fn listen(
    socket: &Path,
    hand: impl Fn(Ask) -> Result<String, String> + Send + 'static,
) -> std::io::Result<()> {
    // A socket left by a run that ended is in the way of the new one.
    let _ = std::fs::remove_file(socket);
    let listener = slinger_control::bind_socket(socket)?;
    let start = Instant::now();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut line = String::new();
            if BufReader::new(&stream).read_line(&mut line).is_err() {
                continue;
            }
            let reply = match parse(&line, start.elapsed().as_secs_f64()).and_then(&hand) {
                Ok(said) => DriveReply::Said(said),
                Err(error) => DriveReply::Refused(error),
            };
            // A client that went away gets no reply.
            let _ = writeln!(&stream, "{}", reply.line());
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::CMD;

    fn sent(line: &str) -> Vec<(Event, Option<String>)> {
        match parse(line, 5.0) {
            Ok(Ask::Send(events)) => events,
            other => panic!("{line}: {other:?}"),
        }
    }

    #[test]
    fn a_key_types_its_character_unless_command_is_held() {
        let j = sent(r#"{"op":"key","chord":"cmd+j"}"#);
        assert_eq!(j, [(Event::Key(Chord::new(CMD, 'j')), None)]);
        let a = sent(r#"{"op":"key","chord":"shift+a"}"#);
        assert_eq!(a, [(Event::Key(Chord::new(SHIFT, 'a')), Some("A".into()))]);
        let esc = sent(r#"{"op":"key","chord":"escape","text":"\u001b"}"#);
        assert_eq!(esc[0].1.as_deref(), Some("\u{1b}"));
        assert_eq!(sent(r#"{"op":"type","text":"Hi\n"}"#).len(), 3);
    }

    #[test]
    fn a_key_that_types_no_character_runs_the_field_s_command() {
        assert_eq!(into_field("a"), Field::Insert("a"));
        assert_eq!(into_field("\u{7f}"), Field::Command(c"deleteBackward:"));
        assert_eq!(into_field("\u{f728}"), Field::Command(c"deleteForward:"));
        assert_eq!(into_field("\u{f702}"), Field::Command(c"moveLeft:"));
        for nothing in ["", "\t", "a\u{7f}", "\u{f704}"] {
            assert_eq!(into_field(nothing), Field::Nothing, "{nothing:?}");
        }
        let delete = sent(r#"{"op":"key","chord":"delete"}"#);
        assert_eq!(into_field(delete[0].1.as_deref().unwrap()), Field::Command(c"deleteBackward:"));
    }

    #[test]
    fn a_stroke_is_a_press_its_drags_and_a_lift() {
        let events = sent(r#"{"op":"stroke","points":[[1,2],[3,4]],"tablet":true}"#);
        let pen = |x, y, when| Pen {
            at: Point { x, y },
            when,
            touch: false,
            eraser: false,
            shift: false,
            tablet: true,
        };
        let want = [
            Event::Press(pen(1.0, 2.0, 5.0)),
            Event::Drag(pen(3.0, 4.0, 5.0 + STEP)),
            Event::Lift(pen(3.0, 4.0, 5.0 + 2.0 * STEP)),
        ];
        assert_eq!(events, want.map(|e| (e, None)));
    }

    #[test]
    fn a_right_middle_or_control_press_is_a_button_s() {
        let at = Point { x: 1.0, y: 2.0 };
        let button = |button, phase| [(Event::Button { button, phase, at }, None)];
        let right = sent(r#"{"op":"press","x":1,"y":2,"button":"right"}"#);
        assert_eq!(right, button(Button::Secondary, Phase::Down));
        let control = sent(r#"{"op":"drag","x":1,"y":2,"control":true}"#);
        assert_eq!(control, button(Button::Secondary, Phase::Drag));
        let middle = sent(r#"{"op":"lift","x":1,"y":2,"button":"middle"}"#);
        assert_eq!(middle, button(Button::Middle, Phase::Up));
        let left = sent(r#"{"op":"press","x":1,"y":2,"button":"left"}"#);
        assert!(matches!(left[0].0, Event::Press(_)), "the left button is the pen's");
        let wrong = parse(r#"{"op":"press","x":1,"y":2,"button":"fourth"}"#, 0.0);
        assert!(wrong.unwrap_err().contains("no button"));
    }

    #[test]
    fn a_menu_says_its_items_and_a_pick_by_title_runs_one_that_can_act() {
        use crate::commands::Keymap;
        use crate::commands::able::Facts;
        use crate::commands::context::{Subject, items};
        let facts = Facts { panes: 1, canvas: true, pages: 1, ..Facts::default() };
        let items =
            items(&Subject::Pane { ui: false }, &Keymap::default(), Command::ToolPen, &facts);
        let popup = Popup { at: Point { x: 10.0, y: 20.0 }, items, subject: Subject::Canvas };
        let said = menu(&popup);
        assert_eq!(said.lines().next(), Some("menu 10,20"));
        assert!(said.contains("  \"Split right\" ⌘D\n"), "{said}");
        assert!(said.contains("\"Close pane\" ⌘W grey"), "{said}");
        assert_eq!(pick(Some(&popup), "Split down"), Ok(Action::Run(Command::SplitDown)));
        assert!(pick(Some(&popup), "Close pane").unwrap_err().contains("grey"));
        let none = pick(Some(&popup), "Fly").unwrap_err();
        assert!(none.contains("Split right, Split down, Close pane"), "{none}");
        assert!(pick(None, "Split down").unwrap_err().contains("no context menu"));
        assert_eq!(
            parse(r#"{"op":"pick","title":"New text"}"#, 0.0),
            Ok(Ask::Pick("New text".into()))
        );
    }

    #[test]
    fn each_line_slinger_drive_writes_is_a_request() {
        let golden = include_str!("../../control/tests/fixtures/drive.jsonl");
        for line in golden.lines() {
            assert!(parse(line, 0.0).is_ok(), "{line}: {:?}", parse(line, 0.0));
        }
        let near = parse(r#"{"op":"proximity","near":true}"#, 0.0);
        assert_eq!(
            near,
            Ok(Ask::Send(vec![(Event::Proximity { near: true, eraser: false }, None)]))
        );
    }

    #[test]
    fn a_request_that_is_wrong_says_how() {
        assert_eq!(parse(r#"{"op":"scene"}"#, 0.0), Ok(Ask::Scene { debug: false }));
        assert_eq!(parse(r#"{"op":"paste"}"#, 0.0), Ok(Ask::Paste));
        assert_eq!(parse(r#"{"op":"clip"}"#, 0.0), Ok(Ask::Clip));
        let given = Edit::Paste(Clip::new(Some("a".into()), None));
        assert_eq!(sent(r#"{"op":"paste","text":"a"}"#), [(Event::Edit(given), None)]);
        assert_eq!(sent(r#"{"op":"cut"}"#), [(Event::Edit(Edit::Cut), None)]);
        for (line, said) in [
            ("x", "not JSON"),
            ("{}", "no \"op\""),
            (r#"{"op":"fly"}"#, "no op"),
            (r#"{"op":"press","x":1}"#, "number \"y\""),
            (r#"{"op":"key","chord":"hyper+x"}"#, "no chord"),
            (r#"{"op":"command","name":"nope"}"#, "no command"),
            (r#"{"op":"stroke","points":[]}"#, "points"),
        ] {
            let error = parse(line, 0.0).unwrap_err();
            assert!(error.contains(said), "{line}: {error}");
        }
    }
}
