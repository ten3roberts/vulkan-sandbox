use arrayvec::ArrayVec;
use ash::vk;
use log::info;
use ultraviolet::mat::*;
use ultraviolet::vec::*;
use vulkan::Error;
use vulkan::{buffer::Buffer, swapchain};
use vulkan::{
    buffer::{BufferType, BufferUsage},
    vertex::*,
};
use vulkan::{common_vertex::CommonVertex, context::VulkanContext};

use vulkan::commands::*;
use vulkan::descriptors;
use vulkan::descriptors::DescriptorPool;
use vulkan::framebuffer::*;
use vulkan::pipeline::*;
use vulkan::renderpass::*;
use vulkan::swapchain::*;
use vulkan::*;

use glfw;
use std::{fs::File, rc::Rc};

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
    fn new(renderer: &MasterRenderer, index: usize) -> Result<Self, Error> {
        let framebuffer = Framebuffer::new(
            renderer.context.device_ref(),
            &renderer.renderpass,
            &[renderer.swapchain.image_views()[index]],
            renderer.swapchain.extent(),
        )?;

        let uniformbuffer = Buffer::new(
            renderer.context.clone(),
            BufferType::Uniform,
            BufferUsage::MappedPersistent,
            &UniformBufferObject {
                mvp: Mat4::from_translation(Vec3::new(0.0, 0.4, 0.0)),
            },
        )?;

        let descriptor_sets =
            renderer.descriptor_pool.allocate(&[renderer.set_layout])?;

        descriptors::write(
            renderer.context.device(),
            descriptor_sets[0],
            &uniformbuffer,
        );

        // Create and record command buffers
        let commandbuffer = renderer.commandpool.allocate(1)?.pop().unwrap();

        commandbuffer.begin(Default::default())?;

        commandbuffer.begin_renderpass(
            &renderer.renderpass,
            &framebuffer,
            renderer.swapchain.extent(),
        );
        commandbuffer.bind_pipeline(&renderer.pipeline);
        commandbuffer.bind_vertexbuffers(0, &[&renderer.vertexbuffer]);
        commandbuffer.bind_descriptor_sets(
            &renderer.pipeline_layout,
            0,
            &descriptor_sets,
        );

        commandbuffer.bind_indexbuffer(&renderer.indexbuffer, 0);
        commandbuffer.draw_indexed(6, 1, 0, 0, 0);
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
    pipeline_layout: PipelineLayout,
    pipeline: Pipeline,

    in_flight_fences: ArrayVec<[vk::Fence; FRAMES_IN_FLIGHT]>,
    image_available_semaphores: ArrayVec<[vk::Semaphore; FRAMES_IN_FLIGHT]>,
    render_finished_semaphores: ArrayVec<[vk::Semaphore; FRAMES_IN_FLIGHT]>,

    commandpool: CommandPool,
    renderpass: RenderPass,

    vertexbuffer: Buffer,
    indexbuffer: Buffer,

    set_layout: vk::DescriptorSetLayout,
    descriptor_pool: DescriptorPool,

    per_frame_data: Vec<PerFrameData>,

    current_frame: usize,
    should_resize: bool,

    // Drop context last
    context: Rc<VulkanContext>,
}

