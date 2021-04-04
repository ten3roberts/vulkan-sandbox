use std::{path::Path, rc::Rc};

use ash::version::DeviceV1_0;
use ash::vk;

use super::{buffer, commands::*, context::VulkanContext, Error};

pub use vk::Format;

/// Specifies texture creation info.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextureInfo {
    pub width: u32,
    pub height: u32,
    /// The maximum amount of mip levels to use.
    /// Actual value may be lower due to texture size.
    /// A value of zero uses the maximum mip levels.
    pub mip_levels: u32,
    /// The type/aspect of texture.
    pub ty: TextureType,
    /// The pixel format.
    pub format: Format,
}

impl Default for TextureInfo {
    fn default() -> Self {
        Self {
            width: 512,
            height: 512,
            mip_levels: 1,
            ty: TextureType::Color,
            format: Format::R8G8B8A8_SRGB,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureType {
    Color,
    Depth,
}

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
    mip_levels: u32,
}

impl Texture {
    /// Loads a color texture from an image file.
    /// Uses the width and height of the loaded image, no resizing.
    /// Uses mipmapping.
    pub fn load<P: AsRef<Path>>(context: Rc<VulkanContext>, path: P) -> Result<Self, Error> {
        let image =
            stb::Image::load(&path, 4).ok_or(Error::ImageError(path.as_ref().to_owned()))?;

        let texture = Self::new(
            context,
            TextureInfo {
                width: image.width(),
                height: image.height(),
                mip_levels: 0,
                ty: TextureType::Color,
                format: vk::Format::R8G8B8A8_SRGB,
            },
        )?;

        let size = image.width() as u64 * image.height() as u64 * 4;
        texture.write(size, image.pixels())?;
        Ok(texture)
    }

    /// Creates a texture from provided raw pixels
    /// Note, raw pixels must match format, width, and height
    pub fn new(context: Rc<VulkanContext>, info: TextureInfo) -> Result<Self, Error> {
        let mut mip_levels = calculate_mip_levels(info.width, info.height);

        // Don't use more mip_levels than info
        if info.mip_levels != 0 {
            mip_levels = mip_levels.min(info.mip_levels)
        }

        // Override mip levels
        let info = TextureInfo { mip_levels, ..info };

        let vk_usage = match info.ty {
            TextureType::Color => vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
            TextureType::Depth => vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
        } | if mip_levels > 1 {
            vk::ImageUsageFlags::TRANSFER_SRC
        } else {
            vk::ImageUsageFlags::default()
        };

        let memory_usage = vk_mem::MemoryUsage::GpuOnly;
        let flags = vk_mem::AllocationCreateFlags::NONE;

        log::debug!("Mip levels: {}", mip_levels);

        let image_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .extent(vk::Extent3D {
                width: info.width,
                height: info.height,
                depth: 1,
            })
            .mip_levels(mip_levels)
            .array_layers(1)
            .format(info.format)
            .tiling(vk::ImageTiling::OPTIMAL)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(vk_usage)
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
        Self::from_image(context, info, image, Some(allocation))
    }

    /// Creates a texture from an already existing VkImage
    /// If allocation is provided, the image will be destroyed along with self
    pub fn from_image(
        context: Rc<VulkanContext>,
        info: TextureInfo,
        image: vk::Image,
        allocation: Option<vk_mem::Allocation>,
    ) -> Result<Self, Error> {
        let aspect_mask = match info.ty {
            TextureType::Color => vk::ImageAspectFlags::COLOR,
            TextureType::Depth => vk::ImageAspectFlags::DEPTH,
        };

        let create_info = vk::ImageViewCreateInfo::builder()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(info.format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask,
                base_mip_level: 0,
                level_count: info.mip_levels,
                base_array_layer: 0,
                layer_count: 1,
            });

        let image_view = unsafe { context.device().create_image_view(&create_info, None) }?;

        Ok(Self {
            context,
            image,
            image_view,
            width: info.width,
            height: info.height,
            mip_levels: info.mip_levels,
            format: info.format,
            allocation,
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
            self.mip_levels,
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

        // Generate Mipmaps
        generate_mipmaps(
            transfer_pool,
            graphics_queue,
            self.image,
            self.width,
            self.height,
            self.mip_levels,
        )?;

        // Done in bitmap
        // Transition back to initial layout
        // transition_layout(
        //     transfer_pool,
        //     graphics_queue,
        //     self.image,
        //     self.mip_levels,
        //     vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        //     vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        // )?;

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

    pub fn mip_levels(&self) -> u32 {
        self.mip_levels
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

fn calculate_mip_levels(width: u32, height: u32) -> u32 {
    (width.max(height) as f32).log2().floor() as u32 + 1
}

fn generate_mipmaps(
    commandpool: &CommandPool,
    queue: vk::Queue,
    image: vk::Image,
    width: u32,
    height: u32,
    mip_levels: u32,
) -> Result<(), Error> {
    let mut barrier = vk::ImageMemoryBarrier {
        s_type: vk::StructureType::IMAGE_MEMORY_BARRIER,
        p_next: std::ptr::null(),
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
        ..Default::default()
    };

    let mut mip_width = width;
    let mut mip_height = height;

    commandpool.single_time_command(queue, |commandbuffer| {
        for i in 1..mip_levels {
            barrier.subresource_range.base_mip_level = i - 1;
            barrier.old_layout = vk::ImageLayout::TRANSFER_DST_OPTIMAL;
            barrier.new_layout = vk::ImageLayout::TRANSFER_SRC_OPTIMAL;

            barrier.src_access_mask = vk::AccessFlags::TRANSFER_WRITE;
            barrier.dst_access_mask = vk::AccessFlags::TRANSFER_READ;

            commandbuffer.pipeline_barrier(
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::TRANSFER,
                &[barrier],
            );

            let offset = vk::Offset3D {
                x: if mip_width > 1 {
                    (mip_width / 2) as _
                } else {
                    1
                },
                y: if mip_height > 1 {
                    (mip_height / 2) as _
                } else {
                    1
                },
                z: 1,
            };

            let blit = vk::ImageBlit {
                src_offsets: [
                    vk::Offset3D { x: 0, y: 0, z: 0 },
                    vk::Offset3D {
                        x: mip_width as i32,
                        y: mip_height as i32,
                        z: 1,
                    },
                ],
                dst_offsets: [vk::Offset3D { x: 0, y: 0, z: 0 }, offset],
                src_subresource: vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: i - 1,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                dst_subresource: vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: i,
                    base_array_layer: 0,
                    layer_count: 1,
                },
            };

            commandbuffer.blit_image(
                image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[blit],
                vk::Filter::LINEAR,
            );

            // Transition new mip level to SHADER_READ_ONLY_OPTIMAL
            barrier.old_layout = vk::ImageLayout::TRANSFER_SRC_OPTIMAL;
            barrier.new_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
            barrier.src_access_mask = vk::AccessFlags::TRANSFER_READ;
            barrier.dst_access_mask = vk::AccessFlags::SHADER_READ;

            commandbuffer.pipeline_barrier(
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                &[barrier],
            );

            if mip_width > 1 {
                mip_width /= 2;
            }

            if mip_height > 1 {
                mip_height /= 2;
            }
        }

        // Transition the last mip level to SHADER_READ_ONLY_OPTIMAL
        barrier.subresource_range.base_mip_level = mip_levels - 1;
        barrier.old_layout = vk::ImageLayout::TRANSFER_DST_OPTIMAL;
        barrier.new_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
        barrier.src_access_mask = vk::AccessFlags::TRANSFER_WRITE;
        barrier.dst_access_mask = vk::AccessFlags::SHADER_READ;

        commandbuffer.pipeline_barrier(
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            &[barrier],
        );
    })
}

// Transitions image layout from one layout to another using a pipeline barrier
fn transition_layout(
    commandpool: &CommandPool,
    queue: vk::Queue,
    image: vk::Image,
    mip_levels: u32,
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
            level_count: mip_levels,
            base_array_layer: 0,
            layer_count: 1,
        },
    };

    commandpool.single_time_command(queue, |commandbuffer| {
        commandbuffer.pipeline_barrier(src_stage_mask, dst_stage_mask, &[barrier])
    })
}
