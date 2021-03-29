use std::mem;

use ash::vk;
use ultraviolet::vec::*;

use super::vertex::VertexDesc;

#[repr(C)]
pub struct CommonVertex {
    position: Vec3,
    color: Vec4,
    uv: Vec2,
}

impl CommonVertex {
    pub fn new(position: Vec3, color: Vec4, uv: Vec2) -> Self {
        Self {
            position,
            color,
            uv,
        }
    }
}

const ATTRIBUTE_DESCRIPTIONS: &'static [vk::VertexInputAttributeDescription] = &[
    vk::VertexInputAttributeDescription {
        binding: 0,
        location: 0,
        format: vk::Format::R32G32B32_SFLOAT,
        offset: 0,
    },
    vk::VertexInputAttributeDescription {
        binding: 0,
        location: 1,
        format: vk::Format::R32G32B32A32_SFLOAT,
        offset: mem::size_of::<Vec3>() as u32,
    },
    vk::VertexInputAttributeDescription {
        binding: 0,
        location: 2,
        format: vk::Format::R32G32_SFLOAT,
        offset: mem::size_of::<Vec4>() as u32 + mem::size_of::<Vec3>() as u32,
    },
];

impl VertexDesc for CommonVertex {
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
