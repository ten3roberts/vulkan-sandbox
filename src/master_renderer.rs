use arrayvec::ArrayVec;
use ash::vk::{self, ShaderStageFlags};
use log::info;
use ultraviolet::mat::*;
use ultraviolet::vec::*;
use vulkan_sandbox::vulkan::pipeline::*;
use vulkan_sandbox::vulkan::renderpass::*;
use vulkan_sandbox::vulkan::sampler;
use vulkan_sandbox::vulkan::sampler::*;
use vulkan_sandbox::vulkan::texture::*;
use vulkan_sandbox::vulkan::VulkanContext;
use vulkan_sandbox::vulkan::{fence, Extent};
use vulkan_sandbox::{
    mesh,
    vulkan::{device, semaphore},
};
use vulkan_sandbox::{mesh::Mesh, vulkan::RenderPass};

use vulkan_sandbox::vulkan;

use vulkan::swapchain;
use vulkan::{Buffer, BufferType, BufferUsage, VertexDesc};

use vulkan::commands::*;
use vulkan::descriptors;
use vulkan::descriptors::*;
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
    set: vk::DescriptorSet,
    commandbuffer: CommandBuffer,
    framebuffer: Framebuffer,
    // The fence currently associated to this image_index
    image_in_flight: vk::Fence,
    set_layout: vk::DescriptorSetLayout,
}

impl PerFrameData {
    fn new(
        context: Rc<VulkanContext>,
        renderpass: &RenderPass,
        color_attachment: &Texture,
        depth_attachment: &Texture,
        swapchain_image: &Texture,
        descriptor_cache: &mut DescriptorLayoutCache,
        descriptor_allocator: &mut DescriptorAllocator,
        texture: &Texture,
        sampler: &Sampler,
        commandpool: &CommandPool,
    ) -> Result<Self, vulkan::Error> {
        let device = context.device();

        let framebuffer = Framebuffer::new(
            context.device_ref(),
            &renderpass,
            &[color_attachment, depth_attachment, swapchain_image],
            swapchain_image.extent(),
        )?;

        let uniformbuffer = Buffer::new(
            context.clone(),
            BufferType::Uniform,
            BufferUsage::MappedPersistent,
            &[UniformBufferObject {
                mvp: Mat4::from_translation(Vec3::new(0.0, 0.4, 0.0)),
            }],
        )?;

        let mut set = Default::default();
        let mut set_layout = Default::default();

        descriptors::DescriptorBuilder::new()
            .bind_uniform_buffer(0, ShaderStageFlags::VERTEX, &uniformbuffer)
            .bind_combined_image_sampler(1, ShaderStageFlags::FRAGMENT, &texture, &sampler)
            .build(device, descriptor_cache, descriptor_allocator, &mut set)?
            .layout(descriptor_cache, &mut set_layout)?;

        // Create and record command buffers
        let commandbuffer = commandpool.allocate(1)?.pop().unwrap();

        Ok(PerFrameData {
            uniformbuffer,
            set,
            framebuffer,
            commandbuffer,
            image_in_flight: vk::Fence::null(),
            set_layout,
        })
    }

