//! The canvas's paper (`[canvas]`): plain, or a grid of dots, and how far
//! apart the dots are. Pure, like the rest of `config`. The grid is display
//! only: `scene::grid` draws it.

/// What the canvas shows under its items when the app starts
/// (`[canvas] background`). The `toggle-grid` command flips it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Background {
    #[default]
    Plain,
    Dots,
}

impl Background {
    /// Parses `"plain"` or `"dots"`.
    pub(super) fn parse(value: &str) -> Option<Background> {
        match value {
            "plain" => Some(Background::Plain),
            "dots" => Some(Background::Dots),
            _ => None,
        }
    }
}

/// The canvas points between two dots (`[canvas] grid`), held in hundredths
/// so it parses by hand: `str::parse` costs the app binary its size budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grid(u32);

impl Grid {
    /// The closest dots may be, in hundredths: 1 point.
    const LEAST: u32 = 100;
    /// The furthest, in hundredths: 500 points.
    const MOST: u32 = 50_000;

    /// The distance in canvas points.
    pub fn points(self) -> f64 {
        f64::from(self.0) / 100.0
    }

    /// Parses `"28.35"` or `"20"`: digits, then at most two decimal places.
    /// From 1 to 500.
    pub(super) fn parse(value: &str) -> Option<Grid> {
        let (whole, part) = value.split_once('.').unwrap_or((value, "0"));
        if whole.is_empty() || whole.len() > 3 || part.is_empty() || part.len() > 2 {
            return None;
        }
        let digits = |text: &str| {
            text.bytes()
                .try_fold(0_u32, |n, c| c.is_ascii_digit().then(|| n * 10 + u32::from(c - b'0')))
        };
        let scale = if part.len() == 1 { 10 } else { 1 };
        let hundredths = digits(whole)? * 100 + digits(part)? * scale;
        (Self::LEAST..=Self::MOST).contains(&hundredths).then_some(Grid(hundredths))
    }
}

impl Default for Grid {
    /// One centimetre at 100% on a screen that shows 72 points per inch:
    /// 72 / 2.54 = 28.35 points.
    fn default() -> Self {
        Grid(2835)
    }
}

/// `[canvas]`, resolved.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Canvas {
    pub background: Background,
    pub grid: Grid,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ErrorKind, parse};

    fn canvas(text: &str) -> Result<Canvas, (usize, ErrorKind)> {
        parse(text).map(|l| l.resolve().canvas).map_err(|e| (e.line, e.kind))
    }

    #[test]
    fn the_paper_is_plain_with_one_centimetre_dots_by_default() {
        let d = canvas("").unwrap();
        assert_eq!(d.background, Background::Plain);
        assert!((d.grid.points() - 28.35).abs() < 1e-9);
    }

    #[test]
    fn parses_the_background_and_the_grid() {
        let c = canvas("[canvas]\nbackground = \"dots\"\ngrid = \"20\"\n").unwrap();
        assert_eq!(c.background, Background::Dots);
        assert!((c.grid.points() - 20.0).abs() < 1e-9);
        for (text, points) in [("1", 1.0), ("2.5", 2.5), ("28.35", 28.35), ("500", 500.0)] {
            assert!((Grid::parse(text).unwrap().points() - points).abs() < 1e-9, "{text}");
        }
        assert_eq!(
            canvas("[canvas]\nbackground = \"plain\"\n").unwrap().background,
            Background::Plain
        );
    }

    #[test]
    fn a_bad_background_or_grid_is_an_error_at_its_line() {
        assert_eq!(
            canvas("[canvas]\n\nbackground = \"grid\"\n"),
            Err((3, ErrorKind::BadBackground))
        );
        for value in
            ["", "0", "0.99", "500.01", "1000", "-5", "+5", "1.234", "5.", ".5", "1e1", "1cm", "a"]
        {
            let text = format!("[canvas]\ngrid = \"{value}\"\n");
            assert_eq!(canvas(&text), Err((2, ErrorKind::BadGrid)), "{value:?}");
        }
    }

    #[test]
    fn the_overlay_wins_key_by_key() {
        let base = parse("[canvas]\nbackground = \"dots\"\ngrid = \"10\"\n").unwrap();
        let top = parse("[canvas]\ngrid = \"20\"\n").unwrap();
        let c = base.over(top).resolve().canvas;
        assert_eq!(c.background, Background::Dots);
        assert_eq!(c.grid, Grid(2000));
    }
}
