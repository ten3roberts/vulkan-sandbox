use super::commands::CommandPool;
use super::*;
use ash::extensions::ext::DebugUtils;
use ash::extensions::khr::Surface;
use ash::vk;
use log::info;

use glfw::Glfw;
use std::rc::Rc;

use super::device::QueueFamilies;

pub struct VulkanContext {
    _entry: ash::Entry,
    instance: ash::Instance,
    device: Rc<ash::Device>,
    physical_device: vk::PhysicalDevice,
    queue_families: QueueFamilies,
    debug_utils: Option<(DebugUtils, vk::DebugUtilsMessengerEXT)>,

    surface_loader: Surface,
    surface: vk::SurfaceKHR,

    graphics_queue: vk::Queue,
    present_queue: vk::Queue,
    allocator: vk_mem::Allocator,

    /// CommandPool for allocatig transfer command buffers
    /// Wrap in option to drop early
    transfer_pool: Option<CommandPool>,

    limits: vk::PhysicalDeviceLimits,
    msaa_samples: vk::SampleCountFlags,
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
        let (device, pdevice_info) =
            device::create(&instance, &surface_loader, surface, instance::get_layers())?;
        log::debug!("Using device: {}", pdevice_info.name);

        // Get the physical device limits
        let limits = device::get_limits(&instance, pdevice_info.physical_device);

        let graphics_queue =
            device::get_queue(&device, pdevice_info.queue_families.graphics().unwrap(), 0);
        let present_queue =
            device::get_queue(&device, pdevice_info.queue_families.present().unwrap(), 0);

        let allocator_info = vk_mem::AllocatorCreateInfo {
            physical_device: pdevice_info.physical_device,
            device: (*device).clone(),
            instance: instance.clone(),
            flags: vk_mem::AllocatorCreateFlags::default(),
            preferred_large_heap_block_size: 0, // Use default
            frame_in_use_count: 0,
            heap_size_limits: None,
        };

        let allocator = vk_mem::Allocator::new(&allocator_info)?;

        let transfer_pool = CommandPool::new(
            device.clone(),
            pdevice_info.queue_families.graphics().unwrap(),
            true,
            true,
        )?;

        let msaa_samples = get_max_msaa_samples(
            limits.framebuffer_color_sample_counts & limits.sampled_image_color_sample_counts,
        );

        Ok(VulkanContext {
            _entry: entry,
            instance,
            device,
            physical_device: pdevice_info.physical_device,
            queue_families: pdevice_info.queue_families,
            debug_utils,
            surface_loader,
            surface,
            graphics_queue,
            present_queue,
            allocator,
            transfer_pool: Some(transfer_pool),
            limits,
            msaa_samples,
        })
    }

    // Returns a borrow of device
    pub fn device(&self) -> &ash::Device {
        &self.device
    }

    /// Returns a new owned reference to device
    pub fn device_ref(&self) -> Rc<ash::Device> {
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

    pub fn allocator(&self) -> &vk_mem::Allocator {
        &self.allocator
    }

    pub fn limits(&self) -> &vk::PhysicalDeviceLimits {
        &self.limits
    }

    /// Returns a commandpool that can be used to allocate for transfer
    /// operations
    pub fn transfer_pool(&self) -> &CommandPool {
        &self
            .transfer_pool
            .as_ref()
            .expect("Transfer pool is only None when dropped")
    }

    /// Returns the maximum number of samples for framebuffer color attachments
    pub fn msaa_samples(&self) -> vk::SampleCountFlags {
        self.msaa_samples
    }
}

impl Drop for VulkanContext {
    fn drop(&mut self) {
        info!("Destroying vulkan context");
        // Destroy the allocator
        self.allocator.destroy();

        // Destroy the transfer pool before device destruction
        self.transfer_pool.take();

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

fn get_max_msaa_samples(sample_counts: vk::SampleCountFlags) -> vk::SampleCountFlags {
    if sample_counts.contains(vk::SampleCountFlags::TYPE_64) {
        vk::SampleCountFlags::TYPE_64
    } else if sample_counts.contains(vk::SampleCountFlags::TYPE_32) {
        vk::SampleCountFlags::TYPE_32
    } else if sample_counts.contains(vk::SampleCountFlags::TYPE_16) {
        vk::SampleCountFlags::TYPE_16
    } else if sample_counts.contains(vk::SampleCountFlags::TYPE_8) {
        vk::SampleCountFlags::TYPE_8
    } else if sample_counts.contains(vk::SampleCountFlags::TYPE_4) {
        vk::SampleCountFlags::TYPE_4
    } else if sample_counts.contains(vk::SampleCountFlags::TYPE_2) {
        vk::SampleCountFlags::TYPE_2
    } else {
        vk::SampleCountFlags::TYPE_1
    }
}
