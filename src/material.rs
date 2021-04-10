use std::{fs::File, path::PathBuf, rc::Rc};

use super::vulkan;
use crate::mesh;
use ash::vk;
use vulkan::pipeline::*;
use vulkan::sampler::*;
use vulkan::texture::*;
use vulkan::Error;
use vulkan::VertexDesc;
use vulkan::VulkanContext;
use vulkan::{descriptors::*, Extent, RenderPass};

pub struct MaterialInfo {
    pub vertexshader: PathBuf,
    pub fragmentshader: PathBuf,
    pub albedo: PathBuf,
}

pub struct Material {
    albedo: Texture,
    pipeline: Pipeline,
    pipeline_layout: PipelineLayout,
    sampler: Sampler,
    set: DescriptorSet,
    set_layout: DescriptorSetLayout,
}

impl Material {
    /// Creates a new material by loading shaders and textures from filesystem.
    /// `extent` refers to the renderpass and pipeline extent.
    pub fn new(
        context: Rc<VulkanContext>,
        layout_cache: &mut DescriptorLayoutCache,
        descriptor_allocator: &mut DescriptorAllocator,
        info: MaterialInfo,
        extent: Extent,
        renderpass: &RenderPass,
        object_set: DescriptorSetLayout,
    ) -> Result<Self, Error> {
        let albedo = Texture::load(context.clone(), info.albedo)?;

        let sampler_info = SamplerInfo {
            address_mode: AddressMode::REPEAT,
            mag_filter: FilterMode::LINEAR,
            min_filter: FilterMode::LINEAR,
            unnormalized_coordinates: false,
            anisotropy: 16.0,
            mip_levels: albedo.mip_levels(),
        };

        let sampler = Sampler::new(context.clone(), sampler_info)?;

        let mut set = Default::default();
        let mut set_layout = Default::default();

        DescriptorBuilder::new()
            .bind_combined_image_sampler(0, vk::ShaderStageFlags::FRAGMENT, &albedo, &sampler)
            .build(
                context.device(),
                layout_cache,
                descriptor_allocator,
                &mut set,
            )?
            .layout(layout_cache, &mut set_layout)?;

        let vertexshader = File::open(info.vertexshader)?;
        let fragmentshader = File::open(info.fragmentshader)?;

        // No global set 0 for now
        let pipeline_layout =
            PipelineLayout::new(context.device_ref(), &[set_layout, object_set])?;

        let pipeline = Pipeline::new(
            context.device_ref(),
            vertexshader,
            fragmentshader,
            extent,
            &pipeline_layout,
            renderpass,
            mesh::Vertex::binding_description(),
            mesh::Vertex::attribute_descriptions(),
            context.msaa_samples(),
        )?;

        Ok(Self {
            albedo,
            pipeline,
            pipeline_layout,
            sampler,
            set,
            set_layout,
        })
    }

    /// Returns a reference to the material pipeline.
    pub fn pipeline(&self) -> &Pipeline {
        &self.pipeline
    }

    pub fn pipeline_layout(&self) -> &PipelineLayout {
        &self.pipeline_layout
    }

    /// Returns the material descriptor set.
    pub fn set(&self) -> DescriptorSet {
        self.set
    }

    // Returns a reference the materials's set_layout.
    pub fn set_layout(&self) -> DescriptorSetLayout {
        self.set_layout
    }

    /// Returns a reference to the material albedo texture.
    pub fn albedo(&self) -> &Texture {
        &self.albedo
    }
}
