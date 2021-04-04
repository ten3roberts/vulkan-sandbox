use arrayvec::ArrayVec;
use ash::vk;
use log::info;
use ultraviolet::mat::*;
use ultraviolet::vec::*;
use vulkan_sandbox::{
    mesh,
    vulkan::{
        device, fence, sampler, semaphore, Pipeline, PipelineLayout, Sampler, SamplerInfo,
        Texture, VulkanContext,
    },
};
use vulkan_sandbox::{mesh::Mesh, vulkan::RenderPass};

use vulkan_sandbox::vulkan;

use vulkan::swapchain;
use vulkan::{Buffer, BufferType, BufferUsage, VertexDesc};

use vulkan::commands::*;
use vulkan::descriptors;
use vulkan::descriptors::DescriptorPool;
use vulkan::swapchain::*;
use vulkan::Framebuffer;

use glfw;
use std::{error::Error, fs::File, rc::Rc};

const FRAMES_IN_FLIGHT: usize = 2;

#[derive(Default)]
#[repr(C)]
struct UniformBufferObject {
    mvp: Mat4,
}

/// Represents data needed to be duplicated for each swapchain image
struct PerFrameData {
    uniformbuffer: Buffer,
    descriptor_sets: Vec<vk::DescriptorSet>,
    commandbuffer: CommandBuffer,
    framebuffer: Framebuffer,
    // The fence currently associated to this image_index
    image_in_flight: vk::Fence,
}

impl PerFrameData {
    fn new(renderer: &MasterRenderer, index: usize) -> Result<Self, vulkan::Error> {
        let framebuffer = Framebuffer::new(
            renderer.context.device_ref(),
            &renderer.renderpass,
            &[
                renderer.swapchain.image_views()[index],
                renderer.swapchain.depth_attachment().image_view(),
            ],
            renderer.swapchain.extent(),
        )?;

        let uniformbuffer = Buffer::new(
            renderer.context.clone(),
            BufferType::Uniform,
            BufferUsage::MappedPersistent,
            &[UniformBufferObject {
                mvp: Mat4::from_translation(Vec3::new(0.0, 0.4, 0.0)),
            }],
        )?;

        let descriptor_sets = renderer.descriptor_pool.allocate(&[renderer.set_layout])?;

        descriptors::write(
            renderer.context.device(),
            descriptor_sets[0],
            &uniformbuffer,
            &renderer.texture,
            &renderer.sampler,
        );

        // Create and record command buffers
        let commandbuffer = renderer.commandpool.allocate(1)?.pop().unwrap();

        commandbuffer.begin(Default::default())?;

        commandbuffer.begin_renderpass(
            &renderer.renderpass,
            &framebuffer,
            renderer.swapchain.extent(),
            // TODO Autogenerate clear color based on one value
            &[
                vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.0, 0.0, 0.0, 0.0],
                    },
                },
                vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue {
                        depth: 1.0,
                        stencil: 0,
                    },
                },
            ],
        );

        commandbuffer.bind_pipeline(&renderer.pipeline);
        commandbuffer.bind_vertexbuffers(0, &[&renderer.mesh.vertex_buffer()]);
        commandbuffer.bind_descriptor_sets(&renderer.pipeline_layout, 0, &descriptor_sets);

        commandbuffer.bind_indexbuffer(&renderer.mesh.index_buffer(), 0);
        commandbuffer.draw_indexed(renderer.mesh.index_count(), 1, 0, 0, 0);
        commandbuffer.end_renderpass();
        commandbuffer.end()?;

        // Construct struct

        Ok(PerFrameData {
            uniformbuffer,
            descriptor_sets,
            framebuffer,
            commandbuffer,
            image_in_flight: vk::Fence::null(),
        })
    }
}

pub struct MasterRenderer {
    swapchain_loader: Rc<ash::extensions::khr::Swapchain>,
    swapchain: Swapchain,
    // Framebuffers to the actual swapchain images
    pipeline_layout: vulkan::PipelineLayout,
    pipeline: Pipeline,

    in_flight_fences: ArrayVec<[vk::Fence; FRAMES_IN_FLIGHT]>,
    image_available_semaphores: ArrayVec<[vk::Semaphore; FRAMES_IN_FLIGHT]>,
    render_finished_semaphores: ArrayVec<[vk::Semaphore; FRAMES_IN_FLIGHT]>,

    commandpool: CommandPool,
    renderpass: RenderPass,

    mesh: Mesh,

    set_layout: vk::DescriptorSetLayout,
    descriptor_pool: DescriptorPool,

    per_frame_data: Vec<PerFrameData>,

    // The current frame-in-flight index
    current_frame: usize,
    should_resize: bool,

