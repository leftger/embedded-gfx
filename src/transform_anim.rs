//! Keyframed rigid-body transform animation (position, rotation, scale).
//!
//! Designed for boot logos and menu transitions. Store keyframes in flash as `const` data
//! and drive meshes with [`AnimationPlayer`].
//!
//! Keyframes store Euler angles for `const` friendliness; sampling converts to
//! quaternions and slerps to avoid gimbal artifacts.

use crate::mesh::K3dMesh;
use crate::tween::lerp;
use nalgebra::{Point3, UnitQuaternion};

/// One keyframe of object transform data.
#[derive(Debug, Clone, Copy)]
pub struct TransformKeyframe {
    pub time: f32,
    pub position: [f32; 3],
    /// Euler angles in radians: roll (X), pitch (Y), yaw (Z).
    pub roll: f32,
    pub pitch: f32,
    pub yaw: f32,
    pub scale: f32,
}

impl TransformKeyframe {
    pub const fn new(
        time: f32,
        position: [f32; 3],
        roll: f32,
        pitch: f32,
        yaw: f32,
        scale: f32,
    ) -> Self {
        Self {
            time,
            position,
            roll,
            pitch,
            yaw,
            scale,
        }
    }

    #[inline]
    pub fn quat(&self) -> UnitQuaternion<f32> {
        UnitQuaternion::from_euler_angles(self.roll, self.pitch, self.yaw)
    }
}

/// Sampled transform at a point in time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SampledTransform {
    pub position: [f32; 3],
    pub roll: f32,
    pub pitch: f32,
    pub yaw: f32,
    pub scale: f32,
}

impl SampledTransform {
    /// Apply this transform to a mesh (quaternion path for rotation).
    pub fn apply_to(&self, mesh: &mut K3dMesh<'_>) {
        mesh.set_position(self.position[0], self.position[1], self.position[2]);
        mesh.set_rotation(UnitQuaternion::from_euler_angles(
            self.roll, self.pitch, self.yaw,
        ));
        mesh.set_scale(self.scale);
    }

    /// Apply position only to a camera.
    pub fn apply_position_to_camera(&self, camera: &mut crate::camera::Camera) {
        camera.set_position(Point3::new(
            self.position[0],
            self.position[1],
            self.position[2],
        ));
    }
}

/// Keyframed transform track (heapless — keyframes live in `const` or static storage).
#[derive(Debug)]
pub struct TransformTrack<'a> {
    keyframes: &'a [TransformKeyframe],
    looping: bool,
}

impl<'a> TransformTrack<'a> {
    pub fn new(keyframes: &'a [TransformKeyframe], looping: bool) -> Self {
        assert!(
            !keyframes.is_empty(),
            "TransformTrack requires at least one keyframe"
        );
        Self { keyframes, looping }
    }

    pub fn duration(&self) -> f32 {
        self.keyframes.last().map(|k| k.time).unwrap_or(0.0)
    }

    pub fn keyframe_count(&self) -> usize {
        self.keyframes.len()
    }

    pub fn is_looping(&self) -> bool {
        self.looping
    }

