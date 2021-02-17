use log::*;
use std::{error::Error, fs::File};
use vulkan::commands::*;
use vulkan::framebuffer::*;
use vulkan::pipeline::*;
use vulkan::renderpass::*;
use vulkan::swapchain::*;

use vulkan::*;

use glfw;

const FRAMES_IN_FLIGHT: u32 = 2;

fn main() -> Result<(), Box<dyn Error>> {
    logger::init();

    let mut glfw = glfw::init(glfw::FAIL_ON_ERRORS)?;

    // Dont initialize opengl context
    glfw.window_hint(glfw::WindowHint::ClientApi(glfw::ClientApiHint::NoApi));

    let (mut window, events) = glfw
        .create_window(800, 600, "Vulkan Window", glfw::WindowMode::Windowed)
        .expect("Failed to create window");

    window.set_all_polling(true);

    // Setup vulkan
    let entry = entry::create()?;
    let instance = instance::create(&entry, &glfw, "Vulkan Application", "Custom")?;
    let (debug_utils, debug_messenger) = debug_utils::create(&entry, &instance)?;
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

    let swapchain_loader = swapchain::create_loader(&instance, &device);

    let vs = File::open("./data/shaders/default.vert.spv")?;
    let fs = File::open("./data/shaders/default.frag.spv")?;

    // Limit lifetime of swapchain
    {
        let swapchain = Swapchain::new(
            &device,
            &swapchain_loader,
            &window,
            &surface_loader,
            surface,
            physical_device,
            &queue_families,
        )?;

        let renderpass = RenderPass::new(&device, swapchain.surface_format().format)?;

        let pipeline_layout = PipelineLayout::new(&device)?;
        let pipeline = Pipeline::new(
            &device,
            vs,
            fs,
            swapchain.extent(),
            &pipeline_layout,
            &renderpass,
        )?;

        let framebuffers = swapchain
            .image_views()
            .iter()
            .map(|view| Framebuffer::new(&device, &renderpass, &[*view], swapchain.extent()))
            .collect::<Result<Vec<_>, _>>()?;

        info!("Framebuffer count: {:?}", framebuffers.len());

        let commandpool =
            CommandPool::new(&device, queue_families.graphics().unwrap(), false, false)?;

        let mut commandbuffers = commandpool.allocate(framebuffers.len() as _)?;

        for (i, commandbuffer) in commandbuffers.iter_mut().enumerate() {
            commandbuffer.begin()?;

            commandbuffer.begin_renderpass(&renderpass, &framebuffers[i], swapchain.extent());
            commandbuffer.bind_pipeline(&pipeline);
            commandbuffer.draw(3, 1, 0, 0);
            commandbuffer.end_renderpass();
            commandbuffer.end()?;
        }

        let image_available_semaphores = (0..FRAMES_IN_FLIGHT)
            .into_iter()
            .map(|_| semaphore::create(&device))
            .collect::<Result<Vec<_>, _>>()?;

        let render_finished_semaphores = (0..FRAMES_IN_FLIGHT)
            .into_iter()
            .map(|_| semaphore::create(&device))
            .collect::<Result<Vec<_>, _>>()?;

        let in_flight_fences = (0..FRAMES_IN_FLIGHT)
            .into_iter()
            .map(|_| fence::create(&device, true))
            .collect::<Result<Vec<_>, _>>()?;

        // Fences for images in flight
        let mut images_in_flight = swapchain
            .images()
            .iter()
            .map(|_| ash::vk::Fence::null())
            .collect::<Vec<_>>();

        // Game loop
        let mut current_frame = 0;

        while !window.should_close() {
            glfw.poll_events();

            for (_, event) in glfw::flush_messages(&events) {
                if let glfw::WindowEvent::CursorPos(_, _) = event {
                } else {
                    info!("Event: {:?}", event);
                }
            }

            fence::wait(&device, &[in_flight_fences[current_frame]], true)?;

            let image_index = swapchain.next_image(image_available_semaphores[current_frame])?;

            // Wait if previous frame is using this image
            if images_in_flight[image_index as usize] != ash::vk::Fence::null() {
                fence::wait(&device, &[images_in_flight[image_index as usize]], true)?;
            }

            // Mark the image as being used by the frame
            images_in_flight[image_index as usize] = in_flight_fences[current_frame];

            let wait_semaphores = [image_available_semaphores[current_frame]];
            let signal_semaphores = [render_finished_semaphores[current_frame]];

            // Reset fence before
            fence::reset(&device, &[in_flight_fences[current_frame]])?;

            // Submit command buffers
            commandbuffers[image_index as usize].submit(
                graphics_queue,
                &wait_semaphores,
                &signal_semaphores,
                in_flight_fences[current_frame],
                &[ash::vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT],
            )?;

            let _suboptimal = swapchain.present(present_queue, &signal_semaphores, image_index)?;
            device::wait_idle(&device)?;
            current_frame = (current_frame + 1) % FRAMES_IN_FLIGHT as usize;
        }

        image_available_semaphores
            .iter()
            .for_each(|s| semaphore::destroy(&device, *s));
        render_finished_semaphores
            .iter()
            .for_each(|s| semaphore::destroy(&device, *s));

        in_flight_fences
            .iter()
            .for_each(|f| fence::destroy(&device, *f));
    }

    device::destroy(device);
    debug_utils::destroy(&debug_utils, debug_messenger);
    surface::destroy(&surface_loader, surface);
    instance::destroy(instance);

    Ok(())
}
