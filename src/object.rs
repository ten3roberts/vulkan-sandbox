use std::rc::Rc;

use ultraviolet::Vec3;

use crate::{material::Material, mesh::Mesh, resources::Handle};

/// Represents an object that can be rendered.
pub struct Object {
    pub material: Handle<Material>,
    pub mesh: Handle<Mesh>,
    pub position: Vec3,
}
