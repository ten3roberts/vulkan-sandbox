use crate::{swapchain, Error};
use ash::Instance;
use ash::{
    extensions::khr::Surface,
    vk::{self, SurfaceKHR},
};
use ash::{version::DeviceV1_0, version::InstanceV1_0};
use std::{
    collections::HashSet,
    ffi::{CStr, CString},
};

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

const DEVICE_EXTENSIONS: &[&str] = &["VK_KHR_swapchain"];

// Rates physical device suitability
fn rate_physical_device(
    instance: &Instance,
    device: vk::PhysicalDevice,
    surface_loader: &Surface,
    surface: SurfaceKHR,
    extensions: &[CString],
) -> Option<(vk::PhysicalDevice, Score, QueueFamilies)> {
    let properties = unsafe { instance.get_physical_device_properties(device) };
    let _features = unsafe { instance.get_physical_device_features(device) };

    // Current device does not support one or more extensions
    if !get_missing_extensions(instance, device, extensions)
        .ok()?
        .is_empty()
    {
        return None;
    }

    // Ensure swapchain capabilites
    let swapchain_support = swapchain::query_support(surface_loader, surface, device).ok()?;

    // Swapchain support isn't adequate
    if swapchain_support.formats.is_empty() || swapchain_support.present_modes.is_empty() {
        return None;
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

    // Device is valid

    let mut score: Score = 0;

    if properties.device_type == vk::PhysicalDeviceType::DISCRETE_GPU {
        score += 1000;
    }

    score += properties.limits.max_image_dimension2_d as Score;
    score += properties.limits.max_push_constants_size as Score;

    Some((device, score, queue_families))
}

fn get_missing_extensions(
    instance: &Instance,
    device: vk::PhysicalDevice,
    extensions: &[CString],
) -> Result<Vec<CString>, Error> {
    let available = unsafe { instance.enumerate_device_extension_properties(device)? };

    Ok(extensions
        .iter()
        .filter(|ext| {
            available
                .iter()
                .find(|avail| unsafe {
                    CStr::from_ptr(avail.extension_name.as_ptr()) == ext.as_c_str()
                })
                .is_none()
        })
        .cloned()
        .collect())
}

// Picks an appropriate physical device
fn pick_physical_device(
    instance: &Instance,
    surface_loader: &Surface,
    surface: SurfaceKHR,
    extensions: &[CString],
) -> Result<(vk::PhysicalDevice, QueueFamilies), Error> {
    let devices = unsafe { instance.enumerate_physical_devices()? };

    let (device, _, queue_families) = devices
        .into_iter()
        .filter_map(|d| rate_physical_device(instance, d, surface_loader, surface, &extensions))
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
) -> Result<(ash::Device, vk::PhysicalDevice, QueueFamilies), Error> {
    let extensions = DEVICE_EXTENSIONS
        .iter()
        .map(|s| CString::new(*s))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    let (physical_device, queue_families) =
        pick_physical_device(instance, surface_loader, surface, &extensions)?;

    let mut unique_queue_families = HashSet::new();
    unique_queue_families.insert(queue_families.graphics().unwrap());
    unique_queue_families.insert(queue_families.present().unwrap());

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

    let extension_names_raw = extensions
        .iter()
        .map(|ext| ext.as_ptr() as *const i8)
        .collect::<Vec<_>>();

    let create_info = vk::DeviceCreateInfo::builder()
        .queue_create_infos(&queue_create_infos)
        .enabled_extension_names(&extension_names_raw)
        .enabled_layer_names(&layer_names_raw);

    let device = unsafe { instance.create_device(physical_device, &create_info, None)? };
    Ok((device, physical_device, queue_families))
}

pub fn get_queue(device: &ash::Device, family_index: u32, index: u32) -> vk::Queue {
    unsafe { device.get_device_queue(family_index, index) }
}

pub fn destroy(device: ash::Device) {
    unsafe { device.destroy_device(None) };
}
