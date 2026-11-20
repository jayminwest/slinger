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

use serde_json::{Value, json};

pub use self::outline::outline;
use crate::commands::{
    Chord, Command, DELETE, DOWN, END, FORWARD_DELETE, HOME, LEFT, RIGHT, SHIFT, UP,
};
use crate::layout::{Point, Size};
use crate::scene::Scene;
use crate::scene::ink::Pen;
use crate::scene::paste::{Clip, Edit};
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

/// Seconds between the points of a `stroke`.
const STEP: f64 = 0.01;

/// What the request `line` asks, its pen events made at `when` (seconds);
/// else what is wrong with it.
pub fn parse(line: &str, when: f64) -> Result<Ask, String> {
    let ask: Value = serde_json::from_str(line).map_err(|e| format!("not JSON: {e}"))?;
    let op = ask["op"].as_str().ok_or("no \"op\"")?;
    match op {
        "scene" => Ok(Ask::Scene { debug: ask["debug"].as_bool().unwrap_or(false) }),
        "shot" => Ok(Ask::Shot(PathBuf::from(text(&ask, "path")?))),
        "resize" => Ok(Ask::Resize(Size { width: num(&ask, "w")?, height: num(&ask, "h")? })),
        "key" => Ok(Ask::Send(vec![key(text(&ask, "chord")?, ask["text"].as_str())?])),
        "type" => Ok(Ask::Send(text(&ask, "text")?.chars().map(typed).collect())),
        "clip" => Ok(Ask::Clip),
        "paste" if ask["text"].is_null() => Ok(Ask::Paste),
        "stroke" => stroke(&ask["points"], |at, i| pen(&ask, at, when + STEP * i)),
        _ => Ok(Ask::Send(vec![(event(op, &ask, when)?, None)])),
    }
}

/// The number `ask` has under `key`.
fn num(ask: &Value, key: &str) -> Result<f64, String> {
    ask[key].as_f64().ok_or_else(|| format!("it needs a number \"{key}\""))
}

/// The string `ask` has under `key`.
fn text<'a>(ask: &'a Value, key: &str) -> Result<&'a str, String> {
    ask[key].as_str().ok_or_else(|| format!("it needs a string \"{key}\""))
}

/// The point `ask` names, in the window from its top left.
fn at(ask: &Value) -> Result<Point, String> {
    Ok(Point { x: num(ask, "x")?, y: num(ask, "y")? })
}

/// A pen event at `at` with what `ask` says of it: a mouse's unless `tablet`.
fn pen(ask: &Value, at: Point, when: f64) -> Pen {
    let flag = |key: &str| ask[key].as_bool().unwrap_or(false);
    Pen {
        at,
        when,
        touch: flag("touch"),
        eraser: flag("eraser"),
        shift: flag("shift"),
        tablet: flag("tablet"),
    }
}

/// The one event `op` names, from the fields of `ask`.
fn event(op: &str, ask: &Value, when: f64) -> Result<Event, String> {
    let flag = |key: &str| ask[key].as_bool().unwrap_or(false);
    Ok(match op {
        "tick" => Event::Tick,
        "move" => Event::Moved(at(ask)?),
        "press" => Event::Press(pen(ask, at(ask)?, when)),
        "drag" => Event::Drag(pen(ask, at(ask)?, when)),
        "lift" => Event::Lift(pen(ask, at(ask)?, when)),
        "scroll" => Event::Scroll { at: at(ask)?, dx: num(ask, "dx")?, dy: num(ask, "dy")? },
        "zoom" => Event::Zoom { at: at(ask)?, factor: num(ask, "factor")? },
        "space" => Event::Space(flag("down")),
        "proximity" => Event::Proximity { near: flag("near"), eraser: flag("eraser") },
        "copy" => Event::Edit(Edit::Copy),
        "cut" => Event::Edit(Edit::Cut),
        "paste" => Event::Edit(Edit::Paste(Clip::new(Some(text(ask, "text")?.into()), None))),
        "drop" => Event::Dropped(at(ask)?, vec![PathBuf::from(text(ask, "path")?)]),
        "command" => {
            let name = text(ask, "name")?;
            Event::Command(Command::named(name).ok_or_else(|| format!("no command {name:?}"))?)
        }
        _ => return Err(format!("no op {op:?}")),
    })
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

/// A press at the first of `points` (`[[x, y], ...]`), a drag to each
/// other, and a lift at the last.
fn stroke(points: &Value, pen: impl Fn(Point, f64) -> Pen) -> Result<Ask, String> {
    let point = |p: &Value| Some(Point { x: p[0].as_f64()?, y: p[1].as_f64()? });
    let points: Option<Vec<Point>> = points.as_array().into_iter().flatten().map(point).collect();
    let points =
        points.filter(|p| !p.is_empty()).ok_or("stroke needs \"points\": [[x, y], ...]")?;
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
                Ok(said) => json!({ "ok": true, "said": said }),
                Err(error) => json!({ "ok": false, "error": error }),
            };
            // A client that went away gets no reply.
            let _ = writeln!(&stream, "{reply}");
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
