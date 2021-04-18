use std::{path::PathBuf, rc::Rc};

use super::MaterialEffect;
use crate::resources::Handle;
use crate::vulkan;
use ash::vk;
use vulkan::descriptors::*;
use vulkan::sampler::*;
use vulkan::texture::*;
use vulkan::Error;
use vulkan::VulkanContext;

pub struct MaterialInfo {
    pub albedo: PathBuf,
}

pub struct Material {
    effect: Handle<MaterialEffect>,
    albedo: Texture,
    sampler: Sampler,
    set: DescriptorSet,
    set_layout: DescriptorSetLayout,
}

impl Material {
    /// Creates a new material derived from a base material
    pub fn new(
        context: Rc<VulkanContext>,
        layout_cache: &mut DescriptorLayoutCache,
        descriptor_allocator: &mut DescriptorAllocator,
        effect: Handle<MaterialEffect>,
        info: MaterialInfo,
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

        Ok(Self {
            albedo,
            effect,
            sampler,
            set,
            set_layout,
        })
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

    /// Return the material's sampler.
    pub fn sampler(&self) -> &Sampler {
        &self.sampler
    }

    /// Get a reference to the material's base material.
    pub fn effect(&self) -> &Handle<MaterialEffect> {
        &self.effect
    }
}
