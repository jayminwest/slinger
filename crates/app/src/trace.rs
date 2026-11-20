//! A workspace switch's timing line (`SLINGER_TRACE=switch`, slinger-9fd400):
//! where the time between a pick of another workspace and the first scene
//! that shows all of it goes. Pure: the Watch and the Follower stamp their
//! phases with [`Instant`]s, the main thread hands them in, and `main.rs`
//! writes the line this module makes to stderr.
//!
//! The phases, each a time since the pick: `watch` (the Watch read the
//! wanted name), `layout` (the Watch sent the layout), `taken` (the main
//! thread took it), `shown` (a Follower got the page of a pane of the new
//! layout; the last one), `heard` (the main thread took that page) and
//! `drawn` (the scene with every canvas pane's page was handed to the
//! platform). A phase that happened before the pick (a page already
//! followed) is left out. A layout with no canvas pane ends at `taken`.

use std::fmt::Write as _;
use std::time::Instant;

/// When the Watch read the wanted workspace's name, and when it sent the
/// layout it got for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamps {
    pub watch: Instant,
    pub layout: Instant,
}

/// When a canvas pane's page came: the Follower got it (`shown`), the main
/// thread took it (`heard`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Paged {
    pub shown: Instant,
    pub heard: Instant,
}

/// The switch being timed, if the trace is on and one is.
#[derive(Debug, Default)]
pub struct Trace {
    on: bool,
    switch: Option<Switch>,
}

/// One pick, and its layout once the main thread took it.
#[derive(Debug)]
struct Switch {
    name: Option<String>,
    picked: Instant,
    /// The layout's stamps, and when the main thread took it.
    taken: Option<(Stamps, Instant)>,
}

impl Trace {
    /// On when `flag`, the value of `SLINGER_TRACE`, contains `switch`.
    pub fn new(flag: Option<&str>) -> Self {
        Self { on: flag.is_some_and(|f| f.contains("switch")), switch: None }
    }

    /// The window picked the workspace `name` (`None`: the first) at `at`.
    /// It replaces a switch whose line is not out yet.
    pub fn pick(&mut self, name: Option<String>, at: Instant) {
        if self.on {
            self.switch = Some(Switch { name, picked: at, taken: None });
        }
    }

    /// The main thread took a layout stamped `stamps` at `at`: of the
    /// workspace `name` with a canvas pane or not (`None`: the Watch's
    /// problem). A layout the Watch asked for before the pick is not the
    /// pick's. The line, when the layout has no canvas pane.
    pub fn taken(
        &mut self,
        stamps: Stamps,
        layout: Option<(&str, bool)>,
        at: Instant,
    ) -> Option<String> {
        let switch = self.switch.as_mut().filter(|s| s.taken.is_none())?;
        if stamps.watch < switch.picked {
            return None;
        }
        switch.taken = Some((stamps, at));
        if let Some((name, _)) = layout {
            switch.name = Some(name.to_string());
        }
        match layout {
            Some((_, true)) => None,
            _ => self.switch.take().map(|s| s.line(None)),
        }
    }

    /// A scene was handed to the platform at `at`, with each canvas pane's
    /// page as it came, or `None` while one has none yet. The line, once
    /// every canvas pane of the switch's layout has its page.
    pub fn drawn(&mut self, pages: Option<Vec<Paged>>, at: Instant) -> Option<String> {
        self.switch.as_ref()?.taken?;
        let pages = pages?;
        let last = |of: fn(&Paged) -> Instant| pages.iter().map(of).max();
        let (shown, heard) = (last(|p| p.shown), last(|p| p.heard));
        self.switch.take().map(|s| s.line(Some((shown, heard, at))))
    }
}

