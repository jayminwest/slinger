use super::*;

const PANE: Rect = Rect { x: 100.0, y: 0.0, width: 400.0, height: 300.0 };

const T: Theme = Theme {
    background: Rgb { r: 0x1e, g: 0x1e, b: 0x1e },
    text: Rgb { r: 1, g: 1, b: 1 },
    muted: Rgb { r: 2, g: 2, b: 2 },
    error: Rgb { r: 3, g: 3, b: 3 },
    focus: Rgb { r: 4, g: 4, b: 4 },
};

fn text(flash: &Flash, now: Instant) -> Option<String> {
    flash.pill(PANE, &T, now).map(|p| p.label.text)
}

#[test]
fn the_first_tool_does_not_flash_and_a_new_one_does_with_its_key() {
    let (keys, now) = (Keymap::default(), Instant::now());
    let flash = Flash::new(Tool::Pen);
    flash.follow(Tool::Pen, &keys, now);
    assert_eq!(text(&flash, now), None);
    flash.follow(Tool::Eraser, &keys, now);
    assert_eq!(text(&flash, now).as_deref(), Some("Eraser ⌘2"));
    flash.follow(Tool::Arrow, &keys, now);
    assert_eq!(text(&flash, now).as_deref(), Some("Arrow ⌘6"), "the new flash takes its place");
}

#[test]
fn a_command_with_no_key_flashes_its_title_alone() {
    let mut keys = Keymap::default();
    keys.bind(Command::ToolLasso, None);
    let now = Instant::now();
    let flash = Flash::new(Tool::Pen);
    flash.follow(Tool::Lasso, &keys, now);
    assert_eq!(text(&flash, now).as_deref(), Some("Lasso"));
    flash.command(Command::ReloadConfig, &keys, now);
    assert_eq!(text(&flash, now).as_deref(), Some("Reload config ⌘R"));
}

#[test]
fn it_shows_until_its_time_and_then_fades_once() {
    let (keys, now) = (Keymap::default(), Instant::now());
    let flash = Flash::new(Tool::Pen);
    assert!(!flash.fade(now), "nothing showing, nothing to fade");
    flash.follow(Tool::Text, &keys, now);
    let pill = flash.pill(PANE, &T, now).unwrap();
    assert_eq!(pill.until, now + SHOWN);
    let almost = now + SHOWN.saturating_sub(Duration::from_millis(1));
    assert!(!flash.fade(almost));
    assert!(flash.pill(PANE, &T, almost).is_some());
    assert_eq!(text(&flash, now + SHOWN), None, "gone at its time, even before it fades");
    assert!(flash.fade(now + SHOWN));
    assert!(!flash.fade(now + SHOWN), "it fades once");
    assert_eq!(text(&flash, now), None);
}

#[test]
fn the_pill_is_at_the_top_centre_of_the_pane_a_step_lighter() {
    let (keys, now) = (Keymap::default(), Instant::now());
    let flash = Flash::new(Tool::Pen);
    flash.follow(Tool::Box, &keys, now);
    let label = flash.pill(PANE, &T, now).unwrap().label;
    let f = label.frame;
    assert_eq!(f.x + f.width / 2.0, PANE.x + PANE.width / 2.0);
    assert_eq!((f.y, f.height), (GAP, HEIGHT));
    assert!(f.width > PAD * 2.0 && f.width < PANE.width);
    assert_eq!(label.colour, T.text);
    assert_eq!(label.fill, Some(Rgb { r: 0x38, g: 0x38, b: 0x38 }));
    assert_eq!(lighter(Rgb { r: 0xff, g: 0xf0, b: 0 }), Rgb { r: 0xff, g: 0xff, b: STEP });

    let narrow = Rect { x: 0.0, y: 50.0, width: 40.0, height: 20.0 };
    let f = flash.pill(narrow, &T, now).unwrap().label.frame;
    assert_eq!((f.x, f.y, f.width), (0.0, 50.0, 40.0), "inside a pane too small for it");
}
