use std::ffi::CString;

use crate::Error;
use ash::{version::EntryV1_0, version::InstanceV1_0, Instance};
use ash::{vk, Entry};
use glfw::Glfw;

/// Creates a vulkan instance with the appropriate extensions and layers
pub fn create(
    entry: &Entry,
    glfw: &Glfw,
    name: &str,
    engine_name: &str,
) -> Result<Instance, Error> {
    let name = CString::new(name).unwrap();
    let engine_name = CString::new(engine_name).unwrap();

    let app_info = vk::ApplicationInfo::builder()
        .application_name(&name)
        .engine_name(&engine_name);

    let extensions: Vec<CString> = glfw
        .get_required_instance_extensions()
        .ok_or(Error::VulkanUnsupported)?
        .into_iter()
        .map(CString::new)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    let extension_names_raw = extensions
        .iter()
        .map(|ext| ext.as_ptr() as *const i8)
        .collect::<Vec<_>>();

    let create_info = vk::InstanceCreateInfo::builder()
        .application_info(&app_info)
        .enabled_extension_names(&extension_names_raw);

    let instance = unsafe { entry.create_instance(&create_info, None)? };
    Ok(instance)
}

pub fn destroy(instance: Instance) {
    unsafe { instance.destroy_instance(None) };
}
