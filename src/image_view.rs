use ash::version::DeviceV1_0;
use ash::{vk, Device};

use crate::Error;

/// Creates an image view from image, format, width and height
pub fn create(
    device: &Device,
    image: vk::Image,
    format: vk::Format,
) -> Result<vk::ImageView, Error> {
    let create_info = vk::ImageViewCreateInfo::builder()
        .image(image)
        .format(format)
        .view_type(vk::ImageViewType::TYPE_2D)
        .components(vk::ComponentMapping {
            r: vk::ComponentSwizzle::IDENTITY,
            g: vk::ComponentSwizzle::IDENTITY,
            b: vk::ComponentSwizzle::IDENTITY,
            a: vk::ComponentSwizzle::IDENTITY,
        })
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        });

    let image_view = unsafe { device.create_image_view(&create_info, None)? };
    Ok(image_view)
}

pub fn destroy(device: &Device, image_view: vk::ImageView) {
    unsafe {
        device.destroy_image_view(image_view, None);
    }
}
