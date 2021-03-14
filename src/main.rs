use log::*;
use master_renderer::MasterRenderer;
use std::{error::Error, rc::Rc, thread, time::Duration};

// mod master_renderer;
use vulkan_sandbox::clock::*;
use vulkan_sandbox::logger;
use vulkan_sandbox::vulkan;

use vulkan::VulkanContext;

use glfw;

mod master_renderer;

fn main() -> Result<(), Box<dyn Error>> {
    logger::init();

    let mut glfw = glfw::init(glfw::FAIL_ON_ERRORS)?;

    // Dont initialize opengl context
    glfw.window_hint(glfw::WindowHint::ClientApi(glfw::ClientApiHint::NoApi));
    glfw.window_hint(glfw::WindowHint::Resizable(false));

    let (mut window, events) = glfw
        .create_window(800, 600, "Vulkan Window", glfw::WindowMode::Windowed)
        .expect("Failed to create window");

    window.set_all_polling(true);

    let context = Rc::new(VulkanContext::new(&glfw, &window)?);

    let mut master_renderer = MasterRenderer::new(context.clone(), &window)?;

    let clock = Clock::new();
    let mut frame_clock = Clock::new();
    let mut last_status = Clock::new();

    while !window.should_close() {
        let elapsed = clock.elapsed();
        let dt = frame_clock.reset();

        glfw.poll_events();

        for (_, event) in glfw::flush_messages(&events) {
            match event {
                glfw::WindowEvent::CursorPos(_, _) => {}
                glfw::WindowEvent::FramebufferSize(w, h) => {
                    info!("Resized: {}, {}", w, h);
                    master_renderer.on_resize();
                    thread::sleep(Duration::from_millis(100));
                    break;
                }
                _ => {
                    info!("Event: {:?}", event);
                }
            }
        }

        if last_status.elapsed().secs() > 1.0 {
            last_status.reset();
            log::info!(
                "Elapsed: {:?}\tFrametime: {:?}\tFramerate: {}",
                elapsed,
                dt,
                1.0 / dt.secs()
            );
        }

        master_renderer.draw(&window, elapsed.secs(), dt.secs())?;
    }

    Ok(())
}
