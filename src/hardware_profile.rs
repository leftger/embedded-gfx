use core::sync::atomic::{AtomicU32, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceTransport {
    None,
    #[cfg(feature = "rtt-trace")]
    Rtt,
    #[cfg(feature = "itm-trace")]
    Itm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CycleSample {
    pub label: &'static str,
    pub cycles: u32,
}

static LAST_SAMPLE_CYCLES: AtomicU32 = AtomicU32::new(0);

#[cfg(all(
    feature = "dwt-profiler",
    target_arch = "arm",
    target_has_atomic = "32"
))]
const DEMCR: *mut u32 = 0xE000_EDFC as *mut u32;
#[cfg(all(
    feature = "dwt-profiler",
    target_arch = "arm",
    target_has_atomic = "32"
))]
const DWT_CTRL: *mut u32 = 0xE000_1000 as *mut u32;
#[cfg(all(
    feature = "dwt-profiler",
    target_arch = "arm",
    target_has_atomic = "32"
))]
const DWT_CYCCNT: *mut u32 = 0xE000_1004 as *mut u32;

#[cfg(all(
    feature = "dwt-profiler",
    target_arch = "arm",
    target_has_atomic = "32"
))]
pub fn init_dwt_cycle_counter() {
    // SAFETY: Accessing ARM debug registers by fixed address on supported targets.
    unsafe {
        let demcr = core::ptr::read_volatile(DEMCR);
        core::ptr::write_volatile(DEMCR, demcr | (1 << 24));
        let dwt_ctrl = core::ptr::read_volatile(DWT_CTRL);
        core::ptr::write_volatile(DWT_CTRL, dwt_ctrl | 1);
    }
}

#[cfg(not(all(
    feature = "dwt-profiler",
    target_arch = "arm",
    target_has_atomic = "32"
)))]
pub fn init_dwt_cycle_counter() {}

#[cfg(all(
    feature = "dwt-profiler",
    target_arch = "arm",
    target_has_atomic = "32"
))]
pub fn read_cycle_counter() -> Option<u32> {
    // SAFETY: Accessing ARM DWT cycle counter register on supported targets.
    Some(unsafe { core::ptr::read_volatile(DWT_CYCCNT) })
}

#[cfg(not(all(
    feature = "dwt-profiler",
    target_arch = "arm",
    target_has_atomic = "32"
)))]
pub fn read_cycle_counter() -> Option<u32> {
    None
}

pub fn sample_cycles(label: &'static str, start_cycles: u32, end_cycles: u32) -> CycleSample {
    let cycles = end_cycles.wrapping_sub(start_cycles);
    LAST_SAMPLE_CYCLES.store(cycles, Ordering::Relaxed);
    CycleSample { label, cycles }
}

pub fn last_sample_cycles() -> u32 {
    LAST_SAMPLE_CYCLES.load(Ordering::Relaxed)
}

pub fn emit_trace(_sample: CycleSample) {
    #[cfg(feature = "rtt-trace")]
    {
        #[cfg(feature = "std")]
        {
            std::println!(
                "RTT_TRACE label={} cycles={}",
                _sample.label,
                _sample.cycles
            );
        }
    }

    #[cfg(feature = "itm-trace")]
    {
        #[cfg(feature = "std")]
        {
            std::println!(
                "ITM_TRACE label={} cycles={}",
                _sample.label,
                _sample.cycles
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_cycles_wraps_and_updates_last_value() {
        let sample = sample_cycles("frame", u32::MAX - 3, 5);
        assert_eq!(sample.label, "frame");
        assert_eq!(sample.cycles, 9);
        assert_eq!(last_sample_cycles(), 9);
    }

    #[test]
    fn read_cycle_counter_is_none_on_host_builds() {
        #[cfg(not(all(feature = "dwt-profiler", target_arch = "arm")))]
        assert_eq!(read_cycle_counter(), None);
    }
}
