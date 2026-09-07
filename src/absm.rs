//! Animation Blending State Machine (ABSM) for no_std embedded systems.
//!
//! Inspired by Fyrox's `fyrox-animation::machine`, adapted for zero-heap-allocation,
//! fixed-capacity MCU execution. Supports smooth crossfade transitions, 1D blend spaces
//! (e.g. Walk <-> Run based on speed), and parameter-driven state switching.
//!
//! # Example
//! ```
//! use embedded_3dgfx::absm::{AnimationStateMachine, StateNode, Transition, TransitionRule};
//! use embedded_3dgfx::skeleton::{AnimClip, BonePose};
//!
//! static IDLE_CLIP: AnimClip<'static> = AnimClip::new(&[], true);
//! static WALK_CLIP: AnimClip<'static> = AnimClip::new(&[], true);
//!
//! let mut sm: AnimationStateMachine<4, 4, 2> = AnimationStateMachine::new(0);
//! sm.set_state(0, StateNode::SingleClip(&IDLE_CLIP));
//! sm.set_state(1, StateNode::SingleClip(&WALK_CLIP));
//! sm.add_transition(Transition {
//!     from: 0,
//!     to: 1,
//!     fade_duration: 0.2,
//!     rule: TransitionRule::ParamGreaterThan(0, 0.1), // if param 0 (speed) > 0.1
//! });
//!
//! sm.set_param_float(0, 1.5);
//! sm.update(0.016);
//! ```

use crate::skeleton::{AnimClip, BonePose};

/// Rule triggering a state machine transition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransitionRule {
    /// Transition immediately.
    Immediate,
    /// Trigger when float parameter `param_index` is strictly greater than `threshold`.
    ParamGreaterThan(usize, f32),
    /// Trigger when float parameter `param_index` is strictly less than `threshold`.
    ParamLessThan(usize, f32),
    /// Trigger when boolean parameter `param_index` equals `expected`.
    ParamBool(usize, bool),
}

/// A node backing an animation state.
#[derive(Debug, Clone, Copy)]
pub enum StateNode<'a> {
    /// Play a single animation clip.
    SingleClip(&'a AnimClip<'a>),
    /// 1D Linear Blend between two animation clips based on a float parameter (e.g. Speed -> Walk vs Run).
    Blend1D {
        /// First animation clip (at or below `min_val`).
        clip_a: &'a AnimClip<'a>,
        /// Second animation clip (at or above `max_val`).
        clip_b: &'a AnimClip<'a>,
        /// Index of the float parameter controlling blend.
        param_index: usize,
        /// Minimum parameter value corresponding to 100% `clip_a`.
        min_val: f32,
        /// Maximum parameter value corresponding to 100% `clip_b`.
        max_val: f32,
    },
    /// Additive layer blending (e.g. Aim / Recoil / Hit reaction blended onto a base locomotion pose).
    AdditiveClip {
        /// Base locomotion clip.
        base_clip: &'a AnimClip<'a>,
        /// Additive overlay clip.
        additive_clip: &'a AnimClip<'a>,
        /// Float parameter index controlling additive weight in 0.0..=1.0.
        param_index: usize,
    },
    /// Multi-point 1D piecewise linear blend space across sorted (parameter_value, clip) pairs.
    BlendSpace1D {
        /// Array of parameter keys and corresponding animation clips.
        points: &'a [(f32, &'a AnimClip<'a>)],
        /// Float parameter index controlling sampling along the blend space.
        param_index: usize,
    },
}

/// A directional transition between two states.
#[derive(Debug, Clone, Copy)]
pub struct Transition {
    /// Source state index.
    pub from: usize,
    /// Target state index.
    pub to: usize,
    /// Duration of crossfade blend in seconds.
    pub fade_duration: f32,
    /// Condition triggering the transition.
    pub rule: TransitionRule,
}

/// Internal transition blend state.
#[derive(Debug, Clone, Copy)]
struct ActiveTransition {
    from_state: usize,
    to_state: usize,
    fade_duration: f32,
    elapsed: f32,
}

/// Fixed-capacity, zero-allocation animation blending state machine.
///
/// * `S`: Maximum number of states.
/// * `T`: Maximum number of transitions.
/// * `P`: Maximum number of parameters.
pub struct AnimationStateMachine<'a, const S: usize, const T: usize, const P: usize> {
    states: [Option<StateNode<'a>>; S],
    transitions: [Option<Transition>; T],
    float_params: [f32; P],
    bool_params: [bool; P],
    current_state: usize,
    time: f32,
    active_transition: Option<ActiveTransition>,
}

