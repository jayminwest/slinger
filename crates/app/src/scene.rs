//! What the window shows. Pure: every decision about content is made here, so
//! `platform` only draws what a [`Scene`] says. [`canvas`] decides what a
//! canvas pane shows. [`tools`] holds the pen's tool and colour and the
//! pop-up at the pen that changes them. [`sidebar`] holds the agents'
//! statuses, the optional sidebar that shows them and the notice when one
//! becomes blocked or done.
//!
//! No chrome (SPEC P6): the panes, each with what it shows, one focus
//! border around the focused pane, and the sidebar when it is open. A
//! canvas pane whose page has arrived draws it; any other pane shows its
//! kind and name. A problem (no daemon, a bad config file) shows in the
//! focused pane, over what it shows; when all is well the window says
//! nothing more, except the one notice that an agent is blocked or done.

pub mod canvas;
pub mod sidebar;
pub mod tools;

use std::rc::Rc;

use crate::backend::Problem;
use crate::config::{Rgb, Theme};
use crate::layout::{self, PaneId, Rect, Size};
use crate::panes::{Kind, Pane, Panes};
use crate::scene::sidebar::Sidebar;

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

/// A pane that draws a canvas: which pane, where in the window, and what it
/// shows, in the pane's own points. The pane's last scene is shared, not
/// copied, with what decides the next one.
#[derive(Debug, Clone, PartialEq)]
pub struct CanvasPane {
    pub pane: PaneId,
    pub frame: Rect,
    pub scene: Rc<canvas::Scene>,
}

/// One window, fully decided.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub title: String,
    pub size: Size,
    pub background: Rgb,
    /// Under the labels and the border.
    pub canvases: Vec<CanvasPane>,
    pub labels: Vec<Label>,
    /// The sidebar's lines: labels read from their left edge, not centred.
    pub sidebar: Vec<Label>,
    /// Around the focused pane: the window's one border.
    pub focus: Border,
}

/// The focus border's width, in points.
pub const BORDER: f64 = 2.0;

/// The window for `panes` in `theme`'s colours, beside `sidebar`. The panes
/// in `canvases` draw their canvas; every other pane shows its kind and
/// name. `problems` show in the focused pane, each with its hint under it:
/// the window keeps working on the last good config and says what is wrong.
/// The notice, if any, is the last label, over the panes.
pub fn build(
    panes: &Panes,
    theme: &Theme,
    problems: &[&Problem],
    sidebar: &Sidebar,
    canvases: Vec<CanvasPane>,
) -> Scene {
    let size = layout::WINDOW;
    let area = sidebar.panes(size.rect());
    let mut labels = Vec::new();
    let mut focus = Border { frame: area, width: BORDER, colour: theme.focus };
    for (id, pane, frame) in panes.place(area) {
        let drawn = canvases.iter().any(|c| c.pane == id);
        let mut texts: Vec<_> =
            (!drawn).then(|| (placeholder(pane), theme.text)).into_iter().collect();
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
    labels.extend(sidebar.notice(theme, area));
    Scene {
        title: "slinger".to_string(),
        size,
        background: theme.background,
        canvases,
        labels,
        sidebar: sidebar.rows(theme, size.rect()),
        focus,
    }
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
        let scene = build(&main(), &T, &[], &Sidebar::new(false), Vec::new());
        assert_eq!(scene.title, "slinger");
        assert_eq!(scene.sidebar, []);
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
        let scene = build(&panes, &T, &[], &Sidebar::new(false), Vec::new());
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
        let panes = Panes::new(Pane { kind: Kind::Ui, name: "board".into() });
        assert_eq!(
            shown(&build(&panes, &T, &[], &Sidebar::new(false), Vec::new())),
            [("agent UI board", T.text)]
        );
    }

    #[test]
    fn problems_show_with_their_hints_in_the_focused_pane() {
        let mut panes = main();
        panes.split(Axis::Column);
        let daemon = Problem { message: "no daemon".into(), hint: Some("start it".into()) };
        let config = Problem { message: "config.toml: line 2: x".into(), hint: None };
        let scene = build(&panes, &T, &[&daemon, &config], &Sidebar::new(false), Vec::new());
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

    #[test]
    fn an_open_sidebar_takes_the_left_and_the_notice_shows_over_the_panes() {
        use sidebar::{Agent, Status, Workspace};
        let ws = |status| {
            let agent = Agent { name: "ada".into(), status, note: None };
            vec![Workspace { name: "notes".into(), agent: Some(agent) }]
        };
        let mut side = Sidebar::new(true);
        side.update(Ok(ws(Some(Status::Working))));
        side.update(Ok(ws(Some(Status::Blocked))));
        let scene = build(&main(), &T, &[], &side, Vec::new());
        let area = side.panes(layout::WINDOW.rect());
        assert_eq!(scene.focus.frame, area);
        assert!(area.x >= sidebar::WIDTH);
        assert_eq!(scene.sidebar, side.rows(&T, layout::WINDOW.rect()));
        assert_eq!(shown(&scene), [("canvas main", T.text), ("ada is blocked", T.background)]);
    }

    #[test]
    fn a_pane_that_draws_its_canvas_shows_no_name_but_still_its_problems() {
        let mut panes = main();
        panes.split(Axis::Row);
        let drawn = canvas::build(&[], layout::Viewport::default(), layout::WINDOW, &T, None);
        let frame = Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 };
        let canvases = vec![CanvasPane { pane: panes.focus(), frame, scene: Rc::new(drawn) }];
        let daemon = Problem { message: "no daemon".into(), hint: None };
        let scene = build(&panes, &T, &[&daemon], &Sidebar::new(false), canvases.clone());
        assert_eq!(shown(&scene), [("canvas main", T.text), ("✗ no daemon", T.error)]);
        assert_eq!(scene.canvases, canvases);
    }
}
