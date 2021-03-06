use std::rc::Rc;

use super::{Error, Texture, TextureUsage};
use arrayvec::ArrayVec;
use ash::Device;
use ash::{version::DeviceV1_0, vk::SampleCountFlags};

use ash::vk;

pub use vk::AttachmentLoadOp as LoadOp;
pub use vk::AttachmentReference;
pub use vk::AttachmentStoreOp as StoreOp;
pub use vk::Format;
pub use vk::ImageLayout;

pub const MAX_ATTACHMENTS: usize = 8;
pub const MAX_SUBPASSES: usize = 8;

/// Specifies a renderpass attachment.
/// Note: the actual images are provided in the frambuffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttachmentInfo {
    pub usage: TextureUsage,
    /// Attachment image format
    pub format: Format,
    /// Number of samples for attachment
    pub samples: SampleCountFlags,
    /// What to do with existing attachment content.
    pub store: StoreOp,
    /// What to do with new attachment content.
    pub load: LoadOp,
    /// The expected image layout of loaded subpass contents
    /// Usually UNDEFINED unless using output of previous renderpass/subpass.
    pub initial_layout: ImageLayout,
    /// Image layout to transition to after renderpass.
    pub final_layout: ImageLayout,
}

impl Default for AttachmentInfo {
    fn default() -> Self {
        Self {
            usage: TextureUsage::ColorAttachment,
            format: Format::R8G8B8A8_SRGB,
            samples: SampleCountFlags::TYPE_1,
            store: StoreOp::STORE,
            load: LoadOp::DONT_CARE,
            initial_layout: ImageLayout::UNDEFINED,
            final_layout: ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        }
    }
}

impl AttachmentInfo {
    /// Generates a new AttachmentInfo with format, samples from a texture.
    /// returned `AttachmentInfo` is not tied to the texture
    pub fn from_texture(
        texture: &Texture,
        load: LoadOp,
        store: StoreOp,
        initial_layout: ImageLayout,
        final_layout: ImageLayout,
    ) -> Self {
        Self {
            usage: texture.usage(),
            format: texture.format(),
            samples: texture.samples(),
            load,
            store,
            initial_layout,
            final_layout,
        }
    }
}

impl Into<vk::AttachmentDescription> for &AttachmentInfo {
    fn into(self) -> vk::AttachmentDescription {
        vk::AttachmentDescription {
            flags: vk::AttachmentDescriptionFlags::default(),
            format: self.format,
            samples: self.samples,
            load_op: self.load,
            store_op: self.store,
            stencil_load_op: LoadOp::DONT_CARE,
            stencil_store_op: StoreOp::DONT_CARE,
            initial_layout: self.initial_layout,
            final_layout: self.final_layout,
        }
    }
}

#[derive(Debug)]
pub struct SubpassInfo<'a, 'b> {
    pub color_attachments: &'a [vk::AttachmentReference],
    /// The attachment indices to use as resolve attachmetns
    pub resolve_attachments: &'b [vk::AttachmentReference],
    pub depth_attachment: Option<AttachmentReference>,
}

impl<'a, 'b> Into<vk::SubpassDescription> for &SubpassInfo<'a, 'b> {
    fn into(self) -> vk::SubpassDescription {
        vk::SubpassDescription {
            flags: vk::SubpassDescriptionFlags::default(),
            pipeline_bind_point: vk::PipelineBindPoint::GRAPHICS,
            input_attachment_count: 0,
            p_input_attachments: std::ptr::null(),
            color_attachment_count: self.color_attachments.len() as u32,
            p_color_attachments: self.color_attachments.as_ptr(),
            p_resolve_attachments: if self.resolve_attachments.len() > 0 {
                self.resolve_attachments.as_ptr()
            } else {
                std::ptr::null()
            },
            p_depth_stencil_attachment: match &self.depth_attachment {
                Some(attachment) => attachment,
                None => std::ptr::null(),
            },
            preserve_attachment_count: 0,
            p_preserve_attachments: std::ptr::null(),
        }
    }
}

#[derive(Debug)]
/// Specifies renderpass creation info. For array conversion reasons, the number of attachments
/// cannot be more than `MAX_ATTACHMENTS` and subpasses no more than `MAX_SUBPASSES`.
pub struct RenderPassInfo<'a, 'b, 'c, 'd> {
    pub attachments: &'a [AttachmentInfo],
    pub subpasses: &'b [SubpassInfo<'c, 'd>],
}

pub struct RenderPass {
    device: Rc<Device>,
    renderpass: vk::RenderPass,
}

impl RenderPass {
    pub fn new(device: Rc<Device>, info: &RenderPassInfo) -> Result<Self, Error> {
        // Convert attachment infos into vulkan equivalent
        let vk_attachments = info
            .attachments
            .iter()
            .map(|attachment| attachment.into())
            .collect::<ArrayVec<[vk::AttachmentDescription; MAX_ATTACHMENTS]>>();

        let vk_subpasses = info
            .subpasses
            .iter()
            .map(|subpass| subpass.into())
            .collect::<ArrayVec<[vk::SubpassDescription; MAX_SUBPASSES]>>();

        let dependencies = [vk::SubpassDependency {
            src_subpass: vk::SUBPASS_EXTERNAL,
            dst_subpass: 0,
            src_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
            src_access_mask: vk::AccessFlags::default(),
            dst_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
            dst_access_mask: vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
            dependency_flags: vk::DependencyFlags::default(),
        }];

        let create_info = vk::RenderPassCreateInfo::builder()
            .attachments(&vk_attachments)
            .subpasses(&vk_subpasses)
            .dependencies(&dependencies);

        let renderpass = unsafe { device.create_render_pass(&create_info, None)? };

        Ok(RenderPass { device, renderpass })
    }

    pub fn renderpass(&self) -> vk::RenderPass {
        self.renderpass
    }
}

impl Drop for RenderPass {
    fn drop(&mut self) {
        unsafe { self.device.destroy_render_pass(self.renderpass, None) }
    }
}
