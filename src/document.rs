use super::resources::*;
use super::Mesh;
use ultraviolet::*;

#[derive(Debug, Clone)]
pub struct Node {
    /// The name of this node.
    name: String,
    /// The mesh index references by this node.
    mesh: Option<usize>,
    position: Vec3,
    rotation: Rotor3,
    scale: Vec3,
}

pub struct Document {
    meshes: Vec<Handle<Mesh>>,
    nodes: Vec<Node>,
}

impl Document {
    pub fn from_gltf(document: gltf::Document, meshes: Vec<Handle<Mesh>>) -> Self {
        let nodes = document
            .nodes()
            .map(|node| {
                let (position, rotation, scale) = node.transform().decomposed();
                Node {
                    name: node.name().unwrap_or_default().to_owned(),
                    mesh: node.mesh().map(|mesh| mesh.index()),
                    position: Vec3::from(position),
                    rotation: Rotor3::from_quaternion_array(rotation),
                    scale: Vec3::from(scale),
                }
            })
            .collect();

        Self { nodes, meshes }
    }

    /// Returns a handle to the mesh at index.
    pub fn mesh(&self, index: usize) -> Handle<Mesh> {
        self.meshes[index]
    }

    /// Returns a reference to the node at index.
    pub fn node(&self, index: usize) -> &Node {
        &self.nodes[index]
    }

    /// Searches for the node with name.
    pub fn find_node<S>(&self, name: S) -> Option<&Node>
    where
        S: AsRef<str>,
    {
        let name = name.as_ref();
        self.nodes.iter().find(|node| node.name == name)
    }
}
