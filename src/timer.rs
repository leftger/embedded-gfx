//! Zero-allocation timer, stopwatch, and fixed-timestep accumulator.
//!
//! Inspired by Bevy's `bevy_time` (`Timer`, `Stopwatch`, and fixed step scheduling),
//! adapted for `no_std` microcontrollers. Works seamlessly with both `f32` seconds
//! and [`core::time::Duration`].

use core::time::Duration;

/// Behavior of a [`Timer`] when its duration is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimerMode {
    /// Stop advancing and stay finished until explicitly reset.
    #[default]
    Once,
    /// Wrap around automatically and restart upon reaching duration.
    Repeating,
}

/// Tracks elapsed time with pause, reset, and accumulation controls.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Stopwatch {
    elapsed: f32,
    paused: bool,
}

impl Stopwatch {
    /// Create a new unpaused stopwatch with `0.0` elapsed time.
    pub const fn new() -> Self {
        Self {
            elapsed: 0.0,
            paused: false,
        }
    }

    /// Advance elapsed time by `dt` seconds (if not paused).
    #[inline]
    pub fn tick(&mut self, dt: f32) -> &mut Self {
        if !self.paused {
            self.elapsed += dt;
        }
        self
    }

    /// Advance elapsed time by a [`core::time::Duration`].
    #[inline]
    pub fn tick_duration(&mut self, duration: Duration) -> &mut Self {
        self.tick(duration.as_secs_f32())
    }

    /// Elapsed time in seconds.
    #[inline]
    pub fn elapsed_secs(&self) -> f32 {
        self.elapsed
    }

    /// Elapsed time as a [`core::time::Duration`].
    #[inline]
    pub fn elapsed(&self) -> Duration {
        Duration::from_secs_f32(self.elapsed.max(0.0))
    }

    /// Set elapsed time in seconds.
    #[inline]
    pub fn set_elapsed(&mut self, time: f32) {
        self.elapsed = time;
    }

    /// Reset elapsed time to `0.0` (does not affect pause state).
    #[inline]
    pub fn reset(&mut self) {
        self.elapsed = 0.0;
    }

    /// Pause tracking. Calls to [`tick`](Stopwatch::tick) will have no effect.
    #[inline]
    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// Unpause tracking.
    #[inline]
    pub fn unpause(&mut self) {
        self.paused = false;
    }

    /// Returns `true` if paused.
    #[inline]
    pub fn is_paused(&self) -> bool {
        self.paused
    }
}

/// A timer that tracks duration, progress fraction, completion, and repetition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timer {
    stopwatch: Stopwatch,
    duration: f32,
    mode: TimerMode,
    just_finished: bool,
}

impl Default for Timer {
    fn default() -> Self {
        Self::from_seconds(1.0, TimerMode::Once)
    }
}

impl Timer {
    /// Create a new timer with a duration in seconds and a [`TimerMode`].
    pub const fn from_seconds(duration: f32, mode: TimerMode) -> Self {
        Self {
            stopwatch: Stopwatch::new(),
            duration: if duration > 0.0 { duration } else { 0.0 },
            mode,
            just_finished: false,
        }
    }

    /// Create a new timer from a [`core::time::Duration`].
    pub fn new(duration: Duration, mode: TimerMode) -> Self {
        Self::from_seconds(duration.as_secs_f32(), mode)
    }

    /// Advance the timer by `dt` seconds and evaluate completion.
    pub fn tick(&mut self, dt: f32) -> &mut Self {
        if self.stopwatch.is_paused() {
            self.just_finished = false;
            return self;
        }

        if self.mode == TimerMode::Once && self.is_finished() {
            self.just_finished = false;
            return self;
        }

        self.stopwatch.tick(dt);

        if self.stopwatch.elapsed_secs() >= self.duration {
            self.just_finished = true;
            match self.mode {
                TimerMode::Once => {
                    self.stopwatch.set_elapsed(self.duration);
                }
                TimerMode::Repeating => {
                    if self.duration > 0.0 {
                        let rem = self.stopwatch.elapsed_secs() % self.duration;
                        self.stopwatch.set_elapsed(rem);
                    } else {
                        self.stopwatch.set_elapsed(0.0);
                    }
                }
            }
        } else {
            self.just_finished = false;
        }

        self
    }

