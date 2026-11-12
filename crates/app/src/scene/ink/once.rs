//! The key each change is applied once under (slinger-1d05ae): the daemon
//! applies a `canvas_apply` sent again under the same key only once
//! (docs/service.md, "Applied once"), so a change whose answer was lost, or
//! a stroke kept on disk that was on its way when the app quit, can be sent
//! again safely. A change's key is this run's name and its own key; a
//! stroke kept from an earlier run keeps the key it had then.

use super::{Change, Ink};

impl Ink {
    /// No ink yet, its changes applied once under keys that start with
    /// `run`, a name no other run of the app shares.
    pub fn named(run: impl Into<String>) -> Self {
        Self { run: run.into(), ..Self::default() }
    }

    /// The key the change under `key` is applied once under.
    pub fn once(&self, key: u64) -> String {
        let kept = self.pending.iter().find(|p| p.key == key).and_then(|p| p.once.clone());
        kept.unwrap_or_else(|| format!("{}.{key}", self.run))
    }

    /// As [`Ink::push_keyed`], for a change an earlier run made and kept:
    /// applied once under `once`, its key then, if it had one.
    pub fn resume(&mut self, change: Change, once: Option<String>) -> u64 {
        let key = self.push_keyed(change);
        if let Some(last) = self.pending.last_mut() {
            last.once = once;
        }
        key
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Rgb;
    use crate::layout::Point;
    use crate::scene::ink::Stroke;

    #[test]
    fn a_change_is_applied_once_under_the_run_s_name_or_the_key_it_was_kept_with() {
        let mut ink = Ink::named("123-456");
        let stroke = Stroke {
            canvas: "main".into(),
            page: "p1".into(),
            colour: Rgb { r: 0, g: 0, b: 0 },
            width: 2.0,
            points: vec![Point { x: 0.0, y: 0.0 }],
        };
        let kept = ink.resume(Change::Add(stroke.clone()), Some("9-9.4".into()));
        let old = ink.resume(Change::Add(stroke.clone()), None);
        let new = ink.push_keyed(Change::Add(stroke));
        assert_eq!(ink.once(kept), "9-9.4");
        assert_eq!(ink.once(old), format!("123-456.{old}"));
        assert_eq!(ink.once(new), format!("123-456.{new}"));
        assert_ne!(ink.once(old), ink.once(new));
    }
}