    texture: Texture,
    sampler: Sampler,

    // Drop context last
    context: Rc<VulkanContext>,
}

impl MasterRenderer {
    pub fn new(context: Rc<VulkanContext>, window: &glfw::Window) -> Result<Self, Box<dyn Error>> {
        let swapchain_loader = Rc::new(swapchain::create_loader(
            context.instance(),
            context.device(),
        ));

        let swapchain = Swapchain::new(context.clone(), Rc::clone(&swapchain_loader), &window)?;

        let renderpass = RenderPass::new(
            context.device_ref(),
            swapchain.surface_format().format,
            swapchain.depth_format(),
        )?;

        let vs = File::open("./data/shaders/default.vert.spv")?;
        let fs = File::open("./data/shaders/default.frag.spv")?;

        let set_layout = descriptors::create_layout(context.device())?;
        let pipeline_layout = PipelineLayout::new(context.device_ref(), &[set_layout])?;

        let pipeline = Pipeline::new(
            context.device_ref(),
            vs,
            fs,
            swapchain.extent(),
            &pipeline_layout,
            &renderpass,
            mesh::Vertex::binding_description(),
            mesh::Vertex::attribute_descriptions(),
        )?;

        let descriptor_pool = DescriptorPool::new(
            context.device_ref(),
            swapchain.image_count(),
            swapchain.image_count(),
            swapchain.image_count(),
        )?;

        let image_available_semaphores = (0..FRAMES_IN_FLIGHT)
            .into_iter()
            .map(|_| semaphore::create(context.device()))
            .collect::<Result<_, _>>()?;

        let render_finished_semaphores = (0..FRAMES_IN_FLIGHT)
            .into_iter()
            .map(|_| semaphore::create(context.device()))
            .collect::<Result<_, _>>()?;

        let in_flight_fences = (0..FRAMES_IN_FLIGHT)
            .into_iter()
            .map(|_| fence::create(context.device(), true))
            .collect::<Result<_, _>>()?;

        let commandpool = CommandPool::new(
            context.device_ref(),
            context.queue_families().graphics().unwrap(),
            false,
            false,
        )?;

        // Simple quad
        let _vertices = [
            mesh::Vertex::new(
                Vec3::new(-0.5, -0.5, 0.0),
                Vec3::unit_x(),
                Vec2::new(1.0, 0.0),
            ),
            mesh::Vertex::new(
                Vec3::new(0.5, -0.5, 0.0),
                Vec3::unit_x(),
                Vec2::new(0.0, 0.0),
            ),
            mesh::Vertex::new(
                Vec3::new(0.5, 0.5, 0.0),
                Vec3::unit_x(),
                Vec2::new(0.0, 1.0),
            ),
            mesh::Vertex::new(
                Vec3::new(-0.5, 0.5, 0.0),
                Vec3::unit_x(),
                Vec2::new(1.0, 1.0),
            ),
            mesh::Vertex::new(
                Vec3::new(-0.5, -0.5, -0.2),
                Vec3::unit_x(),
                Vec2::new(1.0, 0.0),
            ),
            mesh::Vertex::new(
                Vec3::new(0.5, -0.5, -0.2),
                Vec3::unit_x(),
                Vec2::new(0.0, 0.0),
            ),
            mesh::Vertex::new(
                Vec3::new(0.5, 0.5, -0.2),
                Vec3::unit_x(),
                Vec2::new(0.0, 1.0),
            ),
            mesh::Vertex::new(
                Vec3::new(-0.5, 0.5, -0.2),
                Vec3::unit_x(),
                Vec2::new(1.0, 1.0),
            ),
        ];

        let _indices: [u32; 12] = [0, 1, 2, 2, 3, 0, 4, 5, 6, 6, 7, 4];

        let (document, buffers, _images) = gltf::import("./data/models/monkey.gltf")?;

        // let mesh = Mesh::new(context.clone(), &vertices, &indices)?;
        let mesh = Mesh::from_gltf(context.clone(), document.meshes().next().unwrap(), &buffers)?;

        let texture = Texture::load(context.clone(), "./data/textures/uv.png")?;

        let sampler_info = SamplerInfo {
            address_mode: sampler::AddressMode::REPEAT,
            mag_filter: sampler::FilterMode::LINEAR,
            min_filter: sampler::FilterMode::LINEAR,
            unnormalized_coordinates: false,
            anisotropy: 16.0,
            mip_levels: texture.mip_levels(),
        };

        let sampler = Sampler::new(context.clone(), sampler_info)?;

        let mut master_renderer = MasterRenderer {
            context,
            swapchain_loader,
            swapchain,
            pipeline_layout,
            pipeline,
            in_flight_fences,
            image_available_semaphores,
            render_finished_semaphores,
            commandpool,
            renderpass,
            current_frame: 0,
            should_resize: false,
            texture,
            sampler,
            mesh,
            set_layout,
            descriptor_pool,
            per_frame_data: Vec::new(),
        };

        master_renderer.per_frame_data = (0..master_renderer.swapchain.image_count())
            .map(|i| PerFrameData::new(&master_renderer, i as usize))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(master_renderer)
    }

