use crate::Error;
use ash::version::DeviceV1_0;
use ash::Device;

use ash::vk;

pub struct RenderPass<'a> {
    device: &'a Device,
    renderpass: vk::RenderPass,
}

impl<'a> RenderPass<'a> {
    pub fn new(device: &'a Device, format: vk::Format) -> Result<Self, Error> {
        let attachments = [vk::AttachmentDescription::builder()
            .format(format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .build()];

        let color_attachment_ref = vk::AttachmentReference {
            attachment: 0,
            // Layout during subpass
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        };

        let subpasses = [vk::SubpassDescription::builder()
            .color_attachments(&[color_attachment_ref])
            .build()];

        let create_info = vk::RenderPassCreateInfo::builder()
            .attachments(&attachments)
            .subpasses(&subpasses);
        let renderpass = unsafe { device.create_render_pass(&create_info, None)? };

        Ok(RenderPass { device, renderpass })
    }

    pub fn renderpass(&self) -> vk::RenderPass {
        self.renderpass
    }
}

impl<'a> Drop for RenderPass<'a> {
    fn drop(&mut self) {
        unsafe { self.device.destroy_render_pass(self.renderpass, None) }
    }
}
