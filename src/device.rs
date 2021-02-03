use crate::Error;
use ash::Instance;
use ash::{
    extensions::khr::Surface,
    vk::{self, SurfaceKHR},
};
use ash::{version::DeviceV1_0, version::InstanceV1_0};
use std::{collections::HashSet, ffi::CString};

pub struct QueueFamilies {
    graphics: Option<u32>,
    present: Option<u32>,
    transfer: Option<u32>,
}

impl QueueFamilies {
    pub fn find(
        instance: &Instance,
        device: vk::PhysicalDevice,
        surface_loader: &Surface,
        surface: SurfaceKHR,
    ) -> Result<QueueFamilies, Error> {
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

            if unsafe {
                surface_loader.get_physical_device_surface_support(device, i as u32, surface)?
            } {
                queue_families.present = Some(i as u32);
            }

            if family.queue_flags.contains(vk::QueueFlags::TRANSFER) {
                queue_families.transfer = Some(i as u32);
            }
        }

        Ok(queue_families)
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
    surface_loader: &Surface,
    surface: SurfaceKHR,
) -> Option<(vk::PhysicalDevice, Score, QueueFamilies)> {
    let properties = unsafe { instance.get_physical_device_properties(device) };
    let _features = unsafe { instance.get_physical_device_features(device) };

    let mut score: Score = 0;

    if properties.device_type == vk::PhysicalDeviceType::DISCRETE_GPU {
        score += 1000;
    }

    let queue_families = QueueFamilies::find(instance, device, surface_loader, surface).ok()?;

    // Graphics queue is required
    if !queue_families.has_graphics() {
        return None;
    }

    // Present queue is required
    if !queue_families.has_present() {
        return None;
    }

    score += properties.limits.max_image_dimension2_d as Score;
    score += properties.limits.max_push_constants_size as Score;

    Some((device, score, queue_families))
}

// Picks an appropriate physical device
fn pick_physical_device(
    instance: &Instance,
    surface_loader: &Surface,
    surface: SurfaceKHR,
) -> Result<(vk::PhysicalDevice, QueueFamilies), Error> {
    let devices = unsafe { instance.enumerate_physical_devices()? };

    let (device, _, queue_families) = devices
        .into_iter()
        .filter_map(|d| rate_physical_device(instance, d, surface_loader, surface))
        .max_by_key(|v| v.0)
        .ok_or(Error::UnsuitableDevice)?;

    Ok((device, queue_families))
}

/// Creates a logical device by choosing the best appropriate physical device
pub fn create(
    instance: &Instance,
    surface_loader: &Surface,
    surface: SurfaceKHR,
    layers: &[&str],
) -> Result<(ash::Device, QueueFamilies), Error> {
    let (physical_device, queue_families) =
        pick_physical_device(instance, surface_loader, surface)?;

    let mut unique_queue_families = HashSet::new();
    unique_queue_families.insert(queue_families.graphics().unwrap());
    unique_queue_families.insert(queue_families.present().unwrap());

    log::debug!("Unique queue families: {}", unique_queue_families.len());

    let queue_create_infos: Vec<_> = unique_queue_families
        .iter()
        .map(|index| {
            vk::DeviceQueueCreateInfo::builder()
                .queue_family_index(*index)
                .queue_priorities(&[1.0f32])
                .build()
        })
        .collect();

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

pub fn get_queue(device: &ash::Device, family_index: u32, index: u32) -> vk::Queue {
    unsafe { device.get_device_queue(family_index, index) }
}

pub fn destroy(device: ash::Device) {
    unsafe { device.destroy_device(None) };
}
