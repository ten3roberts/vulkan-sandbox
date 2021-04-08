use std::rc::Rc;

use super::{renderpass::MAX_ATTACHMENTS, Error, RenderPass};
use arrayvec::ArrayVec;
use ash::version::DeviceV1_0;
use ash::vk;
use ash::Device;

/// A framebuffer wraps one or more Textures contained in a renderpass.
/// The framebuffer does not own the Textures and as such the user must ensure the referenced
/// textures are kept alive. This is because a texture can be used in several framebuffers
/// simultaneously.
pub struct Framebuffer {
    device: Rc<Device>,
    framebuffer: vk::Framebuffer,
}

impl Framebuffer {
    pub fn new<T: AsRef<vk::ImageView>>(
        device: Rc<Device>,
        renderpass: &RenderPass,
        attachments: &[T],
        width: u32,
        height: u32,
    ) -> Result<Self, Error> {
        let attachment_views = attachments
            .iter()
            .map(|attachment| *attachment.as_ref())
            .collect::<ArrayVec<[vk::ImageView; MAX_ATTACHMENTS]>>();

        let create_info = vk::FramebufferCreateInfo::builder()
            .render_pass(renderpass.renderpass())
            .attachments(&attachment_views)
            .width(width)
            .height(height)
            .layers(1);

        let framebuffer = unsafe { device.create_framebuffer(&create_info, None)? };

        Ok(Framebuffer {
            device,
            framebuffer,
        })
    }

    pub fn framebuffer(&self) -> vk::Framebuffer {
        self.framebuffer
    }
}

impl Drop for Framebuffer {
    fn drop(&mut self) {
        unsafe { self.device.destroy_framebuffer(self.framebuffer, None) }
    }
}
