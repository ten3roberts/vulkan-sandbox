use log::*;
use std::error::Error;

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

    let _graphics_queue = device::get_queue(&device, queue_families.graphics().unwrap(), 0);
    let _present_queue = device::get_queue(&device, queue_families.present().unwrap(), 0);

    let swapchain_loader = swapchain::create_loader(&instance, &device);

    // Limit lifetime of swapchain
    {
        let swapchain = swapchain::Swapchain::new(
            &device,
            &swapchain_loader,
            &window,
            &surface_loader,
            surface,
            physical_device,
            &queue_families,
        );

        while !window.should_close() {
            glfw.poll_events();

            for (_, event) in glfw::flush_messages(&events) {
                if let glfw::WindowEvent::CursorPos(_, _) = event {
                } else {
                    info!("Event: {:?}", event);
                }
            }
        }
    }

    device::destroy(device);
    debug_utils::destroy(&debug_utils, debug_messenger);
    surface::destroy(&surface_loader, surface);
    instance::destroy(instance);

    Ok(())
}
