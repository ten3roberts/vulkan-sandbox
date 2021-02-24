use std::rc::Rc;

use crate::buffer::Buffer;
use crate::framebuffer::Framebuffer;
use crate::pipeline::Pipeline;
use crate::renderpass::RenderPass;
use crate::Error;
use arrayvec::ArrayVec;
use ash::version::DeviceV1_0;
use ash::vk;
use ash::Device;

/// Maximum number of bound vertex buffers
/// This is required to avoid dynamically allocating a list of buffers when
/// binding
pub const MAX_VB_BINDING: usize = 4;

pub struct CommandPool {
    device: Rc<Device>,
    commandpool: vk::CommandPool,
}

/// `transient`: Commandbuffers allocated are very shortlived
/// `reset`: Commandbuffers can be individually reset from pool
impl CommandPool {
    pub fn new(
        device: Rc<Device>,
        queue_family: u32,
        transient: bool,
        reset: bool,
    ) -> Result<Self, Error> {
        let flags = if transient {
            vk::CommandPoolCreateFlags::TRANSIENT
        } else {
            vk::CommandPoolCreateFlags::default()
        } | if reset {
            vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER
        } else {
            vk::CommandPoolCreateFlags::default()
        };

        let create_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(queue_family)
            .flags(flags);

        let commandpool =
            unsafe { device.create_command_pool(&create_info, None)? };

        Ok(CommandPool {
            device,
            commandpool,
        })
    }

    pub fn allocate(&self, count: u32) -> Result<Vec<CommandBuffer>, Error> {
        let alloc_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(self.commandpool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(count);

        // Allocate handles
        let raw = unsafe { self.device.allocate_command_buffers(&alloc_info)? };

        // Wrap handles
        let commandbuffers = raw
            .iter()
            .map(|commandbuffer| CommandBuffer {
                device: self.device.clone(),
                commandbuffer: *commandbuffer,
            })
            .collect::<Vec<_>>();

        Ok(commandbuffers)
    }

    // Resets all command buffers allocated from pool
    // `release`: Release all memory allocated back to the system, if
    // commandbuffers are to be rerecorded, this will need to once again
    // acquire memory
    pub fn reset(&self, release: bool) -> Result<(), Error> {
        let flags = if release {
            vk::CommandPoolResetFlags::RELEASE_RESOURCES
        } else {
            vk::CommandPoolResetFlags::default()
        };

        unsafe { self.device.reset_command_pool(self.commandpool, flags)? }
        Ok(())
    }

    // Frees a single commandbuffer
    // It is more efficient to reset the whole pool rather than freeing all
    // individually
    pub fn free(&self, commandbuffer: CommandBuffer) {
        unsafe {
            self.device.free_command_buffers(
                self.commandpool,
                &[commandbuffer.commandbuffer],
            )
        }
    }

    pub fn device(&self) -> &ash::Device {
        &self.device
    }
}

impl Drop for CommandPool {
    fn drop(&mut self) {
        unsafe { self.device.destroy_command_pool(self.commandpool, None) }
    }
}

pub struct CommandBuffer {
    device: Rc<Device>,
    commandbuffer: vk::CommandBuffer,
}

impl CommandBuffer {
    /// Starts recording of a commandbuffer
    pub fn begin(
        &self,
        flags: vk::CommandBufferUsageFlags,
    ) -> Result<(), Error> {
        let begin_info = vk::CommandBufferBeginInfo {
            s_type: vk::StructureType::COMMAND_BUFFER_BEGIN_INFO,
            p_next: std::ptr::null(),
            flags,
            p_inheritance_info: std::ptr::null(),
        };

        unsafe {
            self.device
                .begin_command_buffer(self.commandbuffer, &begin_info)?
        };

        Ok(())
    }

    // Ends recording of commandbuffer
    pub fn end(&self) -> Result<(), Error> {
        unsafe { self.device.end_command_buffer(self.commandbuffer)? };
        Ok(())
    }

    // Begins a renderpass
    pub fn begin_renderpass(
        &self,
        renderpass: &RenderPass,
        framebuffer: &Framebuffer,
        extent: vk::Extent2D,
    ) {
        let clear_values = [vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.0, 0.0, 0.1, 1.0],
            },
        }];
        let begin_info = vk::RenderPassBeginInfo {
            s_type: vk::StructureType::RENDER_PASS_BEGIN_INFO,
            p_next: std::ptr::null(),
            render_pass: renderpass.renderpass(),
            framebuffer: framebuffer.framebuffer(),
            render_area: vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: extent,
            },
            clear_value_count: clear_values.len() as _,
            p_clear_values: clear_values.as_ptr(),
        };

        unsafe {
            self.device.cmd_begin_render_pass(
                self.commandbuffer,
                &begin_info,
                vk::SubpassContents::INLINE,
            )
        }
    }

    // Ends current renderpass
    pub fn end_renderpass(&self) {
        unsafe { self.device.cmd_end_render_pass(self.commandbuffer) }
    }

    // Binds a graphics pipeline
    pub fn bind_pipeline(&self, pipeline: &Pipeline) {
        unsafe {
            self.device.cmd_bind_pipeline(
                self.commandbuffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline.pipeline(),
            )
        }
    }

    pub fn bind_vertexbuffers(
        &self,
        first_binding: u32,
        vertexbuffers: &[&Buffer],
    ) {
        let buffers: ArrayVec<[vk::Buffer; MAX_VB_BINDING]> =
            vertexbuffers.iter().map(|vb| vb.buffer()).collect();

        unsafe {
            self.device.cmd_bind_vertex_buffers(
                self.commandbuffer,
                first_binding,
                &buffers,
                &[0; MAX_VB_BINDING][0..buffers.len()],
            )
        }
    }

    // Issues a draw command using the currently bound resources
    pub fn draw(
        &self,
        vertex_count: u32,
        instance_count: u32,
        vertex_offset: u32,
        instance_offset: u32,
    ) {
        unsafe {
            self.device.cmd_draw(
                self.commandbuffer,
                vertex_count,
                instance_count,
                vertex_offset,
                instance_offset,
            )
        }
    }

    pub fn copy_buffer(
        &self,
        src: vk::Buffer,
        dst: vk::Buffer,
        regions: &[vk::BufferCopy],
    ) {
        unsafe {
            self.device
                .cmd_copy_buffer(self.commandbuffer, src, dst, regions)
        }
    }

    pub fn submit(
        &self,
        queue: vk::Queue,
        wait_semaphores: &[vk::Semaphore],
        signal_semaphores: &[vk::Semaphore],
        fence: vk::Fence,
        wait_stages: &[vk::PipelineStageFlags],
    ) -> Result<(), Error> {
        let submit_info = vk::SubmitInfo {
            s_type: vk::StructureType::SUBMIT_INFO,
            p_next: std::ptr::null(),
            wait_semaphore_count: wait_semaphores.len() as _,
            p_wait_semaphores: wait_semaphores.as_ptr(),
            p_wait_dst_stage_mask: wait_stages.as_ptr(),
            command_buffer_count: 1,
            p_command_buffers: &self.commandbuffer,
            signal_semaphore_count: signal_semaphores.len() as _,
            p_signal_semaphores: signal_semaphores.as_ptr(),
        };

        unsafe { self.device.queue_submit(queue, &[submit_info], fence) }?;

        Ok(())
    }
}
