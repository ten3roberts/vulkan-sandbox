use arrayvec::ArrayVec;
use ash::vk;
use log::info;
use ultraviolet::mat::*;

use crate::mesh_renderer::MeshRenderer;

use super::*;

use vulkan::context::*;
use vulkan::fence;
use vulkan::renderpass::*;
use vulkan::texture::*;
use vulkan::{device, semaphore};

use vulkan::swapchain;
use vulkan::{Buffer, BufferType, BufferUsage};

use vulkan::commands::*;
use vulkan::descriptors::*;
use vulkan::swapchain::*;
use vulkan::Framebuffer;

use glfw;
use std::{error::Error, mem, rc::Rc};

const FRAMES_IN_FLIGHT: usize = 2;
const MAX_OBJECTS: usize = 8192;

#[derive(Default)]
#[repr(C)]
struct ObjectData {
    mvp: Mat4,
}

/// Represents data needed to be duplicated for each swapchain image
struct PerFrameData {
    object_buffer: Buffer,
    commandpool: CommandPool,
    commandbuffer: CommandBuffer,
    framebuffer: Framebuffer,
    // The fence currently associated to this image_index
    image_in_flight: vk::Fence,
}

impl PerFrameData {
    fn new(
        context: Rc<VulkanContext>,
        renderpass: &RenderPass,
        color_attachment: &Texture,
        depth_attachment: &Texture,
        swapchain_image: &Texture,
        descriptor_layout_cache: &mut DescriptorLayoutCache,
        descriptor_allocator: &mut DescriptorAllocator,
    ) -> Result<Self, vulkan::Error> {
        let framebuffer = Framebuffer::new(
            context.device_ref(),
            &renderpass,
            &[color_attachment, depth_attachment, swapchain_image],
            swapchain_image.extent(),
        )?;

        let object_buffer = Buffer::new_uninit(
            context.clone(),
            BufferType::Storage,
            BufferUsage::MappedPersistent,
            (mem::size_of::<ObjectData>() * MAX_OBJECTS) as u64,
        )?;

        // Create and record command buffers
        let commandpool = CommandPool::new(
            context.device_ref(),
            context.queue_families().graphics().unwrap(),
            true,
            false,
        )?;

        let commandbuffer = commandpool.allocate(1)?.pop().unwrap();

        Ok(PerFrameData {
            object_buffer,
            framebuffer,
            commandpool,
            commandbuffer,
            image_in_flight: vk::Fence::null(),
        })
    }
}

pub struct MasterRenderer {
    swapchain_loader: Rc<ash::extensions::khr::Swapchain>,
    swapchain: Swapchain,

    in_flight_fences: ArrayVec<[vk::Fence; FRAMES_IN_FLIGHT]>,
    image_available_semaphores: ArrayVec<[vk::Semaphore; FRAMES_IN_FLIGHT]>,
    render_finished_semaphores: ArrayVec<[vk::Semaphore; FRAMES_IN_FLIGHT]>,

    renderpass: RenderPass,

    material: Rc<Material>,

    descriptor_layout_cache: DescriptorLayoutCache,
    descriptor_allocator: DescriptorAllocator,

    per_frame_data: ArrayVec<[PerFrameData; MAX_FRAMES]>,

    // The current frame-in-flight index
    current_frame: usize,
    should_resize: bool,

    // Multisampled color and depth renderpass attachments
    color_attachment: Texture,
    depth_attachment: Texture,

    // Drop context last
    context: Rc<VulkanContext>,

    mesh_renderer: MeshRenderer,
}

impl MasterRenderer {
    pub fn new(context: Rc<VulkanContext>, window: &glfw::Window) -> Result<Self, Box<dyn Error>> {
        let swapchain_loader = Rc::new(swapchain::create_loader(
            context.instance(),
            context.device(),
        ));

        let swapchain = Swapchain::new(context.clone(), Rc::clone(&swapchain_loader), &window)?;
        log::debug!("Created swapchain");
        log::debug!("Swapchain image format: {:?}", swapchain.image_format());

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

        let mut descriptor_layout_cache = DescriptorLayoutCache::new(context.device_ref());

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
                    &mut descriptor_layout_cache,
                    &mut descriptor_allocator,
                )
            })
            .collect::<Result<ArrayVec<[PerFrameData; MAX_FRAMES]>, _>>()?;

        let material_info = MaterialInfo {
            vertexshader: "data/shaders/default.vert.spv".into(),
            fragmentshader: "data/shaders/default.frag.spv".into(),
            albedo: "data/textures/uv.png".into(),
        };

        let mesh_renderer = MeshRenderer::new(
            context.clone(),
            &mut descriptor_layout_cache,
            &mut descriptor_allocator,
            swapchain.image_count() as usize,
        )?;

        let material = Material::new(
            context.clone(),
            &mut descriptor_layout_cache,
            &mut descriptor_allocator,
            material_info,
            swapchain.extent(),
            &renderpass,
            mesh_renderer.set_layout(),
        )?;

        let material = Rc::new(material);

        let master_renderer = MasterRenderer {
            context,
            swapchain_loader,
            swapchain,
            material,
            in_flight_fences,
            image_available_semaphores,
            render_finished_semaphores,
            renderpass,
            current_frame: 0,
            should_resize: false,
            descriptor_layout_cache,
            color_attachment,
            depth_attachment,
            descriptor_allocator,
            per_frame_data,
            mesh_renderer,
        };

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
                &mut self.descriptor_layout_cache,
                &mut self.descriptor_allocator,
            )?;

            self.per_frame_data.push(frame);
        }

        Ok(())
    }

    pub fn draw(
        &mut self,
        window: &glfw::Window,
        _dt: f32,
        camera: &Camera,
        scene: &mut Scene,
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
        let frame = &mut self.per_frame_data[image_index as usize];

        // Wait if previous frame is using this image
        if frame.image_in_flight != ash::vk::Fence::null() {
            fence::wait(device, &[frame.image_in_flight], true)?;
        }

        // Mark the image as being used by the frame in flight
        frame.image_in_flight = self.in_flight_fences[self.current_frame];

        frame.commandpool.reset(false)?;
        frame
            .commandbuffer
            .begin(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)?;

        frame.commandbuffer.begin_renderpass(
            &self.renderpass,
            &frame.framebuffer,
            self.swapchain.extent(),
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

        self.mesh_renderer
            .draw(&frame.commandbuffer, camera, image_index, scene)?;

        frame.commandbuffer.end_renderpass();
        frame.commandbuffer.end()?;

        // Present
        let wait_semaphores = [self.image_available_semaphores[self.current_frame]];

        let signal_semaphores = [self.render_finished_semaphores[self.current_frame]];

        // Reset fence before
        fence::reset(device, &[self.in_flight_fences[self.current_frame]])?;

        let view_projection = camera.projection() * camera.calculate_view();

        frame
            .object_buffer
            .write_slice(MAX_OBJECTS as u64, 0, |slice| {
                for (i, object) in scene.objects().iter().enumerate() {
                    let object_data = ObjectData {
                        mvp: view_projection
                            * Mat4::from_translation(object.position)
                            * Mat4::from_scale(0.1),
                    };

                    slice[i] = object_data;
                }
            })?;

        // Submit command buffers
        frame.commandbuffer.submit(
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

    /// Get a reference to the master renderer's material.
    pub fn material(&self) -> &Rc<Material> {
        &self.material
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
