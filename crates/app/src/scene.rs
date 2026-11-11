//! What the window shows. Pure: every decision about content is made here, so
//! `platform` only draws what a [`Scene`] says. [`canvas`] decides what a
//! canvas pane shows. [`tools`] holds the pen's tool and colour and the
//! pop-up at the pen that changes them.
//!
//! No chrome (SPEC P6): the panes, each with what it shows, and one focus
//! border around the focused pane. A problem (no daemon, a bad config file)
//! shows in the focused pane, under its name; when all is well the window
//! says nothing more.

#[cfg_attr(not(test), expect(dead_code, reason = "the canvas pane draws it from slinger-c94c"))]
pub mod canvas;
pub mod tools;

use crate::backend::Problem;
use crate::config::{Rgb, Theme};
use crate::layout::{self, Rect, Size};
use crate::panes::{Kind, Pane, Panes};

/// One line of text at a fixed place, on `fill` when it has one. A label
/// with no text and a fill is a filled rectangle.
#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub text: String,
    pub colour: Rgb,
    pub fill: Option<Rgb>,
    pub frame: Rect,
}

/// A line drawn just inside the edge of `frame`, `width` points wide.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Border {
    pub frame: Rect,
    pub width: f64,
    pub colour: Rgb,
}

/// One window, fully decided.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub title: String,
    pub size: Size,
    pub background: Rgb,
    pub labels: Vec<Label>,
    /// Around the focused pane: the window's one border.
    pub focus: Border,
}

/// The focus border's width, in points.
pub const BORDER: f64 = 2.0;

/// The window for `panes` in `theme`'s colours. Until a pane draws its
/// content, it shows its kind and name. `problems` show in the focused
/// pane, each with its hint under it: the window keeps working on the last
/// good config and says what is wrong.
pub fn build(panes: &Panes, theme: &Theme, problems: &[&Problem]) -> Scene {
    let size = layout::WINDOW;
    let mut labels = Vec::new();
    let mut focus = Border { frame: size.rect(), width: BORDER, colour: theme.focus };
    for (id, pane, frame) in panes.place(size.rect()) {
        let mut texts = vec![(placeholder(pane), theme.text)];
        if id == panes.focus() {
            focus.frame = frame;
            for p in problems {
                texts.push((format!("✗ {}", p.message), theme.error));
                texts.extend(p.hint.clone().map(|hint| (hint, theme.muted)));
            }
        }
        let lines = layout::lines(frame, texts.len());
        labels.extend(texts.into_iter().zip(lines).map(|((text, colour), frame)| Label {
            text,
            colour,
            fill: None,
            frame,
        }));
    }
    Scene { title: "slinger".to_string(), size, background: theme.background, labels, focus }
}

/// What a pane shows until it draws its content.
fn placeholder(pane: &Pane) -> String {
    let kind = match pane.kind {
        Kind::Canvas => "canvas",
        Kind::Ui => "agent UI",
    };
    format!("{kind} {}", pane.name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Axis;

    const T: Theme = Theme {
        background: Rgb { r: 0, g: 0, b: 0 },
        text: Rgb { r: 1, g: 1, b: 1 },
        muted: Rgb { r: 2, g: 2, b: 2 },
        error: Rgb { r: 3, g: 3, b: 3 },
        focus: Rgb { r: 4, g: 4, b: 4 },
    };

    fn shown(scene: &Scene) -> Vec<(&str, Rgb)> {
        scene.labels.iter().map(|l| (l.text.as_str(), l.colour)).collect()
    }

    fn main() -> Panes {
        Panes::new(Pane { kind: Kind::Canvas, name: "main".into() })
    }

    #[test]
    fn one_pane_shows_its_kind_and_name_inside_the_focus_border() {
        let scene = build(&main(), &T, &[]);
        assert_eq!(scene.title, "slinger");
        assert_eq!(scene.background, T.background);
        assert_eq!(shown(&scene), [("canvas main", T.text)]);
        assert_eq!(
            scene.focus,
            Border { frame: layout::WINDOW.rect(), width: BORDER, colour: T.focus }
        );
    }

    #[test]
    fn after_a_split_two_panes_show_and_one_border_is_around_the_new_one() {
        let mut panes = main();
        panes.split(Axis::Row);
        let scene = build(&panes, &T, &[]);
        assert_eq!(shown(&scene), [("canvas main", T.text), ("canvas main", T.text)]);
        let half = layout::WINDOW.width / 2.0;
        assert!(scene.labels[0].frame.x < half && scene.labels[1].frame.x > half);
        assert_eq!(
            scene.focus.frame,
            Rect { x: half, y: 0.0, width: half, height: layout::WINDOW.height }
        );
    }

    #[test]
    fn an_agent_ui_pane_says_so() {
        let panes = Panes::decode(1, "ui:board").unwrap();
        assert_eq!(shown(&build(&panes, &T, &[])), [("agent UI board", T.text)]);
    }

    #[test]
    fn problems_show_with_their_hints_in_the_focused_pane() {
        let mut panes = main();
        panes.split(Axis::Column);
        let daemon = Problem { message: "no daemon".into(), hint: Some("start it".into()) };
        let config = Problem { message: "config.toml: line 2: x".into(), hint: None };
        let scene = build(&panes, &T, &[&daemon, &config]);
        assert_eq!(
            shown(&scene),
            [
                ("canvas main", T.text),
                ("canvas main", T.text),
                ("✗ no daemon", T.error),
                ("start it", T.muted),
                ("✗ config.toml: line 2: x", T.error),
            ]
        );
        let focused = scene.focus.frame;
        assert!(scene.labels[1..].iter().all(|l| l.frame.y >= focused.y));
        assert!(scene.labels[1].frame.y < scene.labels[2].frame.y);
    }
}