    // Called when window is resized
    // Does not recreate the renderer immediately but waits for next frame
    pub fn on_resize(&mut self) {
        self.should_resize = true;
    }

    // Does the resizing
    fn resize(&mut self, window: &glfw::Window) -> Result<(), vulkan::Error> {
        log::debug!("Resizing");
        self.should_resize = false;

        device::wait_idle(self.context.device())?;

        let old_surface_format = self.swapchain.surface_format();

        // Recreate swapchain
        self.swapchain = Swapchain::new(
            self.context.clone(),
            Rc::clone(&self.swapchain_loader),
            window,
        )?;

        // Renderpass depends on swapchain surface format
        if old_surface_format != self.swapchain.surface_format() {
            info!("Surface format changed");
            self.renderpass = RenderPass::new(
                self.context.device_ref(),
                self.swapchain.surface_format().format,
                self.swapchain.depth_format(),
            )?;
        }

        let vs = File::open("./data/shaders/default.vert.spv")?;
        let fs = File::open("./data/shaders/default.frag.spv")?;

        self.pipeline = Pipeline::new(
            self.context.device_ref(),
            vs,
            fs,
            self.swapchain.extent(),
            &self.pipeline_layout,
            &self.renderpass,
            mesh::Vertex::binding_description(),
            mesh::Vertex::attribute_descriptions(),
        )?;

        self.commandpool.reset(false)?;

        self.descriptor_pool.reset()?;

        log::debug!("Recreating per frame data");
        self.per_frame_data = (0..self.swapchain.image_count())
            .map(|i| PerFrameData::new(&self, i as usize))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(())
    }

    pub fn draw(
        &mut self,
        window: &glfw::Window,
        elapsed: f32,
        _dt: f32,
    ) -> Result<(), vulkan::Error> {
        if self.should_resize {
            self.resize(window)?;
        }

        let device = self.context.device();

        // Wait for current_frame to not be in use
        fence::wait(device, &[self.in_flight_fences[self.current_frame]], true)?;

        // Acquire the next image from swapchain
        let image_index = match self
            .swapchain
            .next_image(self.image_available_semaphores[self.current_frame])
        {
            Ok(image_index) => image_index,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.on_resize();
                return Ok(());
            }

            Err(e) => return Err(e.into()),
        };

        // log::info!(
        //     "Drawing frame: currentFrame: {}, image_index: {}",
        //     self.current_frame,
        //     image_index
        // );

        // Extract data for this image in swapchain
        let data = &mut self.per_frame_data[image_index as usize];

        // Wait if previous frame is using this image
        if data.image_in_flight != ash::vk::Fence::null() {
            fence::wait(device, &[data.image_in_flight], true)?;
        }

        // Mark the image as being used by the frame in flight
        data.image_in_flight = self.in_flight_fences[self.current_frame];

        let wait_semaphores = [self.image_available_semaphores[self.current_frame]];

        let signal_semaphores = [self.render_finished_semaphores[self.current_frame]];

        // Reset fence before
        fence::reset(device, &[self.in_flight_fences[self.current_frame]])?;

        data.uniformbuffer.fill(
            0,
            &[UniformBufferObject {
                mvp: Mat4::from_translation(Vec3::new(elapsed.sin() * 0.5, 0.0, 0.5))
                    * Mat4::from_rotation_y(elapsed)
                    * Mat4::from_scale(0.25),
            }],
        )?;

        // Submit command buffers
        data.commandbuffer.submit(
            self.context.graphics_queue(),
            &wait_semaphores,
            &signal_semaphores,
            self.in_flight_fences[self.current_frame],
            &[ash::vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT],
        )?;

        let _suboptimal = match self.swapchain.present(
            self.context.present_queue(),
            &signal_semaphores,
            image_index,
        ) {
            Ok(image_index) => image_index,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.on_resize();
                return Ok(());
            }

            Err(e) => return Err(e.into()),
        };

        self.current_frame = (self.current_frame + 1) % FRAMES_IN_FLIGHT as usize;

        Ok(())
    }
}

impl Drop for MasterRenderer {
    fn drop(&mut self) {
        info!("Destroying master renderer");
        device::wait_idle(self.context.device()).unwrap();

        descriptors::destroy_layout(self.context.device(), self.set_layout);

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
