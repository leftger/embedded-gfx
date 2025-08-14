use core::fmt::Write;
use core::mem;
use embassy_time::Instant;
use heapless::String;

fn now_us() -> u64 {
    Instant::now().as_micros() as u64
}

#[derive(Debug)]
pub struct PerformanceCounter {
    frame_count: u64,
    text: String<256>,
    old_text: String<256>,
    only_fps: bool,
    start_time_us: u64,
    last_measurement_time_us: u64,
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
    }

    pub fn add_measurement(&mut self, label: &str) {
        if self.only_fps {
            return;
        }
        let now = now_us();
        let duration = now.saturating_sub(self.last_measurement_time_us);
        let _ = write!(self.text, "{}: {}us\n", label, duration);
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
        let _ = write!(self.text, "total: {}us\nfps: {}\n", total_us, fps);
        self.old_text = self.text.clone();
    }

    pub fn get_text(&self) -> &str {
        &self.old_text
    }
}