    /// Advance the timer by a [`core::time::Duration`].
    #[inline]
    pub fn tick_duration(&mut self, duration: Duration) -> &mut Self {
        self.tick(duration.as_secs_f32())
    }

    /// `true` if the timer is finished (or reached its target during this tick).
    #[inline]
    pub fn is_finished(&self) -> bool {
        self.stopwatch.elapsed_secs() >= self.duration
    }

    /// `true` only on the tick when the timer crossed or reached its duration.
    #[inline]
    pub fn just_finished(&self) -> bool {
        self.just_finished
    }

    /// Timer completion percentage between `0.0` and `1.0`.
    #[inline]
    pub fn fraction(&self) -> f32 {
        if self.duration <= 0.0 {
            1.0
        } else {
            (self.stopwatch.elapsed_secs() / self.duration).clamp(0.0, 1.0)
        }
    }

    /// Remaining duration in seconds until completion.
    #[inline]
    pub fn remaining_secs(&self) -> f32 {
        (self.duration - self.stopwatch.elapsed_secs()).max(0.0)
    }

    /// Reset the timer to 0 elapsed time.
    #[inline]
    pub fn reset(&mut self) {
        self.stopwatch.reset();
        self.just_finished = false;
    }

    /// Pause the timer.
    #[inline]
    pub fn pause(&mut self) {
        self.stopwatch.pause();
    }

    /// Unpause the timer.
    #[inline]
    pub fn unpause(&mut self) {
        self.stopwatch.unpause();
    }

    /// `true` if paused.
    #[inline]
    pub fn is_paused(&self) -> bool {
        self.stopwatch.is_paused()
    }

    /// Elapsed time in seconds.
    #[inline]
    pub fn elapsed_secs(&self) -> f32 {
        self.stopwatch.elapsed_secs()
    }

    /// Total target duration in seconds.
    #[inline]
    pub fn duration_secs(&self) -> f32 {
        self.duration
    }
}

/// Accumulator for fixed-timestep game / physics loops (e.g. 50 Hz or 60 Hz).
///
/// Decouples erratic render framerates from deterministic fixed update steps,
/// avoiding physics instability on microcontrollers with variable SPI display scanout times.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FixedTimestep {
    step: f32,
    accumulator: f32,
    max_substeps: u32,
}

impl FixedTimestep {
    /// Create a fixed timestep with a given step duration (e.g. `1.0 / 60.0`)
    /// and a maximum number of substeps per tick (e.g. `4`) to prevent spiral-of-death.
    pub const fn from_hz(hz: f32, max_substeps: u32) -> Self {
        let step = if hz > 0.0 { 1.0 / hz } else { 1.0 / 60.0 };
        Self {
            step,
            accumulator: 0.0,
            max_substeps,
        }
    }

    /// Create from step delta in seconds.
    pub const fn from_step(step: f32, max_substeps: u32) -> Self {
        Self {
            step: if step > 0.0 { step } else { 0.01666667 },
            accumulator: 0.0,
            max_substeps,
        }
    }

    /// Feed frame delta-time `frame_dt` and return an iterator of fixed timesteps to simulate.
    pub fn update(&mut self, frame_dt: f32) -> FixedStepIter {
        self.accumulator += frame_dt;
        let mut steps = 0;
        while self.accumulator >= self.step && steps < self.max_substeps {
            self.accumulator -= self.step;
            steps += 1;
        }

        // Clamp accumulator if we exceeded max_substeps to prevent spiral of death
        if self.accumulator >= self.step {
            self.accumulator = 0.0;
        }

        FixedStepIter {
            step: self.step,
            remaining: steps,
        }
    }

    /// Interpolation factor `alpha` in `0.0..1.0` between the previous and current fixed states.
    /// Useful for smooth render state interpolation.
    #[inline]
    pub fn alpha(&self) -> f32 {
        (self.accumulator / self.step).clamp(0.0, 1.0)
    }

