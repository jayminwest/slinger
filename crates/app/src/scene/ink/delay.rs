//! How long ink took from the pen to the screen (SPEC O2): each paint that
//! shows new ink adds its delay from the pen event, and the end of a stroke
//! gives the log line.

use super::Ink;

impl Ink {
    /// A paint showed ink `delay` seconds after its pen event.
    pub fn painted(&mut self, delay: f64) {
        if self.drawing.is_some() && delay.is_finite() {
            self.delays.push(delay);
        }
    }

    /// The pen-to-paint delays of the stroke last drawn as one line, and
    /// then forgets them; `None` without any.
    pub fn delay_log(&mut self) -> Option<String> {
        if self.delays.is_empty() {
            return None;
        }
        let mut ms: Vec<f64> = self.delays.drain(..).map(|s| s * 1000.0).collect();
        ms.sort_by(f64::total_cmp);
        let at = |part: f64| {
            #[expect(clippy::cast_precision_loss, reason = "a stroke has far fewer paints")]
            let last = (ms.len() - 1) as f64;
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a rounded index into the list"
            )]
            ms[(last * part).round() as usize]
        };
        let (median, high, most) = (at(0.5), at(0.95), at(1.0));
        Some(format!(
            "ink: pen to paint over {} paints, ms: median {median:.1}, 95th {high:.1}, most {most:.1}",
            ms.len()
        ))
    }
}
