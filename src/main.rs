use log::*;
use master_renderer::MasterRenderer;
use std::{error::Error, rc::Rc, time::Instant};

// mod master_renderer;
use vulkan::context::VulkanContext;
use vulkan::*;

use glfw;

mod master_renderer;

fn main() -> Result<(), Box<dyn Error>> {
    logger::init();

    let mut glfw = glfw::init(glfw::FAIL_ON_ERRORS)?;

    // Dont initialize opengl context
    glfw.window_hint(glfw::WindowHint::ClientApi(glfw::ClientApiHint::NoApi));

    let (mut window, events) = glfw
        .create_window(800, 600, "Vulkan Window", glfw::WindowMode::Windowed)
        .expect("Failed to create window");

    window.set_all_polling(true);

    let context = Rc::new(VulkanContext::new(&glfw, &window)?);

    let mut master_renderer = MasterRenderer::new(context.clone(), &window)?;

    let init = Instant::now();
    while !window.should_close() {
        let now = Instant::now();
        let elapsed = (now - init).as_secs_f32();

        glfw.poll_events();

        for (_, event) in glfw::flush_messages(&events) {
            match event {
                glfw::WindowEvent::CursorPos(_, _) => {}
                glfw::WindowEvent::FramebufferSize(w, h) => {
                    info!("Resized: {}, {}", w, h);
                    master_renderer.hint_resize();
                    break;
                }
                _ => {
                    info!("Event: {:?}", event);
                }
            }
        }

        master_renderer.draw(&window, elapsed)?;
    }

    Ok(())
}
