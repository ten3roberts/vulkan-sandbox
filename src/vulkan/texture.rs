use std::{path::Path, rc::Rc};

use ash::version::DeviceV1_0;
use ash::vk;

use super::{buffer, commands::*, context::VulkanContext, extent::Extent, Error};

pub use vk::Format;
pub use vk::SampleCountFlags;

/// Specifies texture creation info.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextureInfo {
    pub extent: Extent,
    /// The maximum amount of mip levels to use.
    /// Actual value may be lower due to texture size.
    /// A value of zero uses the maximum mip levels.
    pub mip_levels: u32,
    /// The type/aspect of texture.
    pub usage: TextureUsage,
    /// The pixel format.
    pub format: Format,
    pub samples: SampleCountFlags,
}

impl Default for TextureInfo {
    fn default() -> Self {
        Self {
            extent: (512, 512).into(),
            mip_levels: 1,
            usage: TextureUsage::Sampled,
            format: Format::R8G8B8A8_SRGB,
            samples: SampleCountFlags::TYPE_1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureUsage {
    /// The most common usage. Texture is sampled in shader and transferred from CPU rarely.
    Sampled,
    /// Texture is used as a color attachment. Lazily allocates image when possible.
    ColorAttachment,
    /// Texture is used as a depth attachment. Lazily allocates image when possible.
    DepthAttachment,
}

// Represents a texture combining an image and image view. A texture also stores its own width,
// height, format, mipmapping levels and samples. Manages the deallocation of image memory unless
// created manually without provided allocation using `from_image`.
pub struct Texture {
    context: Rc<VulkanContext>,
    image: vk::Image,
    image_view: vk::ImageView,
    format: vk::Format,
    // May not necessarily own the allocation
    allocation: Option<vk_mem::Allocation>,
    extent: Extent,
    mip_levels: u32,
    samples: vk::SampleCountFlags,
    usage: TextureUsage,
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
                extent: (image.width(), image.height()).into(),
                mip_levels: 0,
                ..Default::default()
            },
        )?;

        let size = image.width() as u64 * image.height() as u64 * 4;
        texture.write(size, image.pixels())?;
        Ok(texture)
    }

    /// Creates a texture from provided raw pixels
    /// Note, raw pixels must match format, width, and height
    pub fn new(context: Rc<VulkanContext>, info: TextureInfo) -> Result<Self, Error> {
        // Re-alias as mutable
        let mut info = info;
        let mut mip_levels = calculate_mip_levels(info.extent);

        // Multisampled images cannot use more than one miplevel
        if info.samples != vk::SampleCountFlags::TYPE_1 {
            info.mip_levels = 1;
        }

        // Don't use more mip_levels than info
        if info.mip_levels != 0 {
            mip_levels = mip_levels.min(info.mip_levels)
        }

        // Override mip levels
        info.mip_levels = mip_levels;

        let vk_usage = match info.usage {
            TextureUsage::Sampled => {
                vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED
            }
            TextureUsage::ColorAttachment => {
                vk::ImageUsageFlags::TRANSIENT_ATTACHMENT | vk::ImageUsageFlags::COLOR_ATTACHMENT
            }
            TextureUsage::DepthAttachment => vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
        } | if mip_levels > 1 {
            vk::ImageUsageFlags::TRANSFER_SRC
        } else {
            vk::ImageUsageFlags::default()
        };

        let memory_usage = vk_mem::MemoryUsage::GpuOnly;
        let flags = vk_mem::AllocationCreateFlags::NONE;

        let image_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .extent(vk::Extent3D {
                width: info.extent.width,
                height: info.extent.height,
                depth: 1,
            })
            .mip_levels(mip_levels)
            .array_layers(1)
            .format(info.format)
            .tiling(vk::ImageTiling::OPTIMAL)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(vk_usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .samples(info.samples);

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
        let aspect_mask = match info.usage {
            TextureUsage::Sampled => vk::ImageAspectFlags::COLOR,
            TextureUsage::ColorAttachment => vk::ImageAspectFlags::COLOR,
            TextureUsage::DepthAttachment => vk::ImageAspectFlags::DEPTH,
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
            extent: info.extent,
            mip_levels: info.mip_levels,
            format: info.format,
            samples: info.samples,
            usage: info.usage,
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
            self.extent,
        )?;

        // Generate Mipmaps
        generate_mipmaps(
            transfer_pool,
            graphics_queue,
            self.image,
            self.extent,
            self.mip_levels,
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

    pub fn mip_levels(&self) -> u32 {
        self.mip_levels
    }

    /// Return a reference to the texture's samples.
    pub fn samples(&self) -> vk::SampleCountFlags {
        self.samples
    }

    /// Return a reference to the texture's type
    pub fn usage(&self) -> TextureUsage {
        self.usage
    }

    // Returns the textures width and height
    pub fn extent(&self) -> Extent {
        self.extent
    }
}

impl AsRef<vk::ImageView> for Texture {
    fn as_ref(&self) -> &vk::ImageView {
        &self.image_view
    }
}

impl AsRef<vk::Image> for Texture {
    fn as_ref(&self) -> &vk::Image {
        &self.image
    }
}

impl Into<vk::ImageView> for &Texture {
    fn into(self) -> vk::ImageView {
        self.image_view
    }
}

impl Into<vk::Image> for &Texture {
    fn into(self) -> vk::Image {
        self.image
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

fn calculate_mip_levels(extent: Extent) -> u32 {
    (extent.width.max(extent.height) as f32).log2().floor() as u32 + 1
}

fn generate_mipmaps(
    commandpool: &CommandPool,
    queue: vk::Queue,
    image: vk::Image,
    extent: Extent,
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

    let mut mip_width = extent.width;
    let mut mip_height = extent.height;

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

            (vk::ImageLayout::TRANSFER_DST_OPTIMAL, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL) => (
                vk::AccessFlags::TRANSFER_WRITE,
                vk::AccessFlags::SHADER_READ,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
            ),
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
