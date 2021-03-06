use ash::vk;
use gltf::{buffer, Semantic};
use std::iter::repeat;
use std::mem;
use std::rc::Rc;
use ultraviolet::{Vec2, Vec3};

use crate::vulkan::{self, VulkanContext};
use crate::Error;
use vulkan::{Buffer, BufferType, BufferUsage};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    position: Vec3,
    normal: Vec3,
    texcoord: Vec2,
}

impl Vertex {
    pub fn new(position: Vec3, normal: Vec3, texcoord: Vec2) -> Self {
        Self {
            position,
            normal,
            texcoord,
        }
    }
}

const ATTRIBUTE_DESCRIPTIONS: &'static [vk::VertexInputAttributeDescription] = &[
    // vec3 3*4 bytes
    vk::VertexInputAttributeDescription {
        binding: 0,
        location: 0,
        format: vk::Format::R32G32B32_SFLOAT,
        offset: 0,
    },
    // vec3 3*4 bytes
    vk::VertexInputAttributeDescription {
        binding: 0,
        location: 1,
        format: vk::Format::R32G32B32_SFLOAT,
        offset: 12,
    },
    // vec2 2*4 bytes
    vk::VertexInputAttributeDescription {
        binding: 0,
        location: 2,
        format: vk::Format::R32G32_SFLOAT,
        offset: 12 + 12,
    },
];

impl vulkan::VertexDesc for Vertex {
    fn binding_description() -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription {
            binding: 0,
            stride: mem::size_of::<Self>() as u32,
            input_rate: vk::VertexInputRate::VERTEX,
        }
    }

    fn attribute_descriptions() -> &'static [vk::VertexInputAttributeDescription] {
        ATTRIBUTE_DESCRIPTIONS
    }
}

pub struct Mesh {
    vertex_buffer: Buffer,
    index_buffer: Buffer,
    vertex_count: u32,
    index_count: u32,
}

impl Mesh {
    pub fn new(
        context: Rc<VulkanContext>,
        vertices: &[Vertex],
        indices: &[u32],
    ) -> Result<Self, Error> {
        let vertex_buffer = Buffer::new(
            context.clone(),
            BufferType::Vertex,
            BufferUsage::Staged,
            vertices,
        )?;

        let index_buffer =
            Buffer::new(context, BufferType::Index32, BufferUsage::Staged, indices)?;

        Ok(Self {
            vertex_buffer,
            index_buffer,
            vertex_count: vertices.len() as u32,
            index_count: indices.len() as u32,
        })
    }

    /// Creates a mesh from an structure-of-arrays vertex data
    /// Each index refers to the direct index of positions, normals and texcoords
    pub fn from_soa(
        context: Rc<VulkanContext>,
        positions: &[Vec3],
        normals: &[Vec3],
        texcoords: &[Vec2],
        indices: &[u32],
    ) -> Result<Self, Error> {
        let mut vertices = Vec::with_capacity(positions.len());

        for i in 0..positions.len() {
            vertices.push(Vertex::new(positions[i], normals[i], texcoords[i]));
        }

        Self::new(context, &vertices, &indices)
    }

    pub fn from_gltf(
        context: Rc<VulkanContext>,
        mesh: gltf::Mesh,
        buffers: &[buffer::Data],
    ) -> Result<Self, Error> {
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut texcoords = Vec::new();
        let mut raw_indices = Vec::new();

        if let Some(primitive) = mesh.primitives().next() {
            let indices_accessor = primitive.indices().ok_or(Error::SparseAccessor)?;
            let indices_view = indices_accessor.view().ok_or(Error::SparseAccessor)?;

            raw_indices = match indices_accessor.size() {
                2 => load_u16_as_u32(&indices_view, buffers),
                4 => load_u32(&indices_view, buffers),
                _ => unreachable!(),
            };

            for (semantic, accessor) in primitive.attributes() {
                let view = accessor.view().ok_or(Error::SparseAccessor)?;
                match semantic {
                    Semantic::Positions => positions = load_vec3(&view, buffers),
                    Semantic::Normals => normals = load_vec3(&view, buffers),
                    Semantic::TexCoords(_) => texcoords = load_vec2(&view, buffers),
                    Semantic::Tangents => {}
                    Semantic::Colors(_) => {}
                    Semantic::Joints(_) => {}
                    Semantic::Weights(_) => {}
                };
            }
        }

        // Pad incase these weren't included in geometry
        pad_vec(&mut normals, Vec3::unit_z(), positions.len());
        pad_vec(&mut texcoords, Vec2::zero(), positions.len());

        Self::from_soa(context, &positions, &normals, &texcoords, &raw_indices)
    }

    // Returns the internal vertex buffer
    pub fn vertex_buffer(&self) -> &Buffer {
        &self.vertex_buffer
    }

    // Returns the internal index buffer
    pub fn index_buffer(&self) -> &Buffer {
        &self.index_buffer
    }

    // Returns the number of vertices
    pub fn vertex_count(&self) -> u32 {
        self.vertex_count
    }

    // Returns the number of indices
    pub fn index_count(&self) -> u32 {
        self.index_count
    }
}

// Pads a vector with copies of val to ensure it is atleast `len` elements
fn pad_vec<T: Copy>(vec: &mut Vec<T>, val: T, len: usize) {
    vec.extend(repeat(val).take(len - vec.len()))
}

fn load_u16_as_u32(view: &buffer::View, buffers: &[buffer::Data]) -> Vec<u32> {
    let buffer = &buffers[view.buffer().index()];

    let raw_data = &buffer[view.offset()..view.offset() + view.length()];
    raw_data
        .chunks_exact(2)
        .map(|val| u16::from_le_bytes([val[0], val[1]]) as u32)
        .collect()
}

fn load_u32(view: &buffer::View, buffers: &[buffer::Data]) -> Vec<u32> {
    let buffer = &buffers[view.buffer().index()];

    let raw_data = &buffer[view.offset()..view.offset() + view.length()];
    raw_data
        .chunks_exact(4)
        .map(|val| u32::from_le_bytes([val[0], val[1], val[2], val[3]]))
        .collect()
}

fn load_vec2(view: &buffer::View, buffers: &[buffer::Data]) -> Vec<Vec2> {
    let buffer = &buffers[view.buffer().index()];

    let raw_data = &buffer[view.offset()..view.offset() + view.length()];
    raw_data
        .chunks_exact(8)
        .map(|val| {
            Vec2::new(
                f32::from_le_bytes([val[0], val[1], val[2], val[3]]),
                f32::from_le_bytes([val[4], val[5], val[6], val[7]]),
            )
        })
        .collect()
}

fn load_vec3(view: &buffer::View, buffers: &[buffer::Data]) -> Vec<Vec3> {
    let buffer = &buffers[view.buffer().index()];

    let raw_data = &buffer[view.offset()..view.offset() + view.length()];
    raw_data
        .chunks_exact(12)
        .map(|val| {
            Vec3::new(
                f32::from_le_bytes([val[0], val[1], val[2], val[3]]),
                f32::from_le_bytes([val[4], val[5], val[6], val[7]]),
                f32::from_le_bytes([val[8], val[9], val[10], val[11]]),
            )
        })
        .collect()
}
