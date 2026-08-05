//! Vertex Animation System
//!
//! Provides keyframe-based vertex animation through linear interpolation.
//! Perfect for animated characters, waving flags, pulsing objects, etc.

/// A single keyframe containing vertex positions
#[derive(Debug, Clone, Copy)]
pub struct Keyframe<'a> {
    pub vertices: &'a [[f32; 3]],
    pub time: f32,
}

/// Vertex animation with multiple keyframes
#[derive(Debug)]
pub struct VertexAnimation<'a> {
    keyframes: &'a [Keyframe<'a>],
    looping: bool,
}

impl<'a> VertexAnimation<'a> {
    /// Create a new vertex animation from keyframes
    ///
    /// # Arguments
    /// * `keyframes` - Array of keyframes with vertex positions and timestamps
    /// * `looping` - Whether the animation should loop
    ///
    /// # Panics
    /// Panics if keyframes array is empty or if keyframes have inconsistent vertex counts
    pub fn new(keyframes: &'a [Keyframe<'a>], looping: bool) -> Self {
        assert!(!keyframes.is_empty(), "Keyframes array cannot be empty");

        // Verify all keyframes have the same vertex count
        let vertex_count = keyframes[0].vertices.len();
        for kf in keyframes.iter() {
            assert_eq!(
                kf.vertices.len(),
                vertex_count,
                "All keyframes must have the same number of vertices"
            );
        }

        Self { keyframes, looping }
    }

    /// Sample the animation at a given time
    ///
    /// Returns interpolated vertex positions for the given time.
    /// Uses a temporary buffer to store interpolated vertices.
    ///
    /// # Arguments
    /// * `time` - Current animation time
    /// * `output` - Output buffer for interpolated vertices (must match keyframe vertex count)
    pub fn sample(&self, time: f32, output: &mut [[f32; 3]]) {
        assert_eq!(
            output.len(),
            self.keyframes[0].vertices.len(),
            "Output buffer size must match keyframe vertex count"
        );

        // Handle edge cases
        if self.keyframes.len() == 1 {
            output.copy_from_slice(self.keyframes[0].vertices);
            return;
        }

        // Get animation duration
        let duration = self.keyframes.last().unwrap().time;

        // Handle looping
        let t = if self.looping {
            if duration > 0.0 { time % duration } else { 0.0 }
        } else {
            time.clamp(0.0, duration)
        };

        // Find the two keyframes to interpolate between (binary search).
        let mut kf1_idx = self
            .keyframes
            .partition_point(|kf| kf.time <= t)
            .saturating_sub(1);
        if self.keyframes[kf1_idx].time > t {
            kf1_idx = 0;
        }
        let kf2_idx = (kf1_idx + 1).min(self.keyframes.len() - 1);

        // If we're at or past the last keyframe
        if kf1_idx == self.keyframes.len() - 1 {
            output.copy_from_slice(self.keyframes[kf1_idx].vertices);
            return;
        }

        let kf1 = &self.keyframes[kf1_idx];
        let kf2 = &self.keyframes[kf2_idx];

        // Calculate interpolation factor
        let alpha = if kf2.time > kf1.time {
            (t - kf1.time) / (kf2.time - kf1.time)
        } else {
            0.0
        };

        // Interpolate vertices
        for (i, out_vertex) in output.iter_mut().enumerate() {
            let v1 = &kf1.vertices[i];
            let v2 = &kf2.vertices[i];

            out_vertex[0] = v1[0] + alpha * (v2[0] - v1[0]);
            out_vertex[1] = v1[1] + alpha * (v2[1] - v1[1]);
            out_vertex[2] = v1[2] + alpha * (v2[2] - v1[2]);
        }
    }

    /// Get the total duration of the animation
    pub fn duration(&self) -> f32 {
        self.keyframes.last().map(|kf| kf.time).unwrap_or(0.0)
    }

    /// Get the number of keyframes
    pub fn keyframe_count(&self) -> usize {
        self.keyframes.len()
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    #[test]
    fn test_single_keyframe() {
        let vertices = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
        let kf = Keyframe {
            vertices: &vertices,
            time: 0.0,
        };
        let keyframes = [kf];
        let anim = VertexAnimation::new(&keyframes, false);

        let mut output = [[0.0, 0.0, 0.0]; 2];
        anim.sample(0.5, &mut output);

        assert_eq!(output[0], [0.0, 0.0, 0.0]);
        assert_eq!(output[1], [1.0, 0.0, 0.0]);
    }

    #[test]
    fn test_two_keyframe_interpolation() {
        let vertices1 = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
        let vertices2 = [[0.0, 2.0, 0.0], [1.0, 2.0, 0.0]];

        let kf1 = Keyframe {
            vertices: &vertices1,
            time: 0.0,
        };
        let kf2 = Keyframe {
            vertices: &vertices2,
            time: 1.0,
        };

        let keyframes = [kf1, kf2];
        let anim = VertexAnimation::new(&keyframes, false);

        // Sample at halfway point
        let mut output = [[0.0, 0.0, 0.0]; 2];
        anim.sample(0.5, &mut output);

        assert_eq!(output[0], [0.0, 1.0, 0.0]);
        assert_eq!(output[1], [1.0, 1.0, 0.0]);
    }

    #[test]
    fn test_looping_animation() {
        let vertices1 = [[0.0, 0.0, 0.0]];
        let vertices2 = [[1.0, 0.0, 0.0]];

        let kf1 = Keyframe {
            vertices: &vertices1,
            time: 0.0,
        };
        let kf2 = Keyframe {
            vertices: &vertices2,
            time: 1.0,
        };

        let keyframes = [kf1, kf2];
        let anim = VertexAnimation::new(&keyframes, true);

        // Sample past the end - should loop
        let mut output = [[0.0, 0.0, 0.0]; 1];
        anim.sample(1.5, &mut output);

        // Should be halfway between keyframes (0.5 after wrapping)
        assert_eq!(output[0], [0.5, 0.0, 0.0]);
    }

    #[test]
    fn test_clamping_non_looping() {
        let vertices1 = [[0.0, 0.0, 0.0]];
        let vertices2 = [[1.0, 0.0, 0.0]];

        let kf1 = Keyframe {
            vertices: &vertices1,
            time: 0.0,
        };
        let kf2 = Keyframe {
            vertices: &vertices2,
            time: 1.0,
        };

        let keyframes = [kf1, kf2];
        let anim = VertexAnimation::new(&keyframes, false);

        // Sample past the end - should clamp to last keyframe
        let mut output = [[0.0, 0.0, 0.0]; 1];
        anim.sample(2.0, &mut output);

        assert_eq!(output[0], [1.0, 0.0, 0.0]);
    }

    #[test]
    fn test_duration() {
        let vertices1 = [[0.0, 0.0, 0.0]];
        let vertices2 = [[1.0, 0.0, 0.0]];

        let kf1 = Keyframe {
            vertices: &vertices1,
            time: 0.0,
        };
        let kf2 = Keyframe {
            vertices: &vertices2,
            time: 2.5,
        };

        let keyframes = [kf1, kf2];
        let anim = VertexAnimation::new(&keyframes, false);
        assert_eq!(anim.duration(), 2.5);
        assert_eq!(anim.keyframe_count(), 2);
    }

    #[test]
    #[should_panic(expected = "Keyframes array cannot be empty")]
    fn test_empty_keyframes_panics() {
        let keyframes: &[Keyframe] = &[];
        let _anim = VertexAnimation::new(keyframes, false);
    }

    #[test]
    #[should_panic(expected = "All keyframes must have the same number of vertices")]
    fn test_inconsistent_vertex_count_panics() {
        let vertices1 = [[0.0, 0.0, 0.0]];
        let vertices2 = [[1.0, 0.0, 0.0], [2.0, 0.0, 0.0]];

        let kf1 = Keyframe {
            vertices: &vertices1,
            time: 0.0,
        };
        let kf2 = Keyframe {
            vertices: &vertices2,
            time: 1.0,
        };

        let keyframes = [kf1, kf2];
        let _anim = VertexAnimation::new(&keyframes, false);
    }
}
