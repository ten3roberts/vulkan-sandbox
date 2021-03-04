use std::rc::Rc;

use super::Error;
use ash::version::DeviceV1_0;
use ash::vk;
use ash::Device;

pub fn create_layout(
    device: &Device,
) -> Result<vk::DescriptorSetLayout, Error> {
    let bindings = [vk::DescriptorSetLayoutBinding {
        binding: 0,
        descriptor_count: 1,
        descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
        stage_flags: vk::ShaderStageFlags::VERTEX,
        p_immutable_samplers: std::ptr::null(),
    }];

    let create_info =
        vk::DescriptorSetLayoutCreateInfo::builder().bindings(&bindings);

    let layout =
        unsafe { device.create_descriptor_set_layout(&create_info, None)? };
    Ok(layout)
}

pub fn destroy_layout(device: &Device, layout: vk::DescriptorSetLayout) {
    unsafe { device.destroy_descriptor_set_layout(layout, None) }
}

pub struct DescriptorPool {
    device: Rc<Device>,
    descriptor_pool: vk::DescriptorPool,
}

impl DescriptorPool {
    pub fn new(
        device: Rc<Device>,
        max_sets: u32,
        uniformbuffer_count: u32,
    ) -> Result<Self, Error> {
        let pool_sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: uniformbuffer_count,
        }];

        let create_info = vk::DescriptorPoolCreateInfo::builder()
            .pool_sizes(&pool_sizes)
            .max_sets(max_sets);

        let descriptor_pool =
            unsafe { device.create_descriptor_pool(&create_info, None)? };

        Ok(Self {
            device,
            descriptor_pool,
        })
    }

    /// Allocates descriptor sets from pool
    /// Allocates one descriptor set for each element in `layouts`
    pub fn allocate(
        &self,
        layouts: &[vk::DescriptorSetLayout],
    ) -> Result<Vec<vk::DescriptorSet>, Error> {
        let alloc_info = vk::DescriptorSetAllocateInfo {
            s_type: vk::StructureType::DESCRIPTOR_SET_ALLOCATE_INFO,
            p_next: std::ptr::null(),
            descriptor_pool: self.descriptor_pool,
            descriptor_set_count: layouts.len() as _,
            p_set_layouts: layouts.as_ptr(),
        };

        let descriptor_sets =
            unsafe { self.device.allocate_descriptor_sets(&alloc_info)? };

        Ok(descriptor_sets)
    }

    /// Resets all descriptors allocated from pool
    /// Frees all allocated descriptor sets
    pub fn reset(&self) -> Result<(), Error> {
        unsafe {
            self.device.reset_descriptor_pool(
                self.descriptor_pool,
                Default::default(),
            )?
        };
        Ok(())
    }
}

impl Drop for DescriptorPool {
    fn drop(&mut self) {
        unsafe {
            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None)
        };
    }
}

pub fn write<B>(device: &Device, descriptor_set: vk::DescriptorSet, buffer: B)
where
    B: AsRef<vk::Buffer>,
{
    let buffer_info = vk::DescriptorBufferInfo {
        buffer: *buffer.as_ref(),
        offset: 0,
        range: vk::WHOLE_SIZE,
    };

    let descriptor_write = vk::WriteDescriptorSet {
        s_type: vk::StructureType::WRITE_DESCRIPTOR_SET,
        p_next: std::ptr::null(),
        dst_set: descriptor_set,
        dst_binding: 0,
        dst_array_element: 0,
        descriptor_count: 1,
        descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
        p_image_info: std::ptr::null(),
        p_buffer_info: &buffer_info,
        p_texel_buffer_view: std::ptr::null(),
    };

    unsafe { device.update_descriptor_sets(&[descriptor_write], &[]) };
}
