use super::Error;
use ash::version::DeviceV1_0;
use ash::vk;
use ash::Device;

pub fn create(device: &Device, signaled: bool) -> Result<vk::Fence, Error> {
    let create_info = vk::FenceCreateInfo {
        s_type: vk::StructureType::FENCE_CREATE_INFO,
        p_next: std::ptr::null(),
        flags: if signaled {
            vk::FenceCreateFlags::SIGNALED
        } else {
            vk::FenceCreateFlags::default()
        },
    };

    let fence = unsafe { device.create_fence(&create_info, None)? };
    Ok(fence)
}

pub fn wait(device: &Device, fences: &[vk::Fence], wait_all: bool) -> Result<(), Error> {
    unsafe { device.wait_for_fences(fences, wait_all, std::u64::MAX)? }
    Ok(())
}

pub fn reset(device: &Device, fences: &[vk::Fence]) -> Result<(), Error> {
    unsafe { device.reset_fences(fences)? }
    Ok(())
}

pub fn destroy(device: &Device, fence: vk::Fence) {
    unsafe { device.destroy_fence(fence, None) }
}
