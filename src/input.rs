//! Platform-agnostic input state, buttons, analog sticks, and debouncing.
//!
//! Inspired by `bevy_input`, this module provides zero-allocation input
//! structures tailored for microcontrollers and embedded devices (GPIO buttons,
//! ADC analog sticks, I²C gamepads, USB HID, etc.).

use core::fmt::Debug;
use nalgebra::Vector2;

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use micromath::F32Ext;

/// Standard gamepad / controller buttons represented as a compact bitmask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum GamepadButton {
    DpadUp = 0,
    DpadDown = 1,
    DpadLeft = 2,
    DpadRight = 3,
    ActionA = 4,
    ActionB = 5,
    ActionX = 6,
    ActionY = 7,
    Start = 8,
    Select = 9,
    LeftBumper = 10,
    RightBumper = 11,
    LeftTrigger = 12,
    RightTrigger = 13,
    LeftThumb = 14,
    RightThumb = 15,
}

impl GamepadButton {
    /// Returns the bitmask for this button.
    #[inline]
    pub const fn mask(self) -> u16 {
        1 << (self as u8)
    }
}

/// Zero-allocation bitmask button input tracker.
///
/// Tracks current held buttons, just-pressed edges, and just-released edges.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct ButtonInput {
    current: u16,
    previous: u16,
}

impl ButtonInput {
    /// Create a new, empty button state.
    pub const fn new() -> Self {
        Self {
            current: 0,
            previous: 0,
        }
    }

    /// Mark a button as currently pressed.
    #[inline]
    pub fn press(&mut self, button: GamepadButton) {
        self.current |= button.mask();
    }

    /// Mark a button as currently released.
    #[inline]
    pub fn release(&mut self, button: GamepadButton) {
        self.current &= !button.mask();
    }

    /// Set button press state from a boolean flag (e.g. reading a digital GPIO pin).
    #[inline]
    pub fn set(&mut self, button: GamepadButton, pressed: bool) {
        if pressed {
            self.press(button);
        } else {
            self.release(button);
        }
    }

    /// Returns `true` if the button is currently held down.
    #[inline]
    pub fn pressed(&self, button: GamepadButton) -> bool {
        (self.current & button.mask()) != 0
    }

    /// Returns `true` if the button was pressed during this tick (transitioned from released to pressed).
    #[inline]
    pub fn just_pressed(&self, button: GamepadButton) -> bool {
        let mask = button.mask();
        (self.current & mask) != 0 && (self.previous & mask) == 0
    }

    /// Returns `true` if the button was released during this tick (transitioned from pressed to released).
    #[inline]
    pub fn just_released(&self, button: GamepadButton) -> bool {
        let mask = button.mask();
        (self.current & mask) == 0 && (self.previous & mask) != 0
    }

    /// Raw bitmask of currently pressed buttons.
    #[inline]
    pub const fn raw_current(&self) -> u16 {
        self.current
    }

    /// Advance the frame state. Call this once at the start or end of each frame tick.
    #[inline]
    pub fn update(&mut self) {
        self.previous = self.current;
    }

    /// Reset all button states to released.
    #[inline]
    pub fn clear(&mut self) {
        self.current = 0;
        self.previous = 0;
    }
}

/// 2-axis analog stick or directional pad input with deadzone filtering.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct VirtualAxis2D {
    /// X axis in `[-1.0, 1.0]`. Positive is right.
    pub x: f32,
    /// Y axis in `[-1.0, 1.0]`. Positive is forward / up.
    pub y: f32,
}

impl VirtualAxis2D {
    /// Create from direct `(x, y)` floats in `[-1.0, 1.0]`.
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Construct from discrete D-pad / arrow button states.
    pub fn from_dpad(up: bool, down: bool, left: bool, right: bool) -> Self {
        let x = match (left, right) {
            (true, false) => -1.0,
            (false, true) => 1.0,
            _ => 0.0,
        };
        let y = match (down, up) {
            (true, false) => -1.0,
            (false, true) => 1.0,
            _ => 0.0,
        };
        Self::new(x, y)
    }

