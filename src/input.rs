//! Platform-agnostic input state.
//!
//! Fill an [`InputState`] each frame from your hardware (GPIO buttons,
//! I²C gamepad, USB HID, SDL2 keyboard, …) and pass it to
//! [`CharacterController::tick`](crate::character::CharacterController::tick).
//!
//! All fields default to zero / false so you only need to set the ones
//! relevant to your hardware.

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
    use super::InputState;

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
}
