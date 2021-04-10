use log::*;
use master_renderer::MasterRenderer;
use std::{error::Error, rc::Rc, thread, time::Duration};
use ultraviolet::Vec3;

// mod master_renderer;
use vulkan_sandbox::logger;
use vulkan_sandbox::vulkan;
use vulkan_sandbox::{camera::Camera, clock::*};

use vulkan::VulkanContext;

use glfw::{self, Action, Key, WindowEvent};

mod master_renderer;

fn main() -> Result<(), Box<dyn Error>> {
    logger::init();

    let mut glfw = glfw::init(glfw::FAIL_ON_ERRORS)?;

    // Dont initialize opengl context
    glfw.window_hint(glfw::WindowHint::ClientApi(glfw::ClientApiHint::NoApi));
    glfw.window_hint(glfw::WindowHint::Resizable(true));

    let (mut window, events) = glfw
        .create_window(800, 600, "Vulkan Window", glfw::WindowMode::Windowed)
        .expect("Failed to create window");

    window.set_all_polling(true);

    let context = Rc::new(VulkanContext::new(&glfw, &window)?);

    let mut master_renderer = MasterRenderer::new(context.clone(), &window)?;

    let clock = Clock::new();
    let mut frame_clock = Clock::new();
    let mut last_status = Clock::new();

    let aspect = 800.0 / 600.0;
    let mut perspective_camera =
        Camera::perspective(Vec3::new(0.0, 0.0, 10.0), 1.0, 800.0 / 600.0, 0.1, 1000.0);
    let mut orthograpic_camera =
        Camera::orthographic(Vec3::new(0.5, 0.0, 100.0), aspect * 8.0, 8.0, 0.1, 1000.0);

    let mut camera = &mut perspective_camera;

    while !window.should_close() {
        let elapsed = clock.elapsed();
        let dt = frame_clock.reset();

        glfw.poll_events();

        for (_, event) in glfw::flush_messages(&events) {
            match event {
                WindowEvent::Key(Key::F1, _, Action::Release, _) => {
                    camera = &mut perspective_camera
                }
                WindowEvent::Key(Key::F2, _, Action::Release, _) => {
                    camera = &mut orthograpic_camera
                }
                WindowEvent::CursorPos(_, _) => {}
                WindowEvent::FramebufferSize(w, h) => {
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

        camera.position.y = (elapsed.secs() * 0.25).sin() * 5.0;

        if last_status.elapsed().secs() > 1.0 {
            last_status.reset();
            log::info!(
                "Elapsed: {:?}\tFrametime: {:?}\tFramerate: {}",
                elapsed,
                dt,
                1.0 / dt.secs()
            );
        }

        master_renderer.draw(&window, elapsed.secs(), dt.secs(), &camera)?;
    }

    Ok(())
}
