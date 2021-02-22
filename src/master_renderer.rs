use ash::vk;
use log::info;
use vulkan::context::VulkanContext;
use vulkan::swapchain;
use vulkan::Error;

use vulkan::commands::*;
use vulkan::framebuffer::*;
use vulkan::pipeline::*;
use vulkan::renderpass::*;
use vulkan::swapchain::*;
use vulkan::*;

use glfw::{self, Glfw};
use std::{fs::*, rc::Rc};

const FRAMES_IN_FLIGHT: u32 = 2;

pub struct MasterRenderer {
    swapchain_loader: Rc<ash::extensions::khr::Swapchain>,
    swapchain: Swapchain,
    // Framebuffers to the actual swapchain images
    framebuffers: Vec<Framebuffer>,
    pipeline: Pipeline,

    in_flight_fences: Vec<vk::Fence>,
    image_available_semaphores: Vec<vk::Semaphore>,
    render_finished_semaphores: Vec<vk::Semaphore>,
    images_in_flight: Vec<vk::Fence>,

    commandpool: CommandPool,
    commandbuffers: Vec<CommandBuffer>,
    renderpass: RenderPass,

    current_frame: usize,
    context: Rc<VulkanContext>,
}

impl MasterRenderer {
    pub fn new(context: Rc<VulkanContext>, window: &glfw::Window) -> Result<Self, Error> {
        let swapchain_loader = Rc::new(swapchain::create_loader(
            context.instance(),
            context.device(),
        ));

        let swapchain = Swapchain::new(
            context.device_rc(),
            Rc::clone(&swapchain_loader),
            &window,
            context.surface_loader(),
            context.surface(),
            context.physical_device(),
            context.queue_families(),
        )?;

        let renderpass = RenderPass::new(context.device_rc(), swapchain.surface_format().format)?;

        let vs = File::open("./data/shaders/default.vert.spv")?;
        let fs = File::open("./data/shaders/default.frag.spv")?;

        let pipeline_layout = PipelineLayout::new(context.device_rc())?;
        let pipeline = Pipeline::new(
            context.device_rc(),
            vs,
            fs,
            swapchain.extent(),
            &pipeline_layout,
            &renderpass,
        )?;

        let framebuffers = swapchain
            .image_views()
            .iter()
            .map(|view| {
                Framebuffer::new(
                    context.device_rc(),
                    &renderpass,
                    &[*view],
                    swapchain.extent(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        let image_available_semaphores = (0..FRAMES_IN_FLIGHT)
            .into_iter()
            .map(|_| semaphore::create(context.device()))
            .collect::<Result<Vec<_>, _>>()?;

        let render_finished_semaphores = (0..FRAMES_IN_FLIGHT)
            .into_iter()
            .map(|_| semaphore::create(context.device()))
            .collect::<Result<Vec<_>, _>>()?;

        let in_flight_fences = (0..FRAMES_IN_FLIGHT)
            .into_iter()
            .map(|_| fence::create(context.device(), true))
            .collect::<Result<Vec<_>, _>>()?;

        // Fences for images in flight
        let images_in_flight = swapchain
            .images()
            .iter()
            .map(|_| ash::vk::Fence::null())
            .collect::<Vec<_>>();

        let commandpool = CommandPool::new(
            context.device_rc(),
            context.queue_families().graphics().unwrap(),
            false,
            false,
        )?;

        let commandbuffers = commandpool.allocate(framebuffers.len() as _)?;

        for (i, commandbuffer) in commandbuffers.iter().enumerate() {
            commandbuffer.begin()?;

            commandbuffer.begin_renderpass(&renderpass, &framebuffers[i], swapchain.extent());
            commandbuffer.bind_pipeline(&pipeline);
            commandbuffer.draw(3, 1, 0, 0);
            commandbuffer.end_renderpass();
            commandbuffer.end()?;
        }

        Ok(MasterRenderer {
            context,
            swapchain_loader,
            swapchain,
            framebuffers,
            pipeline,
            in_flight_fences,
            image_available_semaphores,
            render_finished_semaphores,
            images_in_flight,
            commandpool,
            commandbuffers,
            renderpass,
            current_frame: 0,
        })
    }

    pub fn draw(&mut self) -> Result<(), Error> {
        let device = self.context.device();
        fence::wait(device, &[self.in_flight_fences[self.current_frame]], true)?;

        let image_index = self
            .swapchain
            .next_image(self.image_available_semaphores[self.current_frame])?;

        // Wait if previous frame is using this image
        if self.images_in_flight[image_index as usize] != ash::vk::Fence::null() {
            fence::wait(device, &[self.images_in_flight[image_index as usize]], true)?;
        }

        // Mark the image as being used by the frame
        self.images_in_flight[image_index as usize] = self.in_flight_fences[self.current_frame];

        let wait_semaphores = [self.image_available_semaphores[self.current_frame]];
        let signal_semaphores = [self.render_finished_semaphores[self.current_frame]];

        // Reset fence before
        fence::reset(device, &[self.in_flight_fences[self.current_frame]])?;

        // Submit command buffers
        self.commandbuffers[image_index as usize].submit(
            self.context.graphics_queue(),
            &wait_semaphores,
            &signal_semaphores,
            self.in_flight_fences[self.current_frame],
            &[ash::vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT],
        )?;

        let _suboptimal = self.swapchain.present(
            self.context.present_queue(),
            &signal_semaphores,
            image_index,
        )?;

        device::wait_idle(device)?;
        self.current_frame = (self.current_frame + 1) % FRAMES_IN_FLIGHT as usize;
        Ok(())
    }
}

impl Drop for MasterRenderer {
    fn drop(&mut self) {
        info!("Destroying master renderer");

        self.image_available_semaphores
            .iter()
            .for_each(|s| semaphore::destroy(&self.context.device(), *s));

        self.render_finished_semaphores
            .iter()
            .for_each(|s| semaphore::destroy(&self.context.device(), *s));

        self.in_flight_fences
            .iter()
            .for_each(|f| fence::destroy(&self.context.device(), *f));
    }
}