    /// Construct from raw 10-bit or 12-bit microcontroller ADC readings.
    ///
    /// `center_x` and `center_y` are the resting ADC values (e.g. 2048 for a 12-bit ADC).
    pub fn from_adc(raw_x: u16, raw_y: u16, center_x: u16, center_y: u16, max_val: u16) -> Self {
        let half_range = (max_val / 2).max(1) as f32;
        let x = ((raw_x as f32 - center_x as f32) / half_range).clamp(-1.0, 1.0);
        let y = ((raw_y as f32 - center_y as f32) / half_range).clamp(-1.0, 1.0);
        Self::new(x, y)
    }

    /// Returns a new axis value with radial deadzone filtering.
    ///
    /// Jitter below `threshold` is clamped to zero, and values above `threshold`
    /// are rescaled smoothly to fill the $[0.0, 1.0]$ magnitude range.
    pub fn with_radial_deadzone(&self, threshold: f32) -> Self {
        let threshold = threshold.clamp(0.0, 0.99);
        let len_sq = self.x * self.x + self.y * self.y;
        if len_sq <= threshold * threshold {
            return Self::new(0.0, 0.0);
        }

        let len = len_sq.sqrt();
        if len <= 1e-4 {
            return Self::new(0.0, 0.0);
        }

        let normalized_factor = (len - threshold) / (1.0 - threshold);
        let scale = (normalized_factor / len).min(1.0);
        Self::new(self.x * scale, self.y * scale)
    }

    /// Returns the vector as a [`nalgebra::Vector2<f32>`].
    pub fn to_vector(&self) -> Vector2<f32> {
        Vector2::new(self.x, self.y)
    }
}

/// A comprehensive virtual gamepad combining buttons and dual analog sticks.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct VirtualGamepad {
    pub buttons: ButtonInput,
    pub left_stick: VirtualAxis2D,
    pub right_stick: VirtualAxis2D,
}

impl VirtualGamepad {
    /// Create a new virtual gamepad in neutral state.
    pub const fn new() -> Self {
        Self {
            buttons: ButtonInput::new(),
            left_stick: VirtualAxis2D::new(0.0, 0.0),
            right_stick: VirtualAxis2D::new(0.0, 0.0),
        }
    }

    /// Tick the gamepad buttons for the next frame.
    #[inline]
    pub fn update(&mut self) {
        self.buttons.update();
    }

    /// Convert gamepad state into the character controller [`InputState`].
    pub fn to_input_state(&self) -> InputState {
        InputState {
            forward: self.left_stick.y,
            strafe: self.left_stick.x,
            look_yaw: self.right_stick.x,
            look_pitch: self.right_stick.y,
            jump: self.buttons.pressed(GamepadButton::ActionA),
            sprint: self.buttons.pressed(GamepadButton::RightBumper),
        }
    }
}

/// Software debounce filter for physical microcontroller GPIO pins.
///
/// Filters contact bounce by requiring a pin state to remain steady for
/// `threshold` consecutive ticks before propagating the state change.
#[derive(Clone, Copy, Debug)]
pub struct DebounceFilter<const PINS: usize> {
    states: [bool; PINS],
    counters: [u8; PINS],
    threshold: u8,
}

impl<const PINS: usize> DebounceFilter<PINS> {
    /// Create a debounce filter with a tick threshold (e.g. 2–5 ticks).
    pub const fn new(threshold: u8) -> Self {
        Self {
            states: [false; PINS],
            counters: [0; PINS],
            threshold,
        }
    }

    /// Feed raw pin reading and return the debounced, stable state.
    pub fn update(&mut self, pin_idx: usize, raw_reading: bool) -> bool {
        if pin_idx >= PINS {
            return false;
        }

        if raw_reading != self.states[pin_idx] {
            self.counters[pin_idx] = self.counters[pin_idx].saturating_add(1);
            if self.counters[pin_idx] >= self.threshold {
                self.states[pin_idx] = raw_reading;
                self.counters[pin_idx] = 0;
            }
        } else {
            self.counters[pin_idx] = 0;
        }

        self.states[pin_idx]
    }

    /// Get the current debounced state of a pin.
    pub fn state(&self, pin_idx: usize) -> bool {
        if pin_idx < PINS {
            self.states[pin_idx]
        } else {
            false
        }
    }
}

