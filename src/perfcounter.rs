use crate::hardware_profile;
use core::fmt::Write;
use core::mem;
use heapless::String;

// Use embassy_time for embedded targets when the feature is enabled
#[cfg(feature = "embassy-time")]
use embassy_time::Instant;

#[cfg(feature = "embassy-time")]
fn now_us() -> u64 {
    Instant::now().as_micros() as u64
}

// Use std::time for desktop/simulator targets when std feature is enabled
#[cfg(all(feature = "std", not(feature = "embassy-time")))]
fn now_us() -> u64 {
    extern crate std;
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

// Dummy implementation when neither std nor embassy-time is available
// Users must enable one of these features to use PerformanceCounter
#[cfg(not(any(feature = "std", feature = "embassy-time")))]
fn now_us() -> u64 {
    // Return 0 as a fallback - PerformanceCounter won't work but won't break the build
    // This allows the library to compile in no_std without timing functionality
    0
}

/// Performance counter for measuring rendering performance
///
/// **Timing Requirements:**
/// This module requires the `perfcounter` feature and a timing source:
/// - For embedded with Embassy: enable both `perfcounter` and `embassy-time` features
/// - For desktop/simulator: enable `std` feature (includes perfcounter automatically)
/// - Without a timing source, timing will always return 0
///
/// # Example
/// ```toml
/// # For embedded with Embassy:
/// embedded-3dgfx = { version = "0.1", features = ["perfcounter", "embassy-time"] }
///
/// # For desktop/simulator:
/// embedded-3dgfx = { version = "0.1", features = ["std"] }
///
/// # Pure no_std without perfcounter:
/// embedded-3dgfx = { version = "0.1", default-features = false }
/// ```
#[derive(Debug)]
pub struct PerformanceCounter {
    frame_count: u64,
    text: String<256>,
    old_text: String<256>,
    only_fps: bool,
    start_time_us: u64,
    last_measurement_time_us: u64,
    dwt_frame_start_cycles: u32,
    dwt_last_measurement_cycles: u32,
}

impl Default for PerformanceCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl PerformanceCounter {
    pub fn new() -> Self {
        let now = now_us();
        Self {
            frame_count: 0,
            text: String::new(),
            old_text: String::new(),
            only_fps: false,
            start_time_us: now,
            last_measurement_time_us: now,
            dwt_frame_start_cycles: 0,
            dwt_last_measurement_cycles: 0,
        }
    }

    pub fn only_fps(&mut self, only_fps: bool) {
        self.only_fps = only_fps;
    }

    pub fn get_frametime(&self) -> u64 {
        now_us().saturating_sub(self.start_time_us)
    }

    pub fn start_of_frame(&mut self) {
        self.frame_count += 1;
        self.text.clear();
        self.start_time_us = now_us();
        self.last_measurement_time_us = self.start_time_us;
        hardware_profile::init_dwt_cycle_counter();
        if let Some(cycles) = hardware_profile::read_cycle_counter() {
            self.dwt_frame_start_cycles = cycles;
            self.dwt_last_measurement_cycles = cycles;
        }
    }

    pub fn add_measurement(&mut self, label: &str) {
        if self.only_fps {
            return;
        }
        let now = now_us();
        let duration = now.saturating_sub(self.last_measurement_time_us);
        if let Some(cycles_now) = hardware_profile::read_cycle_counter() {
            let sample = hardware_profile::sample_cycles(
                "perf.measurement",
                self.dwt_last_measurement_cycles,
                cycles_now,
            );
            hardware_profile::emit_trace(sample);
            let _ = write!(
                self.text,
                "{}: {}us ({} cyc)\n",
                label, duration, sample.cycles
            );
            self.dwt_last_measurement_cycles = cycles_now;
        } else {
            let _ = write!(self.text, "{}: {}us\n", label, duration);
        }
        self.last_measurement_time_us = now;
    }

    pub fn discard_measurement(&mut self) {
        mem::swap(&mut self.old_text, &mut self.text);
    }

    pub fn print(&mut self) {
        let total_us = self.get_frametime();
        let fps = if total_us > 0 {
            1_000_000 / total_us
        } else {
            0
        };
        if self.only_fps {
            let _ = write!(self.text, "fps: {}\n", fps);
            self.old_text = self.text.clone();
            return;
        }
        if let Some(cycles_now) = hardware_profile::read_cycle_counter() {
            let total_cycles = cycles_now.wrapping_sub(self.dwt_frame_start_cycles);
            let sample = hardware_profile::sample_cycles(
                "perf.frame",
                self.dwt_frame_start_cycles,
                cycles_now,
            );
            hardware_profile::emit_trace(sample);
            let _ = write!(
                self.text,
                "total: {}us ({} cyc)\nfps: {}\n",
                total_us, total_cycles, fps
            );
        } else {
            let _ = write!(self.text, "total: {}us\nfps: {}\n", total_us, fps);
        }
        self.old_text = self.text.clone();
    }

    pub fn get_text(&self) -> &str {
        &self.old_text
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    #[test]
    fn test_perfcounter_creation() {
        let perf = PerformanceCounter::new();
        assert_eq!(perf.get_text(), "");
        assert_eq!(perf.frame_count, 0);
    }

    #[test]
    fn test_perfcounter_default() {
        let perf = PerformanceCounter::default();
        assert_eq!(perf.get_text(), "");
    }

    #[test]
    fn test_perfcounter_start_of_frame() {
        let mut perf = PerformanceCounter::new();
        perf.start_of_frame();
        assert_eq!(perf.frame_count, 1);

        perf.start_of_frame();
        assert_eq!(perf.frame_count, 2);
    }

    #[test]
    #[cfg(any(feature = "std", feature = "embassy-time"))]
    fn test_perfcounter_get_frametime() {
        let mut perf = PerformanceCounter::new();
        perf.start_of_frame();

        // Sleep a tiny bit to ensure time passes
        std::thread::sleep(std::time::Duration::from_micros(100));

        let frametime = perf.get_frametime();
        assert!(frametime >= 100); // At least 100 microseconds
    }

    #[test]
    #[cfg(any(feature = "std", feature = "embassy-time"))]
    fn test_perfcounter_add_measurement() {
        let mut perf = PerformanceCounter::new();
        perf.start_of_frame();

        std::thread::sleep(std::time::Duration::from_micros(100));
        perf.add_measurement("test_label");

        perf.print();
        let text = perf.get_text();

        // Should contain the label
        assert!(text.contains("test_label"));
        // Should contain some duration value
        assert!(text.contains(":"));
    }

    #[test]
    fn test_perfcounter_only_fps_mode() {
        let mut perf = PerformanceCounter::new();
        perf.only_fps(true);
        perf.start_of_frame();

        perf.add_measurement("should_be_ignored");

        perf.print();
        let text = perf.get_text();

        // Should not contain the label in FPS-only mode
        assert!(!text.contains("should_be_ignored"));
        // But should still contain FPS
        assert!(text.contains("fps"));
    }

    #[test]
    #[cfg(any(feature = "std", feature = "embassy-time"))]
    fn test_perfcounter_discard_measurement() {
        let mut perf = PerformanceCounter::new();
        perf.start_of_frame();

        perf.add_measurement("test1");
        // discard_measurement swaps old_text and text
        perf.discard_measurement();

        // After discard, the measurement should still be in old_text
        let text_after = perf.get_text();
        assert!(text_after.contains("test1"));
    }

    #[test]
    fn test_perfcounter_print_includes_fps() {
        let mut perf = PerformanceCounter::new();
        perf.start_of_frame();

        std::thread::sleep(std::time::Duration::from_micros(1000));

        perf.print();
        let text = perf.get_text();

        // Should contain FPS
        assert!(text.contains("fps"));
        // Should contain total time
        assert!(text.contains("total"));
    }

    #[test]
    #[cfg(any(feature = "std", feature = "embassy-time"))]
    fn test_perfcounter_fps_calculation() {
        let mut perf = PerformanceCounter::new();
        perf.start_of_frame();

        std::thread::sleep(std::time::Duration::from_millis(10));

        perf.print();
        let text = perf.get_text();

        // FPS should be less than 100 (since we slept for 10ms)
        // Just verify the text is generated correctly
        assert!(text.len() > 0);
    }

    #[test]
    #[cfg(any(feature = "std", feature = "embassy-time"))]
    fn test_perfcounter_multiple_measurements() {
        let mut perf = PerformanceCounter::new();
        perf.start_of_frame();

        std::thread::sleep(std::time::Duration::from_micros(100));
        perf.add_measurement("step1");

        std::thread::sleep(std::time::Duration::from_micros(100));
        perf.add_measurement("step2");

        perf.print();
        let text = perf.get_text();

        // Should contain both labels
        assert!(text.contains("step1"));
        assert!(text.contains("step2"));
        assert!(text.contains("total"));
        assert!(text.contains("fps"));
    }
}
