//! The pen's widths (`[pen] widths`): the line widths the width pop-up
//! offers. Pure, like the rest of `config`.

/// One line width, in canvas points, held in tenths so it parses and
/// prints by hand: `str::parse` and float formatting cost the app binary
/// its size budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Width(u16);

impl Width {
    /// The widest a width may be, in tenths: 50 points.
    const MOST: u16 = 500;

    /// The width in canvas points.
    pub fn points(self) -> f64 {
        f64::from(self.0) / 10.0
    }

    /// The width as the config file writes it: `"4"`, `"1.5"`.
    pub fn text(self) -> String {
        match self.0 % 10 {
            0 => format!("{}", self.0 / 10),
            tenth => format!("{}.{tenth}", self.0 / 10),
        }
    }

    /// Parses `"4"` or `"1.5"`: digits, then at most one decimal place.
    /// Above 0, at most 50.
    fn parse(word: &str) -> Option<Width> {
        let (whole, tenth) = match word.split_once('.') {
            Some((whole, tenth)) => (whole, tenth),
            None => (word, "0"),
        };
        let digit = |c: u8| c.is_ascii_digit().then(|| u16::from(c - b'0'));
        let tenth = match tenth.as_bytes() {
            &[d] => digit(d)?,
            _ => return None,
        };
        if whole.is_empty() || whole.len() > 2 {
            return None;
        }
        let whole = whole.bytes().try_fold(0, |n, c| Some(n * 10 + digit(c)?))?;
        let tenths = whole * 10 + tenth;
        (1..=Self::MOST).contains(&tenths).then_some(Width(tenths))
    }
}

/// The pen's widths, in the order the width pop-up shows them: one to
/// [`Widths::MAX`], so each has a digit key. The first is the pen's width
/// when the app starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Widths(Vec<Width>);

impl Widths {
    /// The most widths a list holds: the digit keys 1 to 9.
    pub const MAX: usize = 9;

    /// The widths, never none.
    pub fn widths(&self) -> &[Width] {
        &self.0
    }

    /// Parses `"2 4 8 16"`: widths in points split by spaces or commas.
    pub(super) fn parse(value: &str) -> Option<Widths> {
        let widths = value
            .split(|c: char| c == ',' || c.is_ascii_whitespace())
            .filter(|w| !w.is_empty())
            .map(Width::parse)
            .collect::<Option<Vec<_>>>()?;
        (1..=Self::MAX).contains(&widths.len()).then_some(Widths(widths))
    }
}

impl Default for Widths {
    /// Fine, medium, bold and marker: 2, 4, 8 and 16 points. Fine is the
    /// pen's width before widths could be picked.
    fn default() -> Self {
        Widths(vec![Width(20), Width(40), Width(80), Width(160)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(value: &str) -> Option<Vec<String>> {
        Widths::parse(value).map(|w| w.widths().iter().map(|w| w.text()).collect())
    }

    #[test]
    fn parses_whole_and_tenth_points_split_by_spaces_or_commas() {
        assert_eq!(texts("2 4, 8,16").unwrap(), ["2", "4", "8", "16"]);
        assert_eq!(texts(" 0.5 1.5 50 ").unwrap(), ["0.5", "1.5", "50"]);
        assert_eq!(
            texts("02 4.0").unwrap(),
            ["2", "4"],
            "a leading zero and .0 are the same width"
        );
        let w = Widths::parse("1.5").unwrap();
        assert!((w.widths()[0].points() - 1.5).abs() < f64::EPSILON);
    }

    #[test]
    fn rejects_what_is_not_one_to_nine_widths_above_0_and_at_most_50() {
        for value in [
            "",
            " ",
            "0",
            "0.0",
            "50.1",
            "51",
            "100",
            "-2",
            "+2",
            "2.25",
            "2.",
            ".5",
            "1e1",
            "two",
            "2pt",
            "1 2 3 4 5 6 7 8 9 10",
        ] {
            assert_eq!(texts(value), None, "{value:?}");
        }
        assert_eq!(texts("1 2 3 4 5 6 7 8 9").map(|w| w.len()), Some(Widths::MAX));
    }

    #[test]
    fn the_defaults_start_with_the_fine_pen() {
        let d = Widths::default();
        assert_eq!(d, Widths::parse("2 4 8 16").unwrap());
        assert!((d.widths()[0].points() - 2.0).abs() < f64::EPSILON);
    }
}
