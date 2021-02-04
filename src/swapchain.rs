use super::device::QueueFamilies;
use ash::extensions::khr::Surface;
pub use ash::extensions::khr::Swapchain as SwapchainLoader;
use ash::vk::{self, SurfaceKHR};
use ash::Device;
use ash::Instance;
use std::cmp;

use crate::{image_view, Error};

#[derive(Debug)]
pub struct SwapchainSupport {
    pub capabilities: vk::SurfaceCapabilitiesKHR,
    pub formats: Vec<vk::SurfaceFormatKHR>,
    pub present_modes: Vec<vk::PresentModeKHR>,
}

pub fn query_support(
    surface_loader: &Surface,
    surface: SurfaceKHR,
    physical_device: vk::PhysicalDevice,
) -> Result<SwapchainSupport, Error> {
    let capabilities = unsafe {
        surface_loader.get_physical_device_surface_capabilities(physical_device, surface)?
    };

    let formats =
        unsafe { surface_loader.get_physical_device_surface_formats(physical_device, surface)? };

    let present_modes = unsafe {
        surface_loader.get_physical_device_surface_present_modes(physical_device, surface)?
    };

    Ok(SwapchainSupport {
        capabilities,
        formats,
        present_modes,
    })
}

pub fn pick_format(formats: &[vk::SurfaceFormatKHR]) -> vk::SurfaceFormatKHR {
    for surface_format in formats {
        // Preferred surface_format
        if surface_format.format == vk::Format::B8G8R8_SRGB
            && surface_format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
        {
            return *surface_format;
        }
    }

    return formats[0];
}

/// Picks a present mode
/// If `preferred` is available, it is used
/// Otherwise, FIFO is returned
pub fn pick_present_mode(
    modes: &[vk::PresentModeKHR],
    preferred: vk::PresentModeKHR,
) -> vk::PresentModeKHR {
    for mode in modes {
        // Preferred surface_format
        if *mode == preferred {
            return *mode;
        }
    }

    return vk::PresentModeKHR::FIFO;
}

pub fn pick_extent(
    window: &glfw::Window,
    capabilities: &vk::SurfaceCapabilitiesKHR,
) -> vk::Extent2D {
    // The extent of the surface needs to match exactly
    if capabilities.current_extent.width != std::u32::MAX {
        return capabilities.current_extent;
    }

    // Freely choose extent based on window and min-max capabilities
    let (width, height) = window.get_framebuffer_size();

    let width = cmp::max(
        capabilities.min_image_extent.width,
        cmp::min(capabilities.max_image_extent.width, width as u32),
    );

    let height = cmp::max(
        capabilities.min_image_extent.height,
        cmp::min(capabilities.max_image_extent.height, height as u32),
    );

    vk::Extent2D { width, height }
}

pub fn create_loader(instance: &Instance, device: &Device) -> SwapchainLoader {
    SwapchainLoader::new(instance, device)
}

/// High level swapchain representation
/// Implements Drop
pub struct Swapchain<'a> {
    device: &'a Device,
    swapchain_loader: &'a SwapchainLoader,
    swapchain_khr: vk::SwapchainKHR,
    images: Vec<vk::Image>,
    image_views: Vec<vk::ImageView>,
    extent: vk::Extent2D,
    surface_format: vk::SurfaceFormatKHR,
}

impl<'a> Swapchain<'a> {
    pub fn new(
        device: &'a Device,
        swapchain_loader: &'a SwapchainLoader,
        window: &glfw::Window,
        surface_loader: &Surface,
        surface: vk::SurfaceKHR,
        physical_device: vk::PhysicalDevice,
        queue_families: &QueueFamilies,
    ) -> Result<Self, Error> {
        let support = query_support(surface_loader, surface, physical_device)?;

        let surface_format = pick_format(&support.formats);
        let present_mode = pick_present_mode(&support.present_modes, vk::PresentModeKHR::MAILBOX);
        let extent = pick_extent(window, &support.capabilities);

        // Use one more image than the minumum supported
        let mut image_count = support.capabilities.min_image_count + 1;

        // Make sure max image count isn't exceeded
        if support.capabilities.max_image_count != 0 {
            image_count = cmp::min(image_count, support.capabilities.max_image_count);
        }

        // The full set
        let queue_family_indices = [
            queue_families.graphics().unwrap(),
            queue_families.present().unwrap(),
        ];

        // Decide sharing mode depending on if graphics == present
        let (sharing_mode, queue_family_indices): (vk::SharingMode, &[u32]) =
            if queue_families.graphics() == queue_families.present() {
                (vk::SharingMode::EXCLUSIVE, &[])
            } else {
                (vk::SharingMode::CONCURRENT, &queue_family_indices)
            };

        let create_info = vk::SwapchainCreateInfoKHR::builder()
            .surface(surface)
            .min_image_count(image_count)
            .image_format(surface_format.format)
            .image_color_space(surface_format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            // For now, render directly to the images
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(sharing_mode)
            .queue_family_indices(queue_family_indices)
            .pre_transform(support.capabilities.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true)
            .old_swapchain(vk::SwapchainKHR::null());

        let swapchain_khr = unsafe { swapchain_loader.create_swapchain(&create_info, None)? };

        let images = unsafe { swapchain_loader.get_swapchain_images(swapchain_khr)? };

        // Create image views
        let image_views = images
            .iter()
            .map(|image| image_view::create(device, *image, surface_format.format))
            .collect::<Result<_, _>>()?;

        Ok(Swapchain {
            device,
            swapchain_khr,
            images,
            image_views,
            surface_format,
            swapchain_loader,
            extent,
        })
    }

    // Getters

    pub fn images(&self) -> &[vk::Image] {
        &self.images
    }

    pub fn surface_format(&self) -> vk::SurfaceFormatKHR {
        self.surface_format
    }

    pub fn extent(&self) -> vk::Extent2D {
        self.extent
    }
}

impl<'a> Drop for Swapchain<'a> {
    fn drop(&mut self) {
        // Destroy image views
        self.image_views
            .iter()
            .for_each(|view| image_view::destroy(self.device, *view));

        // Destroy the swapchain
        unsafe {
            self.swapchain_loader
                .destroy_swapchain(self.swapchain_khr, None);
        };
    }
}