impl<'a, const S: usize, const T: usize, const P: usize> AnimationStateMachine<'a, S, T, P> {
    /// Create a new state machine with an initial entry state index.
    pub fn new(initial_state: usize) -> Self {
        Self {
            states: [const { None }; S],
            transitions: [const { None }; T],
            float_params: [0.0; P],
            bool_params: [false; P],
            current_state: initial_state,
            time: 0.0,
            active_transition: None,
        }
    }

    /// Set the node for a given state index.
    pub fn set_state(&mut self, index: usize, node: StateNode<'a>) {
        if index < S {
            self.states[index] = Some(node);
        }
    }

    /// Add a transition between states.
    pub fn add_transition(&mut self, transition: Transition) -> bool {
        for slot in &mut self.transitions {
            if slot.is_none() {
                *slot = Some(transition);
                return true;
            }
        }
        false
    }

    /// Set a float parameter value.
    #[inline]
    pub fn set_param_float(&mut self, index: usize, value: f32) {
        if index < P {
            self.float_params[index] = value;
        }
    }

    /// Get a float parameter value.
    #[inline]
    pub fn get_param_float(&self, index: usize) -> f32 {
        if index < P {
            self.float_params[index]
        } else {
            0.0
        }
    }

    /// Set a boolean parameter value.
    #[inline]
    pub fn set_param_bool(&mut self, index: usize, value: bool) {
        if index < P {
            self.bool_params[index] = value;
        }
    }

    /// Get a boolean parameter value.
    #[inline]
    pub fn get_param_bool(&self, index: usize) -> bool {
        if index < P {
            self.bool_params[index]
        } else {
            false
        }
    }

    /// Current active state index.
    #[inline]
    pub fn current_state(&self) -> usize {
        self.current_state
    }

    /// Check if a transition is currently in progress.
    #[inline]
    pub fn is_transitioning(&self) -> bool {
        self.active_transition.is_some()
    }

    /// Advance the state machine time by `dt` seconds and evaluate state transitions.
    pub fn update(&mut self, dt: f32) {
        self.time += dt;

        // Only evaluate new transitions if not currently in a transition
        if self.active_transition.is_none() {
            for trans_opt in &self.transitions {
                let Some(trans) = trans_opt else { continue };
                if trans.from != self.current_state {
                    continue;
                }

                let triggered = match trans.rule {
                    TransitionRule::Immediate => true,
                    TransitionRule::ParamGreaterThan(p, threshold) => {
                        self.get_param_float(p) > threshold
                    }
                    TransitionRule::ParamLessThan(p, threshold) => {
                        self.get_param_float(p) < threshold
                    }
                    TransitionRule::ParamBool(p, expected) => self.get_param_bool(p) == expected,
                };

                if triggered {
                    if trans.fade_duration <= 1e-4 {
                        self.current_state = trans.to;
                    } else {
                        self.active_transition = Some(ActiveTransition {
                            from_state: trans.from,
                            to_state: trans.to,
                            fade_duration: trans.fade_duration,
                            elapsed: 0.0,
                        });
                    }
                    break;
                }
            }
        }

        // Progress active transition if one is ongoing
        if let Some(mut trans) = self.active_transition {
            trans.elapsed += dt;
            if trans.elapsed >= trans.fade_duration {
                self.current_state = trans.to_state;
                self.active_transition = None;
            } else {
                self.active_transition = Some(trans);
            }
        }
    }

    /// Sample the currently active pose into `out_poses` for skeletal joints.
    pub fn sample_poses(&self, out_poses: &mut [BonePose]) {
        if let Some(trans) = self.active_transition {
            // Blending between from_state and to_state
            let alpha = (trans.elapsed / trans.fade_duration).clamp(0.0, 1.0);

            let mut from_poses = [BonePose::identity(); 32];
            let mut to_poses = [BonePose::identity(); 32];

            let count = out_poses.len().min(32);
            self.sample_state(trans.from_state, &mut from_poses[..count]);
            self.sample_state(trans.to_state, &mut to_poses[..count]);

            for i in 0..count {
                out_poses[i] = BonePose::blend(from_poses[i], to_poses[i], alpha);
            }
        } else {
            self.sample_state(self.current_state, out_poses);
        }
    }

