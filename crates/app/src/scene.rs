//! What the window shows. Pure: every decision about content is made here, so
//! `platform` only draws what a [`Scene`] says. [`canvas`] decides what a
//! canvas pane shows. [`tools`] holds the pen's tool and colour and the
//! pop-up at the pen that changes them. [`ink`] holds the pen's strokes and
//! changes until the server's page has them. [`select`] holds the eraser,
//! the lasso and the selection. [`sidebar`] holds the agents'
//! statuses, the optional sidebar that shows them and the notice when one
//! becomes blocked or done. [`sling`] holds the sling from the window: its
//! one-line entry, and how it goes. [`overlay`] holds the key overlay.
//!
//! No chrome, but every state is visible (SPEC P6): the panes, each with
//! what it shows above its footer ([`status`]), one focus border around the
//! focused pane when there are two or more, and the sidebar when it is
//! open. A
//! canvas pane whose page has arrived draws it; an agent UI pane shows its
//! page ([`Web`]), or its name and why it cannot; any other pane shows its
//! kind and name. A problem (no daemon, a bad config file) shows in the
//! focused pane, over what it shows; when all is well the window says
//! nothing more, except the one notice that an agent is blocked or done.

pub mod button;
pub mod canvas;
pub mod dictation;
pub mod ink;
pub mod live;
pub mod overlay;
pub mod pages;
pub mod resize;
pub mod select;
pub mod shapes;
pub mod sidebar;
pub mod sling;
pub mod status;
pub mod tools;

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use crate::backend::Problem;
use crate::config::{Rgb, Theme};
use crate::layout::{self, PaneId, Rect, Size};
use crate::panes::{Kind, Pane, Panes};
use crate::scene::sidebar::Sidebar;
use crate::ui::Page;

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

/// An agent UI on screen: its page, in `frame`; a live item's page also
/// says where it is ([`live::Live`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Web {
    pub frame: Rect,
    pub page: Arc<Page>,
    pub live: Option<live::Live>,
}

/// The agent UIs fetched so far, by name: each one's page, or why it
/// cannot show.
pub type Uis = HashMap<String, Result<Arc<Page>, Problem>>;

/// What an agent UI pane says over its page, by name: why its last reply
/// did not reach the agent.
pub type Notes = HashMap<String, Problem>;

/// One window, fully decided.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub title: String,
    pub size: Size,
    pub background: Rgb,
    /// Under the labels and the border.
    pub canvases: Vec<CanvasPane>,
    /// The agent UIs, under the labels and the border.
    pub webs: Vec<Web>,
    pub labels: Vec<Label>,
    /// The sidebar's lines: labels read from their left edge, not centred.
    pub sidebar: Vec<sidebar::Row>,
    /// Each pane's footer, under the labels and the border.
    pub footers: Vec<status::Footer>,
    /// Around the focused pane: the window's one border; none, zero wide,
    /// when one pane fills the area.
    pub focus: Border,
    /// The sling's one-line entry, over everything, while it is open.
    pub entry: Option<Box<sling::Entry>>,
    /// The key overlay, over everything but the entry, while it is open.
    pub overlay: Option<Box<overlay::Overlay>>,
}

/// The focus border's width, in points.
pub const BORDER: f64 = 2.0;

/// The window of `size` for `panes` in `theme`'s colours, beside `sidebar`:
/// the panes share what the sidebar leaves of it. The panes
/// in `canvases` draw their canvas. An agent UI pane shows its page from
/// `uis` inside the border's width, with its note from `notes` over it; one
/// that cannot show says its name and why. Every other pane shows its kind
/// and name. `problems` show in the focused pane, each with its hint under it:
/// the window keeps working on the last good config and says what is wrong.
/// The notice, if any, is the last label, over the panes. One pane alone
/// has no focus border: there is no other pane it could be told from.
#[expect(clippy::too_many_arguments, reason = "each is one input the window shows")]
pub fn build(
    size: Size,
    panes: &Panes,
    theme: &Theme,
    problems: &[&Problem],
    sidebar: &Sidebar,
    canvases: Vec<CanvasPane>,
    uis: &Uis,
    notes: &Notes,
) -> Scene {
    let area = sidebar.panes(size.rect());
    let (mut labels, mut webs) = (Vec::new(), Vec::new());
    let placed = panes.place(area);
    let mut focus = match placed.len() {
        1 => Border { frame: area, width: 0.0, colour: theme.background },
        _ => Border { frame: area, width: BORDER, colour: theme.focus },
    };
    for (id, pane, frame) in placed {
        let mut texts = Vec::new();
        match (pane.kind, uis.get(&pane.name)) {
            _ if canvases.iter().any(|c| c.pane == id) => {}
            (Kind::Ui, Some(Ok(page))) => {
                let frame = status::content(frame).outset(-BORDER);
                webs.push(Web { frame, page: Arc::clone(page), live: None });
                if let Some(note) = notes.get(&pane.name) {
                    texts.push((format!("✗ {}", note.message), theme.error));
                    texts.extend(note.hint.clone().map(|hint| (hint, theme.muted)));
                }
            }
            (Kind::Ui, Some(Err(why))) => {
                texts.push((placeholder(pane), theme.text));
                texts.push((format!("✗ {}", why.message), theme.error));
                texts.extend(why.hint.clone().map(|hint| (hint, theme.muted)));
            }
            _ => texts.push((placeholder(pane), theme.text)),
        }
        if id == panes.focus() {
            focus.frame = frame;
            for p in problems {
                texts.push((format!("✗ {}", p.message), theme.error));
                texts.extend(p.hint.clone().map(|hint| (hint, theme.muted)));
            }
        }
        let lines = layout::lines(status::content(frame), texts.len());
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
        webs,
        labels,
        sidebar: sidebar.rows(theme, size.rect()),
        footers: Vec::new(),
        focus,
        entry: None,
        overlay: None,
    }
}