impl MasterRenderer {
    pub fn new(
        context: Rc<VulkanContext>,
        window: &glfw::Window,
    ) -> Result<Self, Error> {
        let swapchain_loader = Rc::new(swapchain::create_loader(
            context.instance(),
            context.device(),
        ));

        let swapchain = Swapchain::new(
            context.clone(),
            Rc::clone(&swapchain_loader),
            &window,
        )?;

        let renderpass = RenderPass::new(
            context.device_ref(),
            swapchain.surface_format().format,
        )?;

        let vs = File::open("./data/shaders/default.vert.spv")?;
        let fs = File::open("./data/shaders/default.frag.spv")?;

        let set_layout = descriptors::create_layout(context.device())?;
        let pipeline_layout =
            PipelineLayout::new(context.device_ref(), &[set_layout])?;

        let pipeline = Pipeline::new(
            context.device_ref(),
            vs,
            fs,
            swapchain.extent(),
            &pipeline_layout,
            &renderpass,
            CommonVertex::binding_description(),
            CommonVertex::attribute_descriptions(),
        )?;

        let descriptor_pool = DescriptorPool::new(
            context.device_ref(),
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

        let vertices = [
            CommonVertex::new(
                Vec3::new(-0.5, -0.5, 0.0),
                Vec4::new(1.0, 0.0, 0.0, 0.0),
            ),
            CommonVertex::new(
                Vec3::new(0.5, -0.5, 0.0),
                Vec4::new(0.0, 1.0, 0.0, 0.0),
            ),
            CommonVertex::new(
                Vec3::new(0.5, 0.5, 0.0),
                Vec4::new(0.0, 0.0, 1.0, 0.0),
            ),
            CommonVertex::new(
                Vec3::new(-0.5, 0.5, 0.0),
                Vec4::new(0.0, 0.0, 1.0, 0.0),
            ),
        ];

        let vertexbuffer = Buffer::new(
            context.clone(),
            BufferType::Vertex,
            BufferUsage::Staged,
            &vertices,
        )?;

        let indices: [u16; 6] = [0, 1, 2, 2, 3, 0];

        let indexbuffer = Buffer::new(
            context.clone(),
            BufferType::Index16,
            BufferUsage::Staged,
            &indices,
        )?;

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
            vertexbuffer,
            indexbuffer,
            current_frame: 0,
            should_resize: false,

            set_layout,
            descriptor_pool,
            per_frame_data: Vec::new(),
        };

        let per_frame_data = (0..master_renderer.swapchain.image_count())
            .map(|i| PerFrameData::new(&master_renderer, i as usize))
            .collect::<Result<Vec<_>, _>>()?;

        master_renderer.per_frame_data = per_frame_data;

        Ok(master_renderer)
    }

    pub fn draw(
        &mut self,
        window: &glfw::Window,
        elapsed: f32,
    ) -> Result<(), Error> {
        if self.should_resize {
            self.resize(window)?;
        }

        let device = self.context.device();
        fence::wait(
            device,
            &[self.in_flight_fences[self.current_frame]],
            true,
        )?;

        let image_index = match self
            .swapchain
            .next_image(self.image_available_semaphores[self.current_frame])
        {
            Ok(image_index) => image_index,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.hint_resize();
                return Ok(());
            }

            Err(e) => return Err(e.into()),
        };

        let data = &mut self.per_frame_data[image_index as usize];

        // Wait if previous frame is using this image
        if data.image_in_flight != ash::vk::Fence::null() {
            fence::wait(device, &[data.image_in_flight], true)?;
        }

        // Mark the image as being used by the frame
        data.image_in_flight = self.in_flight_fences[self.current_frame];

        let wait_semaphores =
            [self.image_available_semaphores[self.current_frame]];
        let signal_semaphores =
            [self.render_finished_semaphores[self.current_frame]];

        // Reset fence before
        fence::reset(device, &[self.in_flight_fences[self.current_frame]])?;

        data.uniformbuffer.fill(
            0,
            &UniformBufferObject {
                mvp: Mat4::from_translation(Vec3::new(
                    elapsed.sin() * 0.5,
                    0.0,
                    0.0,
                )) * Mat4::from_rotation_z(elapsed * 2.0)
                    * Mat4::from_scale(elapsed.cos() * 0.25),
            },
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
                self.hint_resize();
                return Ok(());
            }

            Err(e) => return Err(e.into()),
        };

        device::wait_idle(device)?;
        self.current_frame =
            (self.current_frame + 1) % FRAMES_IN_FLIGHT as usize;

        Ok(())
    }

    pub fn hint_resize(&mut self) {
        self.should_resize = true;
    }

    fn resize(&mut self, window: &glfw::Window) -> Result<(), Error> {
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
            CommonVertex::binding_description(),
            CommonVertex::attribute_descriptions(),
        )?;

        self.commandpool.reset(false)?;
        // self.commandbuffers =
        //     self.commandpool.allocate(self.framebuffers.len() as _)?;

        // for (i, commandbuffer) in self.commandbuffers.iter().enumerate() {
        //     commandbuffer.begin(Default::default())?;

        // commandbuffer.begin_renderpass(
        //     &self.renderpass,
        //     &self.framebuffers[i],
        //     self.swapchain.extent(),
        // );

        // commandbuffer.bind_pipeline(&self.pipeline);
        // commandbuffer.bind_vertexbuffers(0, &[&self.vertexbuffer]);
        // commandbuffer.draw(3, 1, 0, 0);
        // commandbuffer.end_renderpass();
        // commandbuffer.end()?;
        // }

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