    fn sample_state(&self, state_idx: usize, out_poses: &mut [BonePose]) {
        let Some(Some(node)) = self.states.get(state_idx) else {
            return;
        };

        match node {
            StateNode::SingleClip(clip) => {
                for (bone_i, out) in out_poses.iter_mut().enumerate() {
                    if let Some(pose) = clip.sample_bone(self.time, bone_i) {
                        *out = pose;
                    }
                }
            }
            StateNode::Blend1D {
                clip_a,
                clip_b,
                param_index,
                min_val,
                max_val,
            } => {
                let p = self.get_param_float(*param_index);
                let alpha = if (max_val - min_val).abs() > 1e-6 {
                    ((p - min_val) / (max_val - min_val)).clamp(0.0, 1.0)
                } else {
                    0.0
                };

                for (bone_i, out) in out_poses.iter_mut().enumerate() {
                    let p_a = clip_a
                        .sample_bone(self.time, bone_i)
                        .unwrap_or_else(BonePose::identity);
                    let p_b = clip_b
                        .sample_bone(self.time, bone_i)
                        .unwrap_or_else(BonePose::identity);
                    *out = BonePose::blend(p_a, p_b, alpha);
                }
            }
            StateNode::AdditiveClip {
                base_clip,
                additive_clip,
                param_index,
            } => {
                let weight = self.get_param_float(*param_index).clamp(0.0, 1.0);
                for (bone_i, out) in out_poses.iter_mut().enumerate() {
                    let base_p = base_clip
                        .sample_bone(self.time, bone_i)
                        .unwrap_or_else(BonePose::identity);
                    if weight <= 0.0 {
                        *out = base_p;
                    } else {
                        let add_p = additive_clip
                            .sample_bone(self.time, bone_i)
                            .unwrap_or_else(BonePose::identity);
                        *out = BonePose::blend(base_p, add_p, weight);
                    }
                }
            }
            StateNode::BlendSpace1D {
                points,
                param_index,
            } => {
                if points.is_empty() {
                    return;
                }
                let p = self.get_param_float(*param_index);
                if points.len() == 1 || p <= points[0].0 {
                    let clip = points[0].1;
                    for (bone_i, out) in out_poses.iter_mut().enumerate() {
                        if let Some(pose) = clip.sample_bone(self.time, bone_i) {
                            *out = pose;
                        }
                    }
                } else if p >= points[points.len() - 1].0 {
                    let clip = points[points.len() - 1].1;
                    for (bone_i, out) in out_poses.iter_mut().enumerate() {
                        if let Some(pose) = clip.sample_bone(self.time, bone_i) {
                            *out = pose;
                        }
                    }
                } else {
                    let mut idx = 0;
                    while idx + 1 < points.len() && points[idx + 1].0 < p {
                        idx += 1;
                    }
                    let (val_a, clip_a) = points[idx];
                    let (val_b, clip_b) = points[idx + 1];
                    let alpha = if (val_b - val_a).abs() > 1e-6 {
                        ((p - val_a) / (val_b - val_a)).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    for (bone_i, out) in out_poses.iter_mut().enumerate() {
                        let p_a = clip_a
                            .sample_bone(self.time, bone_i)
                            .unwrap_or_else(BonePose::identity);
                        let p_b = clip_b
                            .sample_bone(self.time, bone_i)
                            .unwrap_or_else(BonePose::identity);
                        *out = BonePose::blend(p_a, p_b, alpha);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static CLIP_A: AnimClip<'static> = AnimClip::new(&[], true);
    static CLIP_B: AnimClip<'static> = AnimClip::new(&[], true);

    #[test]
    fn test_absm_transitions() {
        let mut sm: AnimationStateMachine<2, 2, 1> = AnimationStateMachine::new(0);
        sm.set_state(0, StateNode::SingleClip(&CLIP_A));
        sm.set_state(1, StateNode::SingleClip(&CLIP_B));

        sm.add_transition(Transition {
            from: 0,
            to: 1,
            fade_duration: 0.5,
            rule: TransitionRule::ParamGreaterThan(0, 1.0),
        });

        assert_eq!(sm.current_state(), 0);
        assert!(!sm.is_transitioning());

        // Condition not met
        sm.set_param_float(0, 0.5);
        sm.update(0.1);
        assert_eq!(sm.current_state(), 0);
        assert!(!sm.is_transitioning());

        // Condition met -> starts transition
        sm.set_param_float(0, 2.0);
        sm.update(0.1);
        assert!(sm.is_transitioning());

        // Finish transition duration (0.5s)
        sm.update(0.45);
        assert!(!sm.is_transitioning());
        assert_eq!(sm.current_state(), 1);
    }

    #[test]
    fn test_absm_additive_and_blend_space() {
        let mut sm: AnimationStateMachine<2, 1, 2> = AnimationStateMachine::new(0);
        sm.set_state(
            0,
            StateNode::AdditiveClip {
                base_clip: &CLIP_A,
                additive_clip: &CLIP_B,
                param_index: 0,
            },
        );

        static POINTS: [(f32, &AnimClip<'static>); 2] = [(0.0, &CLIP_A), (10.0, &CLIP_B)];
        sm.set_state(
            1,
            StateNode::BlendSpace1D {
                points: &POINTS,
                param_index: 1,
            },
        );

        let mut poses = [BonePose::identity(); 2];
        sm.sample_poses(&mut poses);
        assert_eq!(poses.len(), 2);
    }
}
