use std::rc::Rc;

use ultraviolet::Vec3;

use crate::{material::Material, mesh::Mesh};

/// Represents an object that can be rendered.
pub struct Object {
    pub material: Rc<Material>,
    pub mesh: Rc<Mesh>,
    pub position: Vec3,
}
