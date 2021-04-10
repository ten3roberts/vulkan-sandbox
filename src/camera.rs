use ultraviolet::projection;
use ultraviolet::vec::*;
use ultraviolet::Mat4;

pub struct Camera {
    pub position: Vec3,
    projection: Mat4,
}

impl Camera {
    /// Creates a new perspective projection camera.
    pub fn perspective(position: Vec3, fov: f32, aspect_ratio: f32, near: f32, far: f32) -> Self {
        let projection = projection::perspective_vk(fov, aspect_ratio, near, far);
        Self {
            position,
            projection,
        }
    }

    /// Creates a new orthographic projection camera.
    pub fn orthographic(position: Vec3, width: f32, height: f32, near: f32, far: f32) -> Self {
        let hw = width / 2.0;
        let hh = height / 2.0;
        let projection = projection::orthographic_vk(-hw, hw, -hh, hh, near, far);
        Self {
            position,
            projection,
        }
    }

    /// Return the camera's projection matrix.
    pub fn projection(&self) -> Mat4 {
        self.projection
    }

    /// Calculates the cameras view matrix
    pub fn calculate_view(&self) -> Mat4 {
        Mat4::from_translation(self.position).inversed()
    }
}