/// The window of `size` for `page` alone (`slinger-app --preview`): the
/// page fills it, with no border; as a live `item`, it shows at the default
/// item's size, zoomed by half again.
pub fn preview(page: Arc<Page>, theme: &Theme, item: bool, size: Size) -> Scene {
    let title = format!("slinger: {}", page.code.name);
    let web = if item {
        live::alone(page, size, 1.5)
    } else {
        Web { frame: size.rect(), page, live: None }
    };
    Scene {
        title,
        size,
        background: theme.background,
        canvases: Vec::new(),
        webs: vec![web],
        labels: Vec::new(),
        sidebar: Vec::new(),
        footers: Vec::new(),
        focus: Border { frame: size.rect(), width: 0.0, colour: theme.background },
        entry: None,
        overlay: None,
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

    fn build(
        panes: &Panes,
        theme: &Theme,
        problems: &[&Problem],
        sidebar: &Sidebar,
        canvases: Vec<CanvasPane>,
    ) -> Scene {
        let (uis, notes) = (Uis::new(), Notes::new());
        super::build(layout::WINDOW, panes, theme, problems, sidebar, canvases, &uis, &notes)
    }

    #[test]
    fn one_pane_shows_its_kind_and_name_with_no_focus_border() {
        let scene = build(&main(), &T, &[], &Sidebar::new(false), Vec::new());
        assert_eq!(scene.title, "slinger");
        assert_eq!(scene.sidebar, []);
        assert_eq!(scene.background, T.background);
        assert_eq!(shown(&scene), [("canvas main", T.text)]);
        assert_eq!(
            scene.focus,
            Border { frame: layout::WINDOW.rect(), width: 0.0, colour: T.background }
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
            scene.focus,
            Border {
                frame: Rect { x: half, y: 0.0, width: half, height: layout::WINDOW.height },
                width: BORDER,
                colour: T.focus,
            }
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
    fn an_agent_ui_shows_its_page_inside_the_border_or_its_name_and_why_not() {
        use crate::ui::Code;
        let mut panes = Panes::new(Pane { kind: Kind::Ui, name: "board".into() });
        panes.split(Axis::Row);
        let hash = format!("sha256:{}", "a".repeat(64));
        let code = Code { name: "board".into(), version: 1, hash, html: Vec::new() };
        let what = crate::ui::Shown { code, open: None, data: serde_json::Value::Null };
        let page = Arc::new(Page::new(what, "pane").unwrap());
        let mut uis = Uis::from([("board".to_string(), Ok(Arc::clone(&page)))]);
        let side = Sidebar::new(false);
        let scene =
            super::build(layout::WINDOW, &panes, &T, &[], &side, Vec::new(), &uis, &Notes::new());
        assert_eq!(shown(&scene), []);
        let half = layout::WINDOW.width / 2.0;
        let inner = Rect { x: half, y: 0.0, width: half, height: layout::WINDOW.height };
        assert_eq!(scene.webs.len(), 2);
        let inner = status::content(inner).outset(-BORDER);
        let web = Web { frame: inner, page: Arc::clone(&page), live: None };
        assert_eq!(scene.webs[1], web);

        let note = Problem { message: "scout is not running".into(), hint: Some("start".into()) };
        let kept = Notes::from([("board".to_string(), note)]);
        let noted = super::build(layout::WINDOW, &panes, &T, &[], &side, Vec::new(), &uis, &kept);
        assert_eq!(noted.webs, scene.webs, "the page stays under its note");
        assert_eq!(shown(&noted)[..2], [("✗ scout is not running", T.error), ("start", T.muted)]);
        assert!(noted.labels[2].frame.x >= half, "each pane says it over its own page");

        let alone = preview(page, &T, false, layout::WINDOW);
        assert_eq!((alone.title.as_str(), alone.labels.len()), ("slinger: board", 0));
        assert_eq!(alone.webs[0].frame, layout::WINDOW.rect());

        let why = Problem { message: "ui \"board\" not found".into(), hint: Some("put it".into()) };
        uis.insert("board".into(), Err(why));
        let scene =
            super::build(layout::WINDOW, &panes, &T, &[], &side, Vec::new(), &uis, &Notes::new());
        assert_eq!(scene.webs, []);
        assert_eq!(
            shown(&scene)[..3],
            [
                ("agent UI board", T.text),
                ("✗ ui \"board\" not found", T.error),
                ("put it", T.muted)
            ]
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
            vec![Workspace { name: "notes".into(), canvas: "notes".into(), agent: Some(agent) }]
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

    #[test]
    fn the_panes_and_the_sidebar_follow_the_window_size() {
        let mut panes = main();
        panes.split(Axis::Row);
        let side = Sidebar::new(true);
        let (uis, notes) = (Uis::new(), Notes::new());
        for size in [layout::SMALLEST, layout::WINDOW, Size { width: 1600.0, height: 1000.0 }] {
            let scene = super::build(size, &panes, &T, &[], &side, Vec::new(), &uis, &notes);
            assert_eq!(scene.size, size);
            let area = side.panes(size.rect());
            let half = area.width / 2.0;
            let right = Rect { x: area.x + half, y: 0.0, width: half, height: size.height };
            assert_eq!(scene.focus.frame, right, "the focused pane fills its share of {size:?}");
            assert_eq!(scene.sidebar, side.rows(&T, size.rect()));
            assert!(scene.labels.iter().all(|l| l.frame.y + l.frame.height <= size.height));
        }
    }
}
