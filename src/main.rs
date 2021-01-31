use log::*;
use std::error::Error;

use vulkan::*;

use glfw;

fn main() -> Result<(), Box<dyn Error>> {
    logger::init();
    info!("Info");
    debug!("Debug");
    warn!("Warning");
    error!("Error");
    trace!("Trace");

    let mut glfw = glfw::init(glfw::FAIL_ON_ERRORS)?;

    // Dont initialize opengl context
    glfw.window_hint(glfw::WindowHint::ClientApi(glfw::ClientApiHint::NoApi));

    let (mut window, events) = glfw
        .create_window(800, 600, "Vulkan Window", glfw::WindowMode::Windowed)
        .expect("Failed to create window");

    window.set_all_polling(true);

    // Seup vulkan
    let entry = entry::create()?;
    let instance = instance::create(&entry, &glfw, "Vulkan Application", "Custom")?;

    info!("Created vulkan instance");

    while !window.should_close() {
        glfw.poll_events();

        for (_, event) in glfw::flush_messages(&events) {
            if let glfw::WindowEvent::CursorPos(_, _) = event {
            } else {
                println!("Event: {:?}", event);
            }
        }
    }

    instance::destroy(instance);

    Ok(())
}
