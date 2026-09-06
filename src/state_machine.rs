//! Zero-allocation application state machine inspired by `bevy_state`.
//!
//! Embedded titles transition through high-level phases (e.g. `Boot`,
//! `TitleScreen`, `Gameplay`, `Pause`, `GameOver`).
//!
//! This module provides a deterministic, zero-heap state machine wrapper around
//! any `Copy + PartialEq` enum. It exposes change detection (`just_entered`,
//! `just_exited`) so systems can run one-shot enter/exit routines without
//! global boolean flags.

use core::fmt::Debug;

/// Finite application state tracker with frame change detection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateMachine<S: Copy + PartialEq> {
    current: S,
    previous: S,
    changed: bool,
}

impl<S: Copy + PartialEq> StateMachine<S> {
    /// Create a state machine initialized to `initial`.
    ///
    /// On the very first frame before `update()` is called, `changed` is `true`
    /// and `just_entered(initial)` returns `true`.
    pub const fn new(initial: S) -> Self {
        Self {
            current: initial,
            previous: initial,
            changed: true,
        }
    }

    /// The currently active state.
    #[inline]
    pub const fn current(&self) -> S {
        self.current
    }

    /// The state that was active on the preceding tick.
    #[inline]
    pub const fn previous(&self) -> S {
        self.previous
    }

    /// Returns `true` if currently in `state`.
    #[inline]
    pub fn is(&self, state: S) -> bool {
        self.current == state
    }

    /// Returns `true` if the state transitioned *into* `state` during this frame tick.
    #[inline]
    pub fn just_entered(&self, state: S) -> bool {
        self.changed && self.current == state
    }

    /// Returns `true` if the state transitioned *out of* `state` during this frame tick.
    #[inline]
    pub fn just_exited(&self, state: S) -> bool {
        self.changed && self.previous == state && self.current != state
    }

    /// Returns `true` if a state transition occurred on this frame tick.
    #[inline]
    pub const fn has_changed(&self) -> bool {
        self.changed
    }

    /// Request a transition to `next`.
    ///
    /// If `next` equals the current state, this is a no-op and returns `false`.
    /// Otherwise, sets the next state, marks `changed = true`, and returns `true`.
    pub fn set(&mut self, next: S) -> bool {
        if self.current == next {
            false
        } else {
            self.previous = self.current;
            self.current = next;
            self.changed = true;
            true
        }
    }

    /// Advance to the next frame tick.
    ///
    /// Clears the `changed` flag and synchronizes `previous = current`.
    /// Call this once per frame in your main loop.
    #[inline]
    pub fn update(&mut self) {
        self.changed = false;
        self.previous = self.current;
    }
}

/// Container that holds data only active while the state machine matches `target_state`.
#[derive(Clone, Copy, Debug, Default)]
pub struct StateScoped<T, S: Copy + PartialEq> {
    data: Option<T>,
    target_state: S,
}

impl<T, S: Copy + PartialEq> StateScoped<T, S> {
    /// Create a container scoped to `target_state`.
    pub const fn new(target_state: S) -> Self {
        Self {
            data: None,
            target_state,
        }
    }

    /// Put data into the container.
    pub fn set(&mut self, value: T) {
        self.data = Some(value);
    }

    /// Access data if the state machine is currently in `target_state`.
    pub fn get<'a>(&'a self, sm: &StateMachine<S>) -> Option<&'a T> {
        if sm.is(self.target_state) {
            self.data.as_ref()
        } else {
            None
        }
    }

    /// Mutably access data if the state machine is currently in `target_state`.
    pub fn get_mut<'a>(&'a mut self, sm: &StateMachine<S>) -> Option<&'a mut T> {
        if sm.is(self.target_state) {
            self.data.as_mut()
        } else {
            None
        }
    }

    /// Automatically clears internal data when exiting `target_state`.
    pub fn update(&mut self, sm: &StateMachine<S>) {
        if sm.just_exited(self.target_state) {
            self.data = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum GameState {
        Title,
        Playing,
        GameOver,
    }

    #[test]
    fn test_state_machine_lifecycle() {
        let mut sm = StateMachine::new(GameState::Title);
        assert!(sm.is(GameState::Title));
        assert!(sm.just_entered(GameState::Title));
        assert!(!sm.just_exited(GameState::Title));

        // Tick 1
        sm.update();
        assert!(sm.is(GameState::Title));
        assert!(!sm.just_entered(GameState::Title));
        assert!(!sm.has_changed());

        // Transition to Playing
        assert!(sm.set(GameState::Playing));
        assert!(sm.is(GameState::Playing));
        assert!(sm.just_entered(GameState::Playing));
        assert!(sm.just_exited(GameState::Title));
        assert!(sm.has_changed());

        // Redundant set
        assert!(!sm.set(GameState::Playing));

        // Tick 2
        sm.update();
        assert!(sm.is(GameState::Playing));
        assert!(!sm.just_entered(GameState::Playing));
        assert!(!sm.just_exited(GameState::Title));

        // Transition to GameOver
        sm.set(GameState::GameOver);
        assert!(sm.just_entered(GameState::GameOver));
        assert!(sm.just_exited(GameState::Playing));
    }

    #[test]
    fn test_state_scoped() {
        let mut sm = StateMachine::new(GameState::Title);
        let mut playing_data = StateScoped::new(GameState::Playing);
        playing_data.set(42);

        // While in Title, data is not accessible
        assert_eq!(playing_data.get(&sm), None);

        // Transition to Playing
        sm.set(GameState::Playing);
        assert_eq!(playing_data.get(&sm), Some(&42));

        // Transition to GameOver and update
        sm.set(GameState::GameOver);
        playing_data.update(&sm);
        sm.update();

        // Data should have been cleared on exit
        assert_eq!(playing_data.get(&sm), None);
    }
}
