#[cfg(test)]
mod tests {
    #[derive(Debug, PartialEq, Clone)]
    enum ScheduleState { Active, Paused, Completed }

    #[derive(Debug, Clone)]
    struct Schedule { id: u32, state: ScheduleState, executions: u32 }

    impl Schedule {
        fn new(id: u32) -> Self { Self { id, state: ScheduleState::Active, executions: 0 } }
        fn execute(&mut self) { if self.state == ScheduleState::Active { self.executions += 1; } }
        fn pause(&mut self) { self.state = ScheduleState::Paused; }
        fn resume(&mut self) { self.state = ScheduleState::Active; }
    }

    #[test]
    fn test_schedule_creation() {
        let s = Schedule::new(1);
        assert_eq!(s.id, 1);
        assert_eq!(s.state, ScheduleState::Active);
    }

    #[test]
    fn test_schedule_execution() {
        let mut s = Schedule::new(1);
        s.execute(); s.execute();
        assert_eq!(s.executions, 2);
    }

    #[test]
    fn test_schedule_pause_stops_execution() {
        let mut s = Schedule::new(1);
        s.pause(); s.execute();
        assert_eq!(s.executions, 0);
    }

    #[test]
    fn test_schedule_resume() {
        let mut s = Schedule::new(1);
        s.pause(); s.resume(); s.execute();
        assert_eq!(s.executions, 1);
    }
}
