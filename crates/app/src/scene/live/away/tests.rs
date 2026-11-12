use std::sync::Arc;
use std::time::{Duration, Instant};

use super::*;
use crate::layout::Rect;
use crate::scene::canvas::ItemId;
use crate::ui::{Code, Page, Shown};

fn page(open: &str) -> Arc<Page> {
    let hash = format!("sha256:{}", "a".repeat(64));
    let code = Code { name: "pick-slot".into(), version: 1, hash, html: Vec::new() };
    let shown = Shown { code, open: Some(open.into()), data: serde_json::Value::Null };
    Arc::new(Page::new(shown, "canvas").unwrap())
}

/// Item `item`'s page, on the screen at `x`.
fn web(item: u64, x: f64) -> Web {
    let frame = Rect { x, y: 0.0, width: 320.0, height: 240.0 };
    let clip = Rect { x: 0.0, y: 0.0, width: 800.0, height: 600.0 };
    let live = Live { pane: None, item: ItemId(item), clip, zoom: 1.0, shows: Shows::Page };
    Web { frame, page: page("pick-slot#12"), live: Some(live) }
}

/// What `away` keeps with `on` on the screen at `now`: each web's item and
/// how it shows.
fn kept(away: &mut Away, on: &[Web], now: Instant) -> Vec<(u64, Shows)> {
    let mut webs = on.to_vec();
    away.keep(&mut webs, now);
    webs.iter().filter_map(|w| w.live.map(|l| (l.item.0, l.shows))).collect()
}

#[test]
fn a_live_item_off_the_screen_keeps_its_view_hidden_for_30_s_then_it_closes() {
    let (mut away, start) = (Away::default(), Instant::now());
    let at = |s: u64| start + Duration::from_secs(s);
    assert_eq!(
        kept(&mut away, &[web(2, 0.0), web(3, 400.0)], at(0)),
        [(2, Shows::Page), (3, Shows::Page)]
    );
    // Item 3 is panned off the screen at 5 s.
    assert_eq!(kept(&mut away, &[web(2, 0.0)], at(5)), [(2, Shows::Page), (3, Shows::Away)]);
    assert_eq!(kept(&mut away, &[web(2, 0.0)], at(34)), [(2, Shows::Page), (3, Shows::Away)]);
    assert!(!away.due(at(34)));
    assert!(away.due(at(35)), "30 s after it went, the window draws again");
    assert_eq!(kept(&mut away, &[web(2, 0.0)], at(35)), [(2, Shows::Page)], "and its view closes");
    assert!(!away.due(at(100)));
}

#[test]
fn an_item_back_on_the_screen_shows_its_page_and_its_30_s_start_again_when_it_next_goes() {
    let (mut away, start) = (Away::default(), Instant::now());
    let at = |s: u64| start + Duration::from_secs(s);
    kept(&mut away, &[web(2, 0.0)], at(0));
    assert_eq!(kept(&mut away, &[], at(20)), [(2, Shows::Away)]);
    let back = kept(&mut away, &[web(2, 50.0)], at(25));
    assert_eq!(back, [(2, Shows::Page)], "once, in its place now");
    assert_eq!(kept(&mut away, &[], at(40)), [(2, Shows::Away)]);
    assert_eq!(kept(&mut away, &[], at(69)), [(2, Shows::Away)], "30 s from when it went again");
    assert_eq!(kept(&mut away, &[], at(70)), []);
}

#[test]
fn a_kept_view_is_where_its_item_last_showed() {
    let (mut away, now) = (Away::default(), Instant::now());
    let mut webs = vec![web(2, 120.0)];
    away.keep(&mut webs, now);
    let mut gone = Vec::new();
    away.keep(&mut gone, now + Duration::from_secs(1));
    assert_eq!(gone.len(), 1);
    assert_eq!(gone[0].frame, webs[0].frame);
    assert_eq!(gone[0].live.map(|l| l.shows), Some(Shows::Away));
}
