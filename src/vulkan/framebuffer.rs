use std::rc::Rc;

use super::{renderpass::RenderPass, Error};
use ash::version::DeviceV1_0;
use ash::vk;
use ash::Device;

pub struct Framebuffer {
    device: Rc<Device>,
    framebuffer: vk::Framebuffer,
}

impl Framebuffer {
    pub fn new(
        device: Rc<Device>,
        renderpass: &RenderPass,
        attachments: &[vk::ImageView],
        extent: vk::Extent2D,
    ) -> Result<Self, Error> {
        let create_info = vk::FramebufferCreateInfo::builder()
            .render_pass(renderpass.renderpass())
            .attachments(attachments)
            .width(extent.width)
            .height(extent.height)
            .layers(1);

        let framebuffer =
            unsafe { device.create_framebuffer(&create_info, None)? };

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
