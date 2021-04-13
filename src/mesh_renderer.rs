use arrayvec::ArrayVec;
use std::{mem, ops::Range, rc::Rc};
use ultraviolet::*;

use ash::vk;
use vk::{DescriptorSet, DescriptorSetLayout};

use crate::{
    master_renderer::MasterRenderer, vulkan::descriptors::DescriptorBuilder, Camera, Scene,
};

use super::vulkan;
use super::Material;
use super::Mesh;
use vulkan::commands::*;
use vulkan::descriptors::*;
use vulkan::*;

pub const MAX_OBJECTS: usize = 8192;

#[derive(Default)]
#[repr(C)]
struct ObjectData {
    mvp: Mat4,
}

struct FrameData {
    set: DescriptorSet,
    set_layout: DescriptorSetLayout,
    object_buffer: Buffer,
}

impl FrameData {
    fn new(
        context: Rc<VulkanContext>,
        descriptor_layout_cache: &mut DescriptorLayoutCache,
        descriptor_allocator: &mut DescriptorAllocator,
    ) -> Result<Self, vulkan::Error> {
        let object_buffer = Buffer::new_uninit(
            context.clone(),
            BufferType::Storage,
            BufferUsage::MappedPersistent,
            mem::size_of::<ObjectData>() as u64 * MAX_OBJECTS as u64,
        )?;

        let mut set = Default::default();
        let mut set_layout = Default::default();

        DescriptorBuilder::new()
            .bind_storage_buffer(0, vk::ShaderStageFlags::VERTEX, &object_buffer)
            .build(
                context.device(),
                descriptor_layout_cache,
                descriptor_allocator,
                &mut set,
            )?
            .layout(descriptor_layout_cache, &mut set_layout)?;

        Ok(Self {
            object_buffer,
            set,
            set_layout,
        })
    }
}

struct Batch {
    material: Rc<Material>,
    mesh: Rc<Mesh>,
    range: Range<usize>,
}

struct RenderObject {
    material: Rc<Material>,
    mesh: Rc<Mesh>,
    model_matrix: Mat4,
}

pub struct MeshRenderer {
    context: Rc<VulkanContext>,
    frames: ArrayVec<[FrameData; swapchain::MAX_FRAMES]>,
}

impl MeshRenderer {
    pub fn new(
        context: Rc<VulkanContext>,
        descriptor_layout_cache: &mut DescriptorLayoutCache,
        descriptor_allocator: &mut DescriptorAllocator,
        image_count: usize,
    ) -> Result<Self, vulkan::Error> {
        let frames = (0..image_count)
            .map(|_| {
                FrameData::new(
                    context.clone(),
                    descriptor_layout_cache,
                    descriptor_allocator,
                )
            })
            .collect::<Result<_, _>>()?;

        Ok(Self { context, frames })
    }

    pub fn draw(
        &mut self,
        commandbuffer: &CommandBuffer,
        camera: &Camera,
        image_index: u32,
        scene: &Scene,
    ) -> Result<(), vulkan::Error> {
        let frame = &mut self.frames[image_index as usize];

        let view_projection = camera.projection() * camera.calculate_view();

        if scene.objects().len() > MAX_OBJECTS {
            log::error!("Scene objects exceed MAX_OBJECTS of {}", MAX_OBJECTS);
        }

        frame.object_buffer.write_slice(
            scene.objects().len().min(MAX_OBJECTS) as u64,
            0,
            |slice| {
                for (i, object) in scene.objects().iter().enumerate() {
                    let object_data = ObjectData {
                        mvp: view_projection
                            * Mat4::from_translation(object.position)
                            * Mat4::from_scale(0.1),
                    };

                    slice[i] = object_data;
                }
            },
        )?;

        for (i, object) in scene.objects().iter().enumerate() {
            let material = &object.material;
            let mesh = &object.mesh;
            commandbuffer.bind_pipeline(material.pipeline());
            commandbuffer.bind_descriptor_sets(
                material.pipeline_layout(),
                0,
                &[material.set(), frame.set],
            );

            commandbuffer.bind_vertexbuffers(0, &[&mesh.vertex_buffer()]);

            commandbuffer.bind_indexbuffer(&mesh.index_buffer(), 0);
            commandbuffer.draw_indexed(mesh.index_count(), 1, 0, 0, i as u32);
        }

        Ok(())
    }

    pub fn set_layout(&self) -> DescriptorSetLayout {
        self.frames[0].set_layout
    }
}