/// Normalised per-frame input fed into the character controller.
///
/// Analog axes are in `[-1.0, 1.0]`; the controller applies its own speed
/// scaling so you do not need to multiply by delta-time here.
#[derive(Clone, Copy, Default, Debug)]
#[cfg_attr(feature = "std", derive(PartialEq))]
pub struct InputState {
    /// Forward / backward motion.  `+1.0` = walk forward, `-1.0` = backward.
    pub forward: f32,
    /// Lateral motion.  `+1.0` = strafe right, `-1.0` = strafe left.
    pub strafe: f32,
    /// Yaw (horizontal look) delta this frame, in radians.
    /// Positive rotates the view to the right.
    pub look_yaw: f32,
    /// Pitch (vertical look) delta this frame, in radians.
    /// Positive tilts the view upward.
    pub look_pitch: f32,
    /// `true` on any frame the jump button is held while the character is
    /// grounded.  The controller self-resets this internally via `on_ground`.
    pub jump: bool,
    /// Sprint modifier — held to move at `run_speed` instead of `walk_speed`.
    pub sprint: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_input_state_is_neutral() {
        let input = InputState::default();
        assert_eq!(input.forward, 0.0);
        assert_eq!(input.strafe, 0.0);
        assert_eq!(input.look_yaw, 0.0);
        assert_eq!(input.look_pitch, 0.0);
        assert!(!input.jump);
        assert!(!input.sprint);
    }

    #[test]
    fn input_state_can_store_full_analog_range() {
        let input = InputState {
            forward: 1.0,
            strafe: -1.0,
            look_yaw: 0.75,
            look_pitch: -0.5,
            jump: true,
            sprint: true,
        };
        assert_eq!(input.forward, 1.0);
        assert_eq!(input.strafe, -1.0);
        assert_eq!(input.look_yaw, 0.75);
        assert_eq!(input.look_pitch, -0.5);
        assert!(input.jump);
        assert!(input.sprint);
    }

    #[test]
    fn test_button_input_edges() {
        let mut buttons = ButtonInput::new();
        assert!(!buttons.pressed(GamepadButton::ActionA));
        assert!(!buttons.just_pressed(GamepadButton::ActionA));

        // Press A
        buttons.press(GamepadButton::ActionA);
        assert!(buttons.pressed(GamepadButton::ActionA));
        assert!(buttons.just_pressed(GamepadButton::ActionA));
        assert!(!buttons.just_released(GamepadButton::ActionA));

        // Frame advance: held
        buttons.update();
        assert!(buttons.pressed(GamepadButton::ActionA));
        assert!(!buttons.just_pressed(GamepadButton::ActionA));
        assert!(!buttons.just_released(GamepadButton::ActionA));

        // Release A
        buttons.release(GamepadButton::ActionA);
        assert!(!buttons.pressed(GamepadButton::ActionA));
        assert!(!buttons.just_pressed(GamepadButton::ActionA));
        assert!(buttons.just_released(GamepadButton::ActionA));

        // Frame advance: released
        buttons.update();
        assert!(!buttons.pressed(GamepadButton::ActionA));
        assert!(!buttons.just_released(GamepadButton::ActionA));
    }

    #[test]
    fn test_virtual_axis_radial_deadzone() {
        let axis = VirtualAxis2D::new(0.05, 0.05);
        let filtered = axis.with_radial_deadzone(0.15);
        assert_eq!(filtered.x, 0.0);
        assert_eq!(filtered.y, 0.0);

        let axis_active = VirtualAxis2D::new(0.8, 0.0);
        let filtered_active = axis_active.with_radial_deadzone(0.2);
        assert!(filtered_active.x > 0.7);
        assert_eq!(filtered_active.y, 0.0);
    }

    #[test]
    fn test_debounce_filter() {
        let mut debounce = DebounceFilter::<4>::new(3);
        assert!(!debounce.state(0));

        // Noise spike of 2 ticks
        assert!(!debounce.update(0, true));
        assert!(!debounce.update(0, true));
        assert!(!debounce.update(0, false)); // bounced back
        assert!(!debounce.state(0));

        // Solid hold of 3 ticks
        assert!(!debounce.update(0, true));
        assert!(!debounce.update(0, true));
        assert!(debounce.update(0, true)); // 3rd tick stabilizes
        assert!(debounce.state(0));
    }
}
