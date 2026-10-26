const DISCRETE_MULTIPLIER: f32 = 3.0;
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Delta {
    Pixels(f32),
    Ticks(f32),
}
#[derive(Debug, Default, Clone, Copy)]
pub struct Accumulator {
    pending: f32,
}
impl Accumulator {
    pub fn feed(&mut self, delta: Delta, cell_height: f32) -> i32 {
        if cell_height <= 0.0 {
            return 0;
        }
        let offset = match delta {
            Delta::Pixels(y) => y,
            Delta::Ticks(0.0) => 0.0,
            Delta::Ticks(y) => y.signum() * y.abs().max(1.0) * cell_height * DISCRETE_MULTIPLIER,
        };
        self.pending += offset;
        let steps = (self.pending / cell_height).trunc();
        self.pending -= steps * cell_height;
        steps as i32
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const CELL: f32 = 15.0;
    fn feed_all(deltas: impl IntoIterator<Item = Delta>) -> i32 {
        let mut acc = Accumulator::default();
        deltas.into_iter().map(|d| acc.feed(d, CELL)).sum()
    }
    #[test]
    fn slow_drag_scrolls() {
        assert_eq!(feed_all([Delta::Pixels(4.0); 10]), 2);
        assert_eq!(feed_all([Delta::Pixels(-4.0); 10]), -2);
    }
    #[test]
    fn slicing_does_not_change_distance() {
        let fine = feed_all([Delta::Pixels(1.5); 100]);
        let coarse = feed_all([Delta::Pixels(50.0); 3]);
        assert_eq!(fine, 10);
        assert_eq!(coarse, 10);
    }
    #[test]
    fn reversal_consumes_remainder() {
        let mut acc = Accumulator::default();
        assert_eq!(acc.feed(Delta::Pixels(10.0), CELL), 0);
        assert_eq!(acc.feed(Delta::Pixels(-10.0), CELL), 0);
        assert_eq!(acc.feed(Delta::Pixels(-15.0), CELL), -1);
    }
    #[test]
    fn wheel_notch_is_three_steps() {
        assert_eq!(feed_all([Delta::Ticks(1.0)]), 3);
        assert_eq!(feed_all([Delta::Ticks(0.1)]), 3);
        assert_eq!(feed_all([Delta::Ticks(-0.1)]), -3);
        assert_eq!(feed_all([Delta::Ticks(2.5)]), 7);
        assert_eq!(feed_all([Delta::Ticks(0.0)]), 0);
    }
}
