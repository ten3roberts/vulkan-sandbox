use crate::Error;
use ash::vk;
use ash::{version::DeviceV1_0, version::InstanceV1_0};
use ash::Instance;
use std::ffi::CString;

pub struct QueueFamilies {
    graphics: Option<u32>,
    present: Option<u32>,
    transfer: Option<u32>,
}

impl QueueFamilies {
    pub fn find(instance: &Instance, device: vk::PhysicalDevice) -> QueueFamilies {
        let family_properties =
            unsafe { instance.get_physical_device_queue_family_properties(device) };
        let mut queue_families = QueueFamilies {
            graphics: None,
            present: None,
            transfer: None,
        };

        for (i, family) in family_properties.iter().enumerate() {
            if family.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                queue_families.graphics = Some(i as u32);
            }

            if family.queue_flags.contains(vk::QueueFlags::TRANSFER) {
                queue_families.transfer = Some(i as u32);
            }
        }

        queue_families
    }

    pub fn graphics(&self) -> Option<u32> {
        return self.graphics;
    }

    pub fn present(&self) -> Option<u32> {
        return self.present;
    }

    pub fn transfer(&self) -> Option<u32> {
        return self.transfer;
    }

    pub fn has_graphics(&self) -> bool {
        return self.graphics.is_some();
    }

    pub fn has_present(&self) -> bool {
        return self.present.is_some();
    }

    pub fn has_transfer(&self) -> bool {
        return self.transfer.is_some();
    }
}

type Score = usize;

// Rates physical device suitability
fn rate_physical_device(
    instance: &Instance,
    device: vk::PhysicalDevice,
) -> Option<(vk::PhysicalDevice, Score, QueueFamilies)> {
    let properties = unsafe { instance.get_physical_device_properties(device) };
    let _features = unsafe { instance.get_physical_device_features(device) };

    let mut score: Score = 0;

    if properties.device_type == vk::PhysicalDeviceType::DISCRETE_GPU {
        score += 1000;
    }

    let queue_families = QueueFamilies::find(instance, device);

    // Graphics queue is required
    if !queue_families.has_graphics() {
        return None;
    }

    score += properties.limits.max_image_dimension2_d as Score;
    score += properties.limits.max_push_constants_size as Score;

    Some((device, score, queue_families))
}

// Picks an appropriate physical device
fn pick_physical_device(instance: &Instance) -> Result<(vk::PhysicalDevice, QueueFamilies), Error> {
    let devices = unsafe { instance.enumerate_physical_devices()? };

    let (device, _, queue_families) = devices
        .into_iter()
        .filter_map(|d| rate_physical_device(instance, d))
        .max_by_key(|v| v.0)
        .ok_or(Error::UnsuitableDevice)?;

    Ok((device, queue_families))
}

/// Creates a logical device by choosing the best appropriate physical device
pub fn create(instance: &Instance, layers: &[&str]) -> Result<(ash::Device, QueueFamilies), Error> {
    let (physical_device, queue_families) = pick_physical_device(instance)?;

    let queue_create_infos = [vk::DeviceQueueCreateInfo::builder()
        .queue_family_index(queue_families.graphics().unwrap())
        .queue_priorities(&[1.0f32])
        .build()];

    // Get layers
    let layers = layers
        .iter()
        .map(|s| CString::new(*s))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    let layer_names_raw = layers
        .iter()
        .map(|layer| layer.as_ptr() as *const i8)
        .collect::<Vec<_>>();

    let create_info = vk::DeviceCreateInfo::builder()
        .queue_create_infos(&queue_create_infos)
        .enabled_layer_names(&layer_names_raw);

    let device = unsafe { instance.create_device(physical_device, &create_info, None)? };
    Ok((device, queue_families))
}

pub fn destroy(device: ash::Device) {
    unsafe { device.destroy_device(None) };
}
