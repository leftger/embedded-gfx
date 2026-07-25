//! Tests for embedded-dsp integration in embedded-3dgfx.

#[cfg(feature = "dsp")]
#[test]
fn test_dsp_skeleton_slerp() {
    use embedded_3dgfx::skeleton::Bone;
    use nalgebra::UnitQuaternion;

    let mut bone = Bone::new("arm");
    let target_rot = UnitQuaternion::from_euler_angles(0.0, core::f32::consts::PI / 2.0, 0.0);

    // Halfway SLERP using DSP quaternion helper
    bone.interpolate_rotation_dsp(target_rot, 0.5);

    let euler = bone.rotation.euler_angles();
    assert!((euler.1 - core::f32::consts::PI / 4.0).abs() < 0.1, "Halfway Y rot is ~45 deg");
}

#[cfg(feature = "dsp")]
#[test]
fn test_dsp_camera_smooth_track() {
    use embedded_3dgfx::camera::Camera;
    use nalgebra::Point3;

    let mut camera = Camera::new(1.0);
    camera.set_target(Point3::new(0.0, 0.0, 0.0));

    // Smooth track toward (10, 20, 30) with alpha = 0.5
    camera.smooth_track_dsp(Point3::new(10.0, 20.0, 30.0), 0.5);
}
