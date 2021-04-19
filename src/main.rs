use log::*;
use master_renderer::MasterRenderer;
use rand::prelude::*;
use std::{error::Error, rc::Rc, thread, time::Duration};
use ultraviolet::Vec3;

use vulkan_sandbox::camera::Camera;
use vulkan_sandbox::clock::*;
use vulkan_sandbox::vulkan;

use vulkan::pipeline::*;
use vulkan::VertexDesc;

use resources::*;
use vulkan_sandbox::*;

use vulkan::VulkanContext;

use glfw::{self, Action, Key, WindowEvent};

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

    let clock = Clock::new();
    let mut frame_clock = Clock::new();
    let mut last_status = Clock::new();
    let mut last_spawn = Clock::new();

    let aspect = 800.0 / 600.0;
    let mut perspective_camera =
        Camera::perspective(Vec3::new(0.0, 0.0, 10.0), 1.0, 800.0 / 600.0, 0.1, 1000.0);
    let mut orthographic_camera =
        Camera::orthographic(Vec3::new(0.5, 0.0, 100.0), aspect * 8.0, 8.0, 0.1, 1000.0);

    let mut camera = &mut perspective_camera;

    let mut scene = Scene::new();
    let mut master_renderer = MasterRenderer::new(context.clone(), &window)?;

    let mut resources = ResourceManager::new(context.clone());

    let (document, buffers, _) = gltf::import("./data/models/cube.gltf")?;
    resources.load_mesh("cube", document.meshes().next().unwrap(), &buffers)?;

    let (document, buffers, _) = gltf::import("./data/models/monkey.gltf")?;
    resources.load_mesh("monkey", document.meshes().next().unwrap(), &buffers)?;

    let default_pass = Pipeline::new(
        context.device_ref(),
        &mut master_renderer.descriptor_layout_cache,
        &master_renderer.renderpass,
        PipelineInfo {
            vertexshader: "./data/shaders/default.vert.spv".into(),
            fragmentshader: "./data/shaders/default.frag.spv".into(),
            vertex_binding: mesh::Vertex::binding_description(),
            vertex_attributes: mesh::Vertex::attribute_descriptions(),
            samples: context.msaa_samples(),
            extent: master_renderer.swapchain.extent(),
            subpass: 0,
            ..Default::default()
        },
    )?;

    resources.load_effect("default", vec![default_pass])?;
    resources.load_texture("uv", "./data/textures/uv.png")?;

    resources.load_material(
        "default",
        MaterialInfo {
            albedo: "uv".into(),
            effect: "default".into(),
        },
    )?;

    let positions = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(4.0, 1.0, 0.0),
        Vec3::new(-2.0, 0.0, 3.0),
        Vec3::new(2.0, 0.0, 3.0),
    ];

    for position in &positions {
        let position = *position;
        scene.add(Object {
            material: resources.material("default")?,
            mesh: resources.mesh("monkey")?,
            position,
        });
    }

    let mut rng = rand::thread_rng();

    while !window.should_close() {
        let elapsed = clock.elapsed();
        let dt = frame_clock.reset();

        glfw.poll_events();

        scene.objects_mut()[0].position.x = elapsed.secs().sin();

        for (_, event) in glfw::flush_messages(&events) {
            match event {
                WindowEvent::Key(Key::F1, _, Action::Release, _) => {
                    camera = &mut perspective_camera
                }
                WindowEvent::Key(Key::F2, _, Action::Release, _) => {
                    camera = &mut orthographic_camera
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

        camera.position.y = (elapsed.secs() * 0.25).sin() * 2.0;

        if scene.objects().len() < 5000 {
            last_spawn.reset();
            let position = Vec3::new(
                rng.gen_range(-15.0..15.0),
                rng.gen_range(-15.0..15.0),
                rng.gen_range(-15.0..15.0),
            );

            // log::info!("Adding: {:?}", position);

            scene.add(Object {
                mesh: resources.mesh("cube")?,
                material: resources.material("default")?,
                position,
            })
        }

        if last_status.elapsed().secs() > 1.0 {
            last_status.reset();
            log::info!(
                "Elapsed: {:?}\tFrametime: {:?}\tFramerate: {}\t Objects: {:?}",
                elapsed,
                dt,
                1.0 / dt.secs(),
                scene.objects().len(),
            );
        }

        master_renderer.draw(&window, dt.secs(), &camera, &mut scene, &resources)?;
    }

    std::mem::drop(master_renderer);

    Ok(())
}