    fn record(
        &self,
        pipeline: &Pipeline,
        pipeline_layout: &PipelineLayout,
        renderpass: &RenderPass,
        mesh: &Mesh,
        extent: Extent,
    ) -> Result<(), vulkan::Error> {
        let commandbuffer = &self.commandbuffer;

        commandbuffer.begin(Default::default())?;

        commandbuffer.begin_renderpass(
            &renderpass,
            &self.framebuffer,
            extent,
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

        commandbuffer.bind_pipeline(pipeline);
        commandbuffer.bind_vertexbuffers(0, &[&mesh.vertex_buffer()]);
        commandbuffer.bind_descriptor_sets(&pipeline_layout, 0, &[self.set]);

        commandbuffer.bind_indexbuffer(&mesh.index_buffer(), 0);
        commandbuffer.draw_indexed(mesh.index_count(), 1, 0, 0, 0);
        commandbuffer.end_renderpass();
        commandbuffer.end()?;
        Ok(())
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

    descriptor_cache: DescriptorLayoutCache,
    descriptor_allocator: DescriptorAllocator,

    per_frame_data: Vec<PerFrameData>,

    // The current frame-in-flight index
    current_frame: usize,
    should_resize: bool,

    // Multisampled color and depth renderpass attachments
    color_attachment: Texture,
    depth_attachment: Texture,

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

        let color_attachment = Texture::new(
            context.clone(),
            TextureInfo {
                extent: swapchain.extent(),
                mip_levels: 1,
                usage: TextureUsage::ColorAttachment,
                format: swapchain.image_format(),
                samples: context.msaa_samples(),
            },
        )?;

        let depth_attachment = Texture::new(
            context.clone(),
            TextureInfo {
                extent: swapchain.extent(),
                mip_levels: 1,
                usage: TextureUsage::DepthAttachment,
                format: Format::D32_SFLOAT,
                samples: context.msaa_samples(),
            },
        )?;

        let renderpass = create_renderpass(
            context.device_ref(),
            &color_attachment,
            &depth_attachment,
            swapchain.image_format(),
        )?;

        let vs = File::open("./data/shaders/default.vert.spv")?;
        let fs = File::open("./data/shaders/default.frag.spv")?;

        let mut descriptor_cache = DescriptorLayoutCache::new(context.device_ref());

        let mut descriptor_allocator = DescriptorAllocator::new(context.device_ref(), 2);

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

        let per_frame_data = swapchain
            .images()
            .iter()
            .map(|swapchain_image| {
                PerFrameData::new(
                    context.clone(),
                    &renderpass,
                    &color_attachment,
                    &depth_attachment,
                    swapchain_image,
                    &mut descriptor_cache,
                    &mut descriptor_allocator,
                    &texture,
                    &sampler,
                    &commandpool,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        let set_layout = per_frame_data[0].set_layout;

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
            context.msaa_samples(),
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
            current_frame: 0,
            should_resize: false,
            texture,
            sampler,
            mesh,
            descriptor_cache,
            color_attachment,
            depth_attachment,
            descriptor_allocator,
            per_frame_data,
        };

        master_renderer.record()?;

        Ok(master_renderer)
    }

    // Called when window is resized
    // Does not recreate the renderer immediately but waits for next frame
    pub fn on_resize(&mut self) {
        self.should_resize = true;
    }

    fn record(&mut self) -> Result<(), vulkan::Error> {
        self.commandpool.reset(false)?;
        self.per_frame_data
            .iter()
            .map(|frame| {
                frame.record(
                    &self.pipeline,
                    &self.pipeline_layout,
                    &self.renderpass,
                    &self.mesh,
                    self.swapchain.extent(),
                )
            })
            .collect::<Result<(), _>>()?;
        Ok(())
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

        self.color_attachment = Texture::new(
            self.context.clone(),
            TextureInfo {
                extent: self.swapchain.extent(),
                mip_levels: 1,
                usage: TextureUsage::ColorAttachment,
                format: self.swapchain.image_format(),
                samples: self.context.msaa_samples(),
            },
        )?;

        self.depth_attachment = Texture::new(
            self.context.clone(),
            TextureInfo {
                extent: self.swapchain.extent(),
                mip_levels: 1,
                usage: TextureUsage::DepthAttachment,
                format: Format::D32_SFLOAT,
                samples: self.context.msaa_samples(),
            },
        )?;

        // Renderpass depends on swapchain surface format
        if old_surface_format != self.swapchain.surface_format() {
            info!("Surface format changed");
            self.renderpass = create_renderpass(
                self.context.device_ref(),
                &self.color_attachment,
                &self.depth_attachment,
                self.swapchain.image_format(),
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
            self.context.msaa_samples(),
        )?;

        self.commandpool.reset(false)?;

        self.descriptor_allocator.reset()?;

        log::debug!("Recreating per frame data");
        self.per_frame_data.clear();
        for swapchain_image in self.swapchain.images() {
            let frame = PerFrameData::new(
                self.context.clone(),
                &self.renderpass,
                &self.color_attachment,
                &self.depth_attachment,
                swapchain_image,
                &mut self.descriptor_cache,
                &mut self.descriptor_allocator,
                &self.texture,
                &self.sampler,
                &self.commandpool,
            )?;

            self.per_frame_data.push(frame);
        }

        self.record()?;

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

fn create_renderpass(
    device: Rc<ash::Device>,
    color_attachment: &Texture,
    depth_attachment: &Texture,
    swapchain_format: vk::Format,
) -> Result<RenderPass, vulkan::Error> {
    let renderpass_info = RenderPassInfo {
        attachments: &[
            // Color attachment
            AttachmentInfo::from_texture(
                color_attachment,
                LoadOp::CLEAR,
                StoreOp::STORE,
                ImageLayout::UNDEFINED,
                ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            ),
            // Depth attachment
            AttachmentInfo::from_texture(
                depth_attachment,
                LoadOp::CLEAR,
                StoreOp::DONT_CARE,
                ImageLayout::UNDEFINED,
                ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            ),
            // Present attachment
            AttachmentInfo {
                usage: vulkan::TextureUsage::ColorAttachment,
                format: swapchain_format,
                samples: vk::SampleCountFlags::TYPE_1,
                load: LoadOp::DONT_CARE,
                store: StoreOp::STORE,
                initial_layout: ImageLayout::UNDEFINED,
                final_layout: ImageLayout::PRESENT_SRC_KHR,
            },
        ],
        subpasses: &[SubpassInfo {
            color_attachments: &[AttachmentReference {
                attachment: 0,
                layout: ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            }],
            resolve_attachments: &[AttachmentReference {
                attachment: 2,
                layout: ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            }],
            depth_attachment: Some(AttachmentReference {
                attachment: 1,
                layout: ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            }),
        }],
    };

    let renderpass = RenderPass::new(device, &renderpass_info)?;
    Ok(renderpass)
}