    /// Sample the track at `time` (seconds).
    pub fn sample(&self, time: f32) -> SampledTransform {
        if self.keyframes.len() == 1 {
            return keyframe_to_sampled(self.keyframes[0]);
        }

        let duration = self.duration();
        let t = if self.looping {
            if duration > 0.0 { time % duration } else { 0.0 }
        } else {
            time.clamp(0.0, duration)
        };

        let mut kf1_idx = self
            .keyframes
            .partition_point(|kf| kf.time <= t)
            .saturating_sub(1);
        if self.keyframes[kf1_idx].time > t {
            kf1_idx = 0;
        }

        if kf1_idx == self.keyframes.len() - 1 {
            return keyframe_to_sampled(self.keyframes[kf1_idx]);
        }
        let kf2_idx = kf1_idx + 1;

        let kf1 = self.keyframes[kf1_idx];
        let kf2 = self.keyframes[kf2_idx];

        let alpha = if kf2.time > kf1.time {
            (t - kf1.time) / (kf2.time - kf1.time)
        } else {
            0.0
        };

        // Slerp in quaternion space when `anim-blend` is enabled; otherwise Euler lerp.
        #[cfg(feature = "anim-blend")]
        let (roll, pitch, yaw) = {
            let q = kf1.quat().slerp(&kf2.quat(), alpha);
            q.euler_angles()
        };
        #[cfg(not(feature = "anim-blend"))]
        let (roll, pitch, yaw) = (
            lerp(kf1.roll, kf2.roll, alpha),
            lerp(kf1.pitch, kf2.pitch, alpha),
            lerp(kf1.yaw, kf2.yaw, alpha),
        );

        SampledTransform {
            position: [
                lerp(kf1.position[0], kf2.position[0], alpha),
                lerp(kf1.position[1], kf2.position[1], alpha),
                lerp(kf1.position[2], kf2.position[2], alpha),
            ],
            roll,
            pitch,
            yaw,
            scale: lerp(kf1.scale, kf2.scale, alpha),
        }
    }
}

fn keyframe_to_sampled(kf: TransformKeyframe) -> SampledTransform {
    SampledTransform {
        position: kf.position,
        roll: kf.roll,
        pitch: kf.pitch,
        yaw: kf.yaw,
        scale: kf.scale,
    }
}

/// Playback state for a [`TransformTrack`].
#[derive(Debug)]
pub struct AnimationPlayer<'a> {
    track: TransformTrack<'a>,
    time: f32,
    playing: bool,
    speed: f32,
}

impl<'a> AnimationPlayer<'a> {
    pub fn new(track: TransformTrack<'a>) -> Self {
        Self {
            track,
            time: 0.0,
            playing: true,
            speed: 1.0,
        }
    }

    pub fn with_speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }

    pub fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn reset(&mut self) {
        self.time = 0.0;
    }

    pub fn set_time(&mut self, time: f32) {
        self.time = time;
    }

    pub fn time(&self) -> f32 {
        self.time
    }

    pub fn track(&self) -> &TransformTrack<'a> {
        &self.track
    }

    pub fn advance(&mut self, dt: f32) {
        if self.playing && dt > 0.0 {
            self.time += dt * self.speed;
        }
    }

    pub fn sample(&self) -> SampledTransform {
        self.track.sample(self.time)
    }

    pub fn apply_to(&self, mesh: &mut K3dMesh<'_>) {
        self.sample().apply_to(mesh);
    }

    /// `true` when non-looping track has passed the last keyframe time.
    pub fn is_done(&self) -> bool {
        !self.track.is_looping() && self.time >= self.track.duration()
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    const TRACK: &[TransformKeyframe] = &[
        TransformKeyframe::new(0.0, [0.0, -2.0, 0.0], 0.0, 0.0, 0.0, 1.0),
        TransformKeyframe::new(1.0, [0.0, 0.0, 0.0], 0.0, 0.0, 3.14, 1.0),
    ];

    #[test]
    fn test_track_interpolation() {
        let track = TransformTrack::new(TRACK, false);
        let mid = track.sample(0.5);
        assert!((mid.position[1] - (-1.0)).abs() < 1e-5);
        assert!((mid.yaw - 1.57).abs() < 0.15);
    }

    #[test]
    fn test_player_done() {
        let track = TransformTrack::new(TRACK, false);
        let mut player = AnimationPlayer::new(track);
        player.advance(1.5);
        assert!(player.is_done());
    }

    #[test]
    fn test_looping_track() {
        const LOOP_TRACK: &[TransformKeyframe] = &[
            TransformKeyframe::new(0.0, [0.0, 0.0, 0.0], 0.0, 0.0, 0.0, 1.0),
            TransformKeyframe::new(1.0, [1.0, 0.0, 0.0], 0.0, 0.0, 0.0, 1.0),
        ];
        let track = TransformTrack::new(LOOP_TRACK, true);
        let s = track.sample(1.5);
        assert!((s.position[0] - 0.5).abs() < 1e-5);
    }
}
