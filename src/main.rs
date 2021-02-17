use log::*;
use std::{error::Error, fs::File};
use vulkan::commands::*;
use vulkan::framebuffer::*;
use vulkan::pipeline::*;
use vulkan::renderpass::*;
use vulkan::swapchain::*;

use vulkan::*;

use glfw;

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

        let image_available = semaphore::create(&device)?;
        let render_finished = semaphore::create(&device)?;

        // Game loop
        while !window.should_close() {
            // commandbuffers[image_index].
            glfw.poll_events();

            for (_, event) in glfw::flush_messages(&events) {
                if let glfw::WindowEvent::CursorPos(_, _) = event {
                } else {
                    info!("Event: {:?}", event);
                }
            }

            let image_index = swapchain.next_image(image_available)?;

            // Submit command buffers
            commandbuffers[image_index as usize].submit(
                graphics_queue,
                &[image_available],
                &[render_finished],
                &[ash::vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT],
            )?;

            let _suboptimal = swapchain.present(present_queue, &[render_finished], image_index)?;
            device::wait_idle(&device)?;
        }

        semaphore::destroy(&device, image_available);
        semaphore::destroy(&device, render_finished);
    }

    device::destroy(device);
    debug_utils::destroy(&debug_utils, debug_messenger);
    surface::destroy(&surface_loader, surface);
    instance::destroy(instance);

    Ok(())
}
