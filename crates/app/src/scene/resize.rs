//! Resizing panes with the pen (SPEC section 11): a press on the divider
//! between two panes grabs it, each drag moves it and the panes beside it
//! resize at once, and the lift lets go and says what to tell the server.
//! Pure: where a divider is and the share a point gives its split are
//! `layout`'s ([`crate::layout::Divider`]).

use crate::layout::{Divider, Point, Rect};
use crate::panes::Panes;

/// The divider under the pen, if any.
#[derive(Debug, Default)]
pub struct Dividers {
    held: Option<Divider>,
}

impl Dividers {
    /// The pen went down at `at` over `panes` laid in `area`: it grabs the
    /// divider under it. Returns false, and grabs nothing, away from every
    /// divider.
    pub fn down(&mut self, panes: &Panes, area: Rect, at: Point) -> bool {
        self.held = panes.divider_at(area, at);
        self.held.is_some()
    }

    /// Whether the pen holds a divider.
    pub fn held(&self) -> bool {
        self.held.is_some()
    }

    /// The pen moved to `at` while down: the divider it holds follows, and
    /// `panes` resize. Returns false when it holds none.
    pub fn drag(&mut self, panes: &mut Panes, at: Point) -> bool {
        let Some(divider) = self.held else { return false };
        // The panes may have followed the server since the press, and the
        // divider gone with them: then the drag moves nothing.
        panes.set_ratio(divider.before, divider.ratio(at));
        true
    }

    /// The pen lifted at `at`: the divider stays there and lets go. Returns
    /// the divider as the server names it, the number of the pane before
    /// it and its split's share; `None` when the pen held none, or the
    /// divider is no longer there.
    pub fn up(&mut self, panes: &mut Panes, at: Point) -> Option<(u32, f64)> {
        let divider = self.held.take()?;
        let ratio = divider.ratio(at);
        panes.set_ratio(divider.before, ratio).then(|| (panes.number_of(divider.before), ratio))
    }
}

#[cfg(test)]
mod tests {
    use slinger_control::{Pane as Shown, PaneKind, PaneSplit, PaneTree};

    use super::*;

    const AREA: Rect = Rect { x: 0.0, y: 0.0, width: 800.0, height: 600.0 };

    fn at(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    fn leaf(pane: u32) -> PaneTree {
        PaneTree::Pane(Shown { pane, kind: PaneKind::Canvas, target: "main".into() })
    }

    /// `1 | (2 / 3)`, as the server sends it.
    fn three() -> Panes {
        let right = PaneTree::Split {
            split: PaneSplit::Column,
            ratio: 0.5,
            first: leaf(2).into(),
            second: leaf(3).into(),
        };
        let layout = PaneTree::Split {
            split: PaneSplit::Row,
            ratio: 0.5,
            first: leaf(1).into(),
            second: right.into(),
        };
        Panes::follow(&layout, 1)
    }

    fn widths(panes: &Panes) -> Vec<(f64, f64)> {
        panes.place(AREA).iter().map(|(_, _, r)| (r.width, r.height)).collect()
    }

    #[test]
    fn a_dragged_divider_resizes_the_panes_beside_it_and_names_itself_on_the_lift() {
        let (mut panes, mut dividers) = (three(), Dividers::default());
        assert!(dividers.down(&panes, AREA, at(404.0, 100.0)), "within a pen's reach");
        assert!(dividers.held());
        assert!(dividers.drag(&mut panes, at(200.0, 120.0)));
        assert_eq!(widths(&panes), [(200.0, 600.0), (600.0, 300.0), (600.0, 300.0)]);
        assert_eq!(dividers.up(&mut panes, at(240.0, 120.0)), Some((1, 0.3)));
        assert_eq!(widths(&panes)[0], (240.0, 600.0));
        assert!(!dividers.held());

        assert!(dividers.down(&panes, AREA, at(500.0, 297.0)));
        assert_eq!(dividers.up(&mut panes, at(500.0, 150.0)), Some((2, 0.25)), "after pane 2");
        assert_eq!(widths(&panes)[1], (560.0, 150.0));
    }

    #[test]
    fn a_press_away_from_a_divider_holds_nothing() {
        let (mut panes, mut dividers) = (three(), Dividers::default());
        assert!(!dividers.down(&panes, AREA, at(200.0, 300.0)));
        assert!(!dividers.drag(&mut panes, at(100.0, 300.0)));
        assert_eq!(dividers.up(&mut panes, at(100.0, 300.0)), None);
        assert_eq!(widths(&panes), widths(&three()), "nothing moved");
    }

    #[test]
    fn a_divider_gone_with_the_servers_panes_moves_nothing() {
        let (mut panes, mut dividers) = (three(), Dividers::default());
        assert!(dividers.down(&panes, AREA, at(500.0, 300.0)));
        // The server's layout came: one pane now.
        panes = Panes::follow(&leaf(1), 1);
        assert!(dividers.drag(&mut panes, at(500.0, 100.0)), "the drag still holds it");
        assert_eq!(dividers.up(&mut panes, at(500.0, 100.0)), None);
    }
}
