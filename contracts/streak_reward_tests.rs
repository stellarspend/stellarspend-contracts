#[cfg(test)]
mod tests {
    #[derive(Debug, Default)]
    struct Streak { count: u32, rewarded: bool }

    impl Streak {
        fn increment(&mut self) { self.count += 1; }
        fn reset(&mut self) { self.count = 0; self.rewarded = false; }
        fn try_reward(&mut self, threshold: u32) -> bool {
            if self.count >= threshold { self.rewarded = true; true } else { false }
        }
    }

    #[test]
    fn test_streak_increment() {
        let mut s = Streak::default();
        s.increment(); s.increment();
        assert_eq!(s.count, 2);
    }

    #[test]
    fn test_streak_reset() {
        let mut s = Streak { count: 5, rewarded: true };
        s.reset();
        assert_eq!(s.count, 0);
        assert!(!s.rewarded);
    }

    #[test]
    fn test_reward_payout_trigger() {
        let mut s = Streak { count: 7, rewarded: false };
        assert!(s.try_reward(7));
        assert!(s.rewarded);
    }

    #[test]
    fn test_no_reward_below_threshold() {
        let mut s = Streak { count: 3, rewarded: false };
        assert!(!s.try_reward(7));
    }
}
