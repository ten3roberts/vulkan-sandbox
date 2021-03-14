use std::{path::Path, rc::Rc};

use ash::version::DeviceV1_0;
use ash::vk;
use stb_image::image;

use super::{buffer, commands::*, context::VulkanContext, Error};

// Represents a texture combining an image and image view
pub struct Texture {
    context: Rc<VulkanContext>,
    image: vk::Image,
    image_view: vk::ImageView,
    format: vk::Format,
    // May not necessarily own the allocation
    allocation: Option<vk_mem::Allocation>,
    width: u32,
    height: u32,
}

impl Texture {
    pub fn load<P: AsRef<Path> + ToString>(
        context: Rc<VulkanContext>,
        path: P,
    ) -> Result<Self, Error> {
        let pixels = match image::load_with_depth(&path, 4, true) {
            image::LoadResult::Error(_) => return Err(Error::ImageError(path.to_string())),
            image::LoadResult::ImageU8(pixels) => pixels,
            image::LoadResult::ImageF32(_) => unreachable!(),
        };

        Self::new(context, pixels.width as _, pixels.height as _, &pixels.data)
    }

    /// Creates a texture from raw pixels
    /// pixels are of format R8G8B8A8
    pub fn new(
        context: Rc<VulkanContext>,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> Result<Self, Error> {
        let size = width as u64 * height as u64 * 4;

        let memory_usage = vk_mem::MemoryUsage::GpuOnly;
        let flags = vk_mem::AllocationCreateFlags::NONE;
        let format = vk::Format::R8G8B8A8_SRGB;
        let image_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .format(format)
            .tiling(vk::ImageTiling::OPTIMAL)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .samples(vk::SampleCountFlags::TYPE_1);

        let allocator = context.allocator();

        let (image, allocation, _allocation_info) = allocator.create_image(
            &image_info,
            &vk_mem::AllocationCreateInfo {
                usage: memory_usage,
                flags,
                ..Default::default()
            },
        )?;

        let texture = Self::from_image(context, width, height, format, image, Some(allocation))?;

        texture.write(size, pixels)?;

        Ok(texture)
    }

    /// Creates a texture from an already existing VkImage
    /// If allocation is provided, the image will be destroyed along with self
    pub fn from_image(
        context: Rc<VulkanContext>,
        width: u32,
        height: u32,
        format: vk::Format,
        image: vk::Image,
        allocation: Option<vk_mem::Allocation>,
    ) -> Result<Self, Error> {
        let create_info = vk::ImageViewCreateInfo::builder()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        let image_view = unsafe { context.device().create_image_view(&create_info, None) }?;

        Ok(Self {
            context,
            image,
            image_view,
            format,
            allocation,
            width,
            height,
        })
    }

    pub fn write(&self, size: vk::DeviceSize, pixels: &[u8]) -> Result<(), Error> {
        let allocator = self.context.allocator();
        // Create a new or reuse staging buffer
        let (staging_buffer, staging_allocation, staging_info) =
            buffer::create_staging(allocator, size as _, true)?;

        let mapped = staging_info.get_mapped_data();

        // Use the write function to write into the mapped memory
        unsafe { std::ptr::copy_nonoverlapping(pixels.as_ptr(), mapped, size as _) }

        let transfer_pool = self.context.transfer_pool();
        let graphics_queue = self.context.graphics_queue();

        // Prepare the image layout
        transition_layout(
            transfer_pool,
            graphics_queue,
            self.image,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        )?;

        buffer::copy_to_image(
            transfer_pool,
            graphics_queue,
            staging_buffer,
            self.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            self.width,
            self.height,
        )?;

        // Transition back to initial layout
        transition_layout(
            transfer_pool,
            graphics_queue,
            self.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        )?;

        // Destroy the staging buffer
        allocator.destroy_buffer(staging_buffer, &staging_allocation)?;
        Ok(())
    }

    pub fn format(&self) -> vk::Format {
        self.format
    }

    pub fn image(&self) -> vk::Image {
        self.image
    }

    pub fn image_view(&self) -> vk::ImageView {
        self.image_view
    }
}

impl Drop for Texture {
    fn drop(&mut self) {
        let allocator = self.context.allocator();

        // Destroy allocation if texture owns image
        if let Some(allocation) = self.allocation.take() {
            allocator.destroy_image(self.image, &allocation).unwrap();
        }

        // Destroy image view
        unsafe {
            self.context
                .device()
                .destroy_image_view(self.image_view, None);
        }
    }
}

// Transitions image layout from one layout to another using a pipeline barrier
pub fn transition_layout(
    commandpool: &CommandPool,
    queue: vk::Queue,
    image: vk::Image,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
) -> Result<(), Error> {
    let (src_access_mask, dst_access_mask, src_stage_mask, dst_stage_mask) =
        match (old_layout, new_layout) {
            (vk::ImageLayout::UNDEFINED, vk::ImageLayout::TRANSFER_DST_OPTIMAL) => (
                vk::AccessFlags::default(),
                vk::AccessFlags::TRANSFER_WRITE,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
            ),

            (vk::ImageLayout::TRANSFER_DST_OPTIMAL, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL) => {
                (
                    vk::AccessFlags::TRANSFER_WRITE,
                    vk::AccessFlags::SHADER_READ,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                )
            }
            _ => return Err(Error::UnsupportedLayoutTransition(old_layout, new_layout)),
        };

    let barrier = vk::ImageMemoryBarrier {
        s_type: vk::StructureType::IMAGE_MEMORY_BARRIER,
        p_next: std::ptr::null(),
        src_access_mask,
        dst_access_mask,
        old_layout,
        new_layout,
        src_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
        dst_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
        image,
        subresource_range: vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        },
    };

    commandpool.single_time_command(queue, |commandbuffer| {
        commandbuffer.pipeline_barrier(src_stage_mask, dst_stage_mask, &[barrier])
    })
}
