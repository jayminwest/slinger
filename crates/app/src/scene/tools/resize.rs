//! What a size command does to a text that is already there
//! (slinger-c29905): the text being typed, else the selected texts, take
//! it; with neither, the command sets a new text's size ([`super::Tools`]).
//! Each text steps from its own size, so texts of different sizes stay
//! apart; a size picked from the pop-up makes them all that size.

use super::SIZES;

/// A size command, as it acts on one text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Resize {
    /// `size-up`: the next of [`SIZES`] above the text's own size.
    Up,
    /// `size-down`: the next of [`SIZES`] below it.
    Down,
    /// A size picked from the pop-up.
    To(u16),
}

impl Resize {
    /// The size a text of `size` takes: its own when no size of [`SIZES`]
    /// lies that way (a size-up at 72, or of an agent's 100-point text).
    pub fn of(self, size: f64) -> f64 {
        let mut sizes = SIZES.iter().map(|&s| f64::from(s));
        match self {
            Resize::Up => sizes.find(|&s| s > size).unwrap_or(size),
            Resize::Down => sizes.rfind(|&s| s < size).unwrap_or(size),
            Resize::To(to) => f64::from(to),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Resize;

    #[test]
    fn a_text_steps_from_its_own_size_along_the_list_and_stays_at_either_end() {
        assert_eq!(Resize::Up.of(24.0), 36.0);
        assert_eq!(Resize::Up.of(30.0), 36.0, "an agent's size steps to the next on the list");
        assert_eq!(Resize::Down.of(30.0), 24.0);
        assert_eq!(Resize::Up.of(72.0), 72.0);
        assert_eq!(Resize::Down.of(12.0), 12.0);
        assert_eq!(Resize::Up.of(100.0), 100.0, "past the list, a size-up leaves it");
        assert_eq!(Resize::Down.of(100.0), 72.0);
        assert_eq!(Resize::To(48).of(12.0), 48.0);
    }
}