    /// Fixed step delta in seconds.
    #[inline]
    pub fn step_secs(&self) -> f32 {
        self.step
    }
}

/// Iterator yielding fixed timesteps for a frame.
pub struct FixedStepIter {
    step: f32,
    remaining: u32,
}

impl Iterator for FixedStepIter {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining > 0 {
            self.remaining -= 1;
            Some(self.step)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stopwatch() {
        let mut sw = Stopwatch::new();
        assert_eq!(sw.elapsed_secs(), 0.0);
        sw.tick(0.5);
        assert_eq!(sw.elapsed_secs(), 0.5);
        sw.pause();
        sw.tick(0.5);
        assert_eq!(sw.elapsed_secs(), 0.5);
        sw.unpause();
        sw.tick(0.5);
        assert_eq!(sw.elapsed_secs(), 1.0);
        sw.reset();
        assert_eq!(sw.elapsed_secs(), 0.0);
    }

    #[test]
    fn test_timer_once() {
        let mut timer = Timer::from_seconds(1.0, TimerMode::Once);
        assert!(!timer.is_finished());
        assert!(!timer.just_finished());
        assert_eq!(timer.fraction(), 0.0);

        timer.tick(0.5);
        assert!(!timer.is_finished());
        assert_eq!(timer.fraction(), 0.5);
        assert_eq!(timer.remaining_secs(), 0.5);

        timer.tick(0.5);
        assert!(timer.is_finished());
        assert!(timer.just_finished());
        assert_eq!(timer.fraction(), 1.0);

        timer.tick(0.5);
        assert!(timer.is_finished());
        assert!(!timer.just_finished());
    }

    #[test]
    fn test_timer_repeating() {
        let mut timer = Timer::from_seconds(1.0, TimerMode::Repeating);
        timer.tick(0.7);
        assert!(!timer.just_finished());

        timer.tick(0.5); // total 1.2 -> wraps to 0.2
        assert!(timer.just_finished());
        assert!((timer.elapsed_secs() - 0.2).abs() < 1e-5);
    }

    #[test]
    fn test_fixed_timestep() {
        let mut fixed = FixedTimestep::from_hz(60.0, 4); // 0.0166667s
        let step = fixed.step_secs();

        // 1 full step + half step in frame
        let count = fixed.update(step * 1.5).count();
        assert_eq!(count, 1);
        assert!(fixed.alpha() > 0.4 && fixed.alpha() < 0.6);

        // Another half step -> triggers second step
        let count2 = fixed.update(step * 0.5).count();
        assert_eq!(count2, 1);
    }

    #[test]
    fn test_stopwatch_duration_controls_and_pause() {
        let mut sw = Stopwatch::new();
        sw.tick_duration(Duration::from_millis(250));
        assert!((sw.elapsed_secs() - 0.25).abs() < 1e-4);
        assert_eq!(sw.elapsed(), Duration::from_secs_f32(0.25));
        sw.set_elapsed(3.0);
        assert_eq!(sw.elapsed_secs(), 3.0);
        sw.pause();
        assert!(sw.is_paused());
        sw.tick(1.0);
        assert_eq!(sw.elapsed_secs(), 3.0);
        sw.unpause();
        assert!(!sw.is_paused());
        sw.reset();
        assert_eq!(sw.elapsed_secs(), 0.0);
    }

    #[test]
    fn test_timer_pause_duration_and_reset_paths() {
        let mut timer = Timer::new(Duration::from_millis(500), TimerMode::Repeating);
        assert!((timer.duration_secs() - 0.5).abs() < 1e-4);
        timer.tick(0.1);
        assert!((timer.elapsed_secs() - 0.1).abs() < 1e-4);
        timer.pause();
        assert!(timer.is_paused());
        timer.tick(0.2);
        assert!((timer.elapsed_secs() - 0.1).abs() < 1e-4);
        timer.unpause();
        timer.tick_duration(Duration::from_millis(500));
        assert!(timer.just_finished());
        timer.reset();
        assert!(!timer.is_finished());
        assert_eq!(timer.elapsed_secs(), 0.0);
        let _ = Timer::default();
    }
}
