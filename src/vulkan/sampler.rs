use std::rc::Rc;

use super::{Error, VulkanContext};
use ash::version::DeviceV1_0;
use ash::vk;

// Re-export enums
pub use vk::Filter as FilterMode;
pub use vk::SamplerAddressMode as AddressMode;

#[derive(Clone, Copy, PartialEq)]
pub struct SamplerInfo {
    pub address_mode: vk::SamplerAddressMode,
    pub filter_mode: vk::Filter,
    pub unnormalized_coordinates: bool,
    // From 1.0 to 16.0
    // Anisotropy is disabled if value is set to 1.0
    pub anisotropy: f32,
}

pub struct Sampler {
    context: Rc<VulkanContext>,
    sampler: vk::Sampler,
}

impl Sampler {
    // Creates a new sampler from the specified sampling options
    pub fn new(context: Rc<VulkanContext>, info: SamplerInfo) -> Result<Self, Error> {
        let max_anisotropy = info.anisotropy.max(context.limits().max_sampler_anisotropy);
        let anisotropy_enable = if max_anisotropy > 1.0 {
            vk::TRUE
        } else {
            vk::FALSE
        };

        let create_info = vk::SamplerCreateInfo {
            s_type: vk::StructureType::SAMPLER_CREATE_INFO,
            p_next: std::ptr::null(),
            flags: vk::SamplerCreateFlags::default(),
            mag_filter: info.filter_mode,
            min_filter: info.filter_mode,
            mipmap_mode: vk::SamplerMipmapMode::LINEAR,
            address_mode_u: info.address_mode,
            address_mode_v: info.address_mode,
            address_mode_w: info.address_mode,
            mip_lod_bias: 0.0,
            anisotropy_enable,
            max_anisotropy,
            compare_enable: vk::FALSE,
            compare_op: vk::CompareOp::ALWAYS,
            min_lod: 0.0,
            max_lod: 0.0,
            border_color: vk::BorderColor::INT_OPAQUE_BLACK,
            unnormalized_coordinates: info.unnormalized_coordinates as u32,
        };

        let sampler = unsafe { context.device().create_sampler(&create_info, None)? };
        Ok(Self { context, sampler })
    }

    pub fn sampler(&self) -> vk::Sampler {
        self.sampler
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        unsafe {
            self.context.device().destroy_sampler(self.sampler, None);
        }
    }
}

impl AsRef<vk::Sampler> for Sampler {
    fn as_ref(&self) -> &vk::Sampler {
        &self.sampler
    }
}
