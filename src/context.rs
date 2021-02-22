use crate::*;
use ash::{
    extensions::{ext::DebugUtils, khr::Surface},
    vk,
};
use log::info;

use glfw::Glfw;
use std::rc::Rc;

use crate::device::QueueFamilies;

pub struct VulkanContext {
    entry: ash::Entry,
    instance: ash::Instance,
    device: Rc<ash::Device>,
    physical_device: vk::PhysicalDevice,
    queue_families: QueueFamilies,
    debug_utils: Option<(DebugUtils, vk::DebugUtilsMessengerEXT)>,

    surface_loader: Surface,
    surface: vk::SurfaceKHR,

    graphics_queue: vk::Queue,
    present_queue: vk::Queue,
}

impl VulkanContext {
    pub fn new(glfw: &Glfw, window: &glfw::Window) -> Result<Self, Error> {
        let entry = entry::create()?;
        let instance = instance::create(&entry, &glfw, "Vulkan Application", "Custom")?;

        // Create debug utils if validation layers are enabled
        let debug_utils = if instance::ENABLE_VALIDATION_LAYERS {
            Some(debug_utils::create(&entry, &instance)?)
        } else {
            None
        };

        // debug_utils::create(&entry, &instance)?;
        let surface_loader = surface::create_loader(&entry, &instance);

        let surface = surface::create(&instance, &window)?;
        let (device, physical_device, queue_families) = device::create(
            &instance,
            &surface_loader,
            surface,
            instance::INSTANCE_LAYERS,
        )?;

        let graphics_queue = device::get_queue(&device, queue_families.graphics().unwrap(), 0);
        let present_queue = device::get_queue(&device, queue_families.present().unwrap(), 0);

        Ok(VulkanContext {
            entry,
            instance,
            device,
            physical_device,
            queue_families,
            debug_utils,
            surface_loader,
            surface,
            graphics_queue,
            present_queue,
        })
    }

    // Returns a borrow of device
    pub fn device(&self) -> &ash::Device {
        &self.device
    }

    /// Returns a new owned reference to device
    pub fn device_rc(&self) -> Rc<ash::Device> {
        Rc::clone(&self.device)
    }

    pub fn physical_device(&self) -> vk::PhysicalDevice {
        self.physical_device
    }

    pub fn queue_families(&self) -> &QueueFamilies {
        &self.queue_families
    }

    pub fn present_queue(&self) -> vk::Queue {
        self.present_queue
    }

    pub fn graphics_queue(&self) -> vk::Queue {
        self.graphics_queue
    }

    pub fn surface(&self) -> vk::SurfaceKHR {
        self.surface
    }

    pub fn surface_loader(&self) -> &Surface {
        &self.surface_loader
    }

    pub fn instance(&self) -> &ash::Instance {
        &self.instance
    }
}

impl Drop for VulkanContext {
    fn drop(&mut self) {
        info!("Destroying vulkan context");
        // Destroy the device
        device::destroy(&self.device);

        // Destroy debug utils if present
        if let Some((debug_utils, debug_messenger)) = self.debug_utils.take() {
            debug_utils::destroy(&debug_utils, debug_messenger)
        }

        surface::destroy(&self.surface_loader, self.surface);
        instance::destroy(&self.instance);
    }
}
