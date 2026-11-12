use super::*;

const fn rgb(hex: u32) -> Rgb {
    let [_, r, g, b] = hex.to_be_bytes();
    Rgb { r, g, b }
}

#[test]
fn the_default_theme_gives_the_designs_colours() {
    let theme = Theme::default();
    assert_eq!(sidebar(&theme), rgb(0x21_21_21));
    assert_eq!(footer(&theme), rgb(0x23_23_23));
    assert_eq!(panel(&theme), rgb(0x26_26_26));
    assert_eq!(hover(&theme), rgb(0x2e_2e_2e));
    assert_eq!(chip(&theme), rgb(0x36_36_36));
    assert_eq!(edge(&theme), rgb(0x3d_3d_3d));
    assert_eq!(selected(&theme), rgb(0x2e_4a_6e));
    let label = selected_label(&theme);
    assert!(label.b > label.g && label.g > label.r && label.r > 0xb0, "{label:?}");
}

#[test]
fn a_mix_goes_from_one_colour_to_the_other() {
    let (black, white) = (rgb(0), rgb(0xff_ff_ff));
    assert_eq!(mix(black, white, 0), black);
    assert_eq!(mix(black, white, 255), white);
    assert_eq!(mix(black, white, 999), white);
    assert_eq!(mix(white, black, 128), rgb(0x7f_7f_7f));
}

#[test]
fn a_light_theme_keeps_its_contrasts() {
    let theme = Theme { background: rgb(0xff_ff_ff), text: rgb(0), ..Theme::default() };
    let shades = [sidebar(&theme), footer(&theme), panel(&theme), chip(&theme), edge(&theme)];
    assert!(shades.windows(2).all(|w| w[0].r > w[1].r), "each a step further from the page");
}

#[test]
fn the_sidebar_has_an_edge_and_panes_a_divider_on_each_shared_edge() {
    let theme = Theme::default();
    let window = Rect { x: 0.0, y: 0.0, width: 1000.0, height: 600.0 };
    let side = Rect { width: 200.0, ..window };
    let area = Rect { x: 200.0, width: 800.0, ..window };
    let left = Rect { width: 400.0, ..area };
    let top = Rect { x: 600.0, width: 400.0, height: 300.0, ..area };
    let under = Rect { y: 300.0, ..top };
    let rules = rules(&theme, window, side, area, &[left, top, under]);
    let lines: Vec<Rect> = rules.iter().map(|r| r.frame).collect();
    assert_eq!(rules[0], Rule { frame: side, colour: sidebar(&theme) });
    assert_eq!(lines[1], Rect { x: 199.0, width: 1.0, ..side });
    assert_eq!(lines[2], Rect { height: 1.0, ..window });
    assert!(lines.contains(&Rect { x: 599.5, width: 1.0, ..left }), "between left and right");
    assert!(lines.contains(&Rect { y: 299.5, height: 1.0, ..top }), "between top and under");
    assert_eq!(lines.len(), 5, "no rule along the window's edge");
    assert!(rules[1..].iter().all(|r| r.colour == edge(&theme)));

    let closed = Rect { width: 0.0, ..side };
    let alone = super::rules(&theme, window, closed, window, &[window]);
    assert_eq!(alone, [Rule { frame: Rect { height: 1.0, ..window }, colour: edge(&theme) }]);
}