impl Switch {
    /// The line: the workspace's name and each phase since the pick, with
    /// the pages' and the scene's when it ends at `drawn`.
    fn line(&self, drawn: Option<(Option<Instant>, Option<Instant>, Instant)>) -> String {
        let name = self.name.as_deref().unwrap_or("-");
        let mut phases: Vec<(&str, Option<Instant>)> = Vec::new();
        if let Some((stamps, taken)) = self.taken {
            phases.extend([
                ("watch", Some(stamps.watch)),
                ("layout", Some(stamps.layout)),
                ("taken", Some(taken)),
            ]);
        }
        if let Some((shown, heard, at)) = drawn {
            phases.extend([("shown", shown), ("heard", heard), ("drawn", Some(at))]);
        }
        let mut line = format!("switch {name}:");
        let since = phases.into_iter().filter_map(|(phase, at)| Some((phase, at?)));
        for (i, (phase, at)) in since.filter(|(_, at)| *at >= self.picked).enumerate() {
            let ms = at.duration_since(self.picked).as_millis();
            let comma = if i == 0 { "" } else { "," };
            // Writing to a String cannot fail.
            let _ = write!(line, "{comma} {phase} +{ms}ms");
        }
        line
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    /// `ms` milliseconds after `start`.
    fn at(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }

    fn stamps(start: Instant, watch: u64, layout: u64) -> Stamps {
        Stamps { watch: at(start, watch), layout: at(start, layout) }
    }

    fn paged(start: Instant, shown: u64, heard: u64) -> Paged {
        Paged { shown: at(start, shown), heard: at(start, heard) }
    }

    #[test]
    fn a_switch_prints_one_line_once_every_canvas_pane_has_its_page() {
        let t = Instant::now();
        let mut trace = Trace::new(Some("switch"));
        trace.pick(Some("notes".into()), t);
        assert_eq!(trace.drawn(Some(vec![]), at(t, 5)), None, "no layout yet");
        assert_eq!(trace.taken(stamps(t, 612, 640), Some(("notes", true)), at(t, 1000)), None);
        assert_eq!(trace.drawn(None, at(t, 1001)), None, "a pane has no page yet");
        let pages = vec![paged(t, 900, 1100), paged(t, 1004, 1500)];
        let line = trace.drawn(Some(pages), at(t, 1512)).unwrap();
        assert_eq!(
            line,
            "switch notes: watch +612ms, layout +640ms, taken +1000ms, \
             shown +1004ms, heard +1500ms, drawn +1512ms"
        );
        assert_eq!(trace.drawn(Some(vec![]), at(t, 1600)), None, "one line per switch");
    }

    #[test]
    fn a_second_pick_replaces_the_first() {
        let t = Instant::now();
        let mut trace = Trace::new(Some("pen,switch"));
        trace.pick(Some("notes".into()), t);
        trace.pick(Some("plans".into()), at(t, 100));
        let stale = stamps(t, 50, 60);
        assert_eq!(trace.taken(stale, Some(("notes", true)), at(t, 120)), None, "before the pick");
        trace.taken(stamps(t, 150, 160), Some(("plans", true)), at(t, 500));
        let line = trace.drawn(Some(vec![paged(t, 20, 40)]), at(t, 510)).unwrap();
        assert_eq!(line, "switch plans: watch +50ms, layout +60ms, taken +400ms, drawn +410ms");
    }

    #[test]
    fn nothing_without_the_flag() {
        for flag in [None, Some(""), Some("pen")] {
            let t = Instant::now();
            let mut trace = Trace::new(flag);
            trace.pick(Some("notes".into()), t);
            assert_eq!(trace.taken(stamps(t, 1, 2), Some(("notes", false)), at(t, 3)), None);
            assert_eq!(trace.drawn(Some(vec![]), at(t, 4)), None);
        }
    }

    #[test]
    fn a_layout_with_no_canvas_pane_or_a_problem_ends_at_taken() {
        let t = Instant::now();
        let mut trace = Trace::new(Some("switch"));
        trace.pick(None, t);
        let line = trace.taken(stamps(t, 1, 2), Some(("agents", false)), at(t, 3));
        assert_eq!(line.unwrap(), "switch agents: watch +1ms, layout +2ms, taken +3ms");
        trace.pick(Some("gone".into()), t);
        let line = trace.taken(stamps(t, 1, 2), None, at(t, 3));
        assert_eq!(line.unwrap(), "switch gone: watch +1ms, layout +2ms, taken +3ms");
        trace.pick(None, t);
        let line = trace.taken(stamps(t, 1, 2), None, at(t, 3));
        assert_eq!(line.unwrap(), "switch -: watch +1ms, layout +2ms, taken +3ms");
    }
}
