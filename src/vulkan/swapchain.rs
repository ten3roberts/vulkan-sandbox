use ash::extensions::khr::Surface;
pub use ash::extensions::khr::Swapchain as SwapchainLoader;
use ash::vk::{self, SurfaceKHR};
use ash::Device;
use ash::Instance;
use std::{cmp, rc::Rc};

use super::{Error, Extent, Texture, TextureInfo, VulkanContext};

/// The maximum number of images in the swapchain. Actual image count may be less but never more.
/// This is to allow inline allocation of per swapchain image resources through `ArrayVec`.
pub const MAX_FRAMES: usize = 5;

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

fn pick_format(formats: &[vk::SurfaceFormatKHR]) -> vk::SurfaceFormatKHR {
    for surface_format in formats {
        // Preferred surface_format
        if surface_format.format == vk::Format::B8G8R8A8_SRGB
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
fn pick_present_mode(
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

fn pick_extent(window: &glfw::Window, capabilities: &vk::SurfaceCapabilitiesKHR) -> Extent {
    // The extent of the surface needs to match exactly
    if capabilities.current_extent.width != std::u32::MAX {
        return capabilities.current_extent.into();
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

    (width, height).into()
}

pub fn create_loader(instance: &Instance, device: &Device) -> SwapchainLoader {
    SwapchainLoader::new(instance, device)
}

/// High level swapchain representation
/// Implements Drop
pub struct Swapchain {
    swapchain_loader: Rc<SwapchainLoader>,
    swapchain_khr: vk::SwapchainKHR,
    images: Vec<Texture>,
    extent: Extent,
    surface_format: vk::SurfaceFormatKHR,
}

impl Swapchain {
    pub fn new(
        context: Rc<VulkanContext>,
        swapchain_loader: Rc<SwapchainLoader>,
        window: &glfw::Window,
    ) -> Result<Self, Error> {
        let support = query_support(
            context.surface_loader(),
            context.surface(),
            context.physical_device(),
        )?;

        // Use one more image than the minumum supported
        let mut image_count = (support.capabilities.min_image_count + 1).min(MAX_FRAMES as u32);

        // Make sure max image count isn't exceeded
        if support.capabilities.max_image_count != 0 {
            image_count = cmp::min(image_count, support.capabilities.max_image_count);
        }

        // The full set
        let queue_family_indices = [
            context.queue_families().graphics().unwrap(),
            context.queue_families().present().unwrap(),
        ];

        // Decide sharing mode depending on if graphics == present
        let (sharing_mode, queue_family_indices): (vk::SharingMode, &[u32]) =
            if context.queue_families().graphics() == context.queue_families().present() {
                (vk::SharingMode::EXCLUSIVE, &[])
            } else {
                (vk::SharingMode::CONCURRENT, &queue_family_indices)
            };

        let surface_format = pick_format(&support.formats);

        let present_mode = pick_present_mode(&support.present_modes, vk::PresentModeKHR::IMMEDIATE);

        let extent = pick_extent(window, &support.capabilities);

        let create_info = vk::SwapchainCreateInfoKHR::builder()
            .surface(context.surface())
            .min_image_count(image_count)
            .image_format(surface_format.format)
            .image_color_space(surface_format.color_space)
            .image_extent(extent.into())
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

        let image_info = TextureInfo {
            extent,
            mip_levels: 1,
            usage: super::TextureUsage::ColorAttachment,
            format: surface_format.format,
            samples: vk::SampleCountFlags::TYPE_1,
        };

        let images = images
            .iter()
            .map(|image| Texture::from_image(context.clone(), image_info, *image, None))
            .collect::<Result<_, _>>()?;

        Ok(Swapchain {
            swapchain_khr,
            images,
            surface_format,
            swapchain_loader,
            extent,
        })
    }

    pub fn next_image(&self, semaphore: vk::Semaphore) -> Result<u32, vk::Result> {
        let (image_index, _) = unsafe {
            self.swapchain_loader.acquire_next_image(
                self.swapchain_khr,
                std::u64::MAX,
                semaphore,
                vk::Fence::null(),
            )?
        };

        Ok(image_index)
    }

    pub fn present(
        &self,
        queue: vk::Queue,
        wait_semaphores: &[vk::Semaphore],
        image_index: u32,
    ) -> Result<bool, vk::Result> {
        let present_info = vk::PresentInfoKHR {
            s_type: vk::StructureType::PRESENT_INFO_KHR,
            p_next: std::ptr::null(),
            wait_semaphore_count: wait_semaphores.len() as _,
            p_wait_semaphores: wait_semaphores.as_ptr(),
            swapchain_count: 1,
            p_swapchains: &self.swapchain_khr,
            p_image_indices: &image_index,
            p_results: std::ptr::null_mut(),
        };
        let suboptimal = unsafe { self.swapchain_loader.queue_present(queue, &present_info)? };

        Ok(suboptimal)
    }

    /// Returns the number of image in the swapchain. The same as `color_attachments`.len()
    pub fn image_count(&self) -> u32 {
        self.images.len() as u32
    }

    pub fn image_format(&self) -> vk::Format {
        self.surface_format.format
    }

    pub fn surface_format(&self) -> vk::SurfaceFormatKHR {
        self.surface_format
    }

    pub fn extent(&self) -> Extent {
        self.extent
    }

    /// Get a reference to a swapchain image by index
    pub fn image(&self, index: usize) -> &Texture {
        &self.images[index]
    }
    /// Get a reference to the swapchain's images
    pub fn images(&self) -> &Vec<Texture> {
        &self.images
    }
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        // Destroy the swapchain
        unsafe {
            self.swapchain_loader
                .destroy_swapchain(self.swapchain_khr, None);
        };
    }
}
