//! This module contains low level buffer helper functions
use std::{mem, rc::Rc};

use ash::vk;
use vk_mem::Allocator;

use crate::{commands::*, context::VulkanContext, device, Error};

#[derive(Clone, Copy, PartialEq, Eq)]
// Defines the type of a buffer
pub enum BufferType {
    /// Vertex buffer
    Vertex,
    /// 16 bit index buffer
    Index16,
    /// 32 bit index buffer
    Index32,
    // Uniform,
    // Instance,
}

#[derive(Clone, Copy, PartialEq, Eq)]
// Defines the expected usage pattern of a buffer
pub enum BufferUsage {
    /// Buffer data will be set once or rarely and used many times
    /// Uses temporary staging buffers and optimizes for GPU read access
    Staged,
    /// Buffer data will seldom be set but used many times
    /// Uses a persistent staging buffer and optimizes for GPU read access
    StagedPersistent,
}

/// Higher level construct abstracting buffer and buffer memory for index,
/// vertex and uniform use
/// buffer usage
pub struct Buffer {
    context: Rc<VulkanContext>,
    buffer: vk::Buffer,
    allocation: vk_mem::Allocation,
    _allocation_info: vk_mem::AllocationInfo,
    ty: BufferType,
    usage: BufferUsage,

    // If a staging buffer is persisted
    staging_buffer:
        Option<(vk::Buffer, vk_mem::Allocation, vk_mem::AllocationInfo)>,
}

impl Buffer {
    /// Creates a new buffer and fills it with vertex data using staging
    /// buffer
    pub fn new<T>(
        context: Rc<VulkanContext>,
        ty: BufferType, // TODO implement
        usage: BufferUsage,
        data: &[T],
    ) -> Result<Self, Error> {
        let size = data.len() * mem::size_of::<T>();

        // Calculate the buffer usage flags
        let vk_usage = match ty {
            BufferType::Vertex => vk::BufferUsageFlags::VERTEX_BUFFER,
            BufferType::Index16 | BufferType::Index32 => {
                vk::BufferUsageFlags::INDEX_BUFFER
            }
        } | match usage {
            BufferUsage::Staged | BufferUsage::StagedPersistent => {
                vk::BufferUsageFlags::TRANSFER_DST
            }
        };

        // Create the main GPU side buffer
        let buffer_info = vk::BufferCreateInfo::builder()
            .size(size as _)
            .usage(vk_usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        // Create the buffer
        let (buffer, allocation, allocation_info) =
            context.allocator().create_buffer(
                &buffer_info,
                &vk_mem::AllocationCreateInfo {
                    usage: vk_mem::MemoryUsage::GpuOnly,
                    ..Default::default()
                },
            )?;

        let mut buffer = Self {
            context,
            buffer,
            allocation,
            _allocation_info: allocation_info,
            ty,
            usage,
            staging_buffer: None,
        };

        // Fill the buffer with provided data
        buffer.fill(data, 0)?;
        Ok(buffer)
    }

    /// Update the buffer data by mapping memory and filling it using the
    /// provided closure
    /// `size`: Specifies the number of bytes to map
    /// `offset`: Specifies the offset in bytes into buffer to map
    pub fn write<F>(
        &mut self,
        size: vk::DeviceSize,
        offset: vk::DeviceSize,
        write_func: F,
    ) -> Result<(), Error>
    where
        F: FnOnce(*mut u8),
    {
        // Create a new or reuse staging buffer
        let (staging_buffer, staging_memory, staging_info) = match &self
            .staging_buffer
        {
            Some(v) => v,
            None => {
                self.staging_buffer =
                    Some(create_staging(self.context.allocator(), size as _)?);
                self.staging_buffer.as_ref().unwrap()
            }
        };

        let mapped = staging_info.get_mapped_data();

        // Use the write function to write into the mapped memory
        write_func(mapped);

        copy(
            self.context.graphics_queue(),
            self.context.transfer_pool(),
            *staging_buffer,
            self.buffer,
            size as _,
            offset,
        )?;

        // Destroy the staging buffer if non persistent usage
        if self.usage != BufferUsage::StagedPersistent {
            self.context
                .allocator()
                .destroy_buffer(*staging_buffer, &staging_memory)?;
            self.staging_buffer = None;
        }

        Ok(())
    }

    /// Fills the buffer  with provided data
    /// Uses write internally
    /// data cannot be larger in size than maximum buffer size
    pub fn fill<T>(
        &mut self,
        data: &[T],
        offset: vk::DeviceSize,
    ) -> Result<(), Error> {
        let size = data.len() * mem::size_of::<T>();
        self.write(size as _, offset, |mapped| unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr() as _, mapped, size)
        })
    }

    /// Returns the raw vk buffer
    pub fn buffer(&self) -> vk::Buffer {
        self.buffer
    }

    /// Returns the buffer type
    pub fn usage(&self) -> BufferUsage {
        self.usage
    }

    /// Returns the buffer type
    pub fn ty(&self) -> BufferType {
        self.ty
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        let allocator = self.context.allocator();
        allocator
            .destroy_buffer(self.buffer, &self.allocation)
            .unwrap();

        // Destroy persistent staging buffer
        if let Some((buffer, memory, _)) = self.staging_buffer.take() {
            allocator.destroy_buffer(buffer, &memory).unwrap();
        }
    }
}

/// Creates a suitable general purpose staging buffer
pub fn create_staging(
    allocator: &Allocator,
    size: vk::DeviceSize,
) -> Result<(vk::Buffer, vk_mem::Allocation, vk_mem::AllocationInfo), Error> {
    let (buffer, allocation, allocation_info) = allocator.create_buffer(
        &vk::BufferCreateInfo::builder()
            .size(size)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE),
        &vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::CpuToGpu,
            flags: vk_mem::AllocationCreateFlags::MAPPED,
            ..Default::default()
        },
    )?;

    Ok((buffer, allocation, allocation_info))
}

/// Copies the contents of one buffer to another
/// `commandpool`: pool to allocate transfer command buffer
/// Does not wait for operation to complete
pub fn copy(
    queue: vk::Queue,
    commandpool: &CommandPool,
    src_buffer: vk::Buffer,
    dst_buffer: vk::Buffer,
    size: vk::DeviceSize,
    offset: vk::DeviceSize,
) -> Result<(), Error> {
    let commandbuffer = commandpool.allocate(1)?.pop().unwrap();

    commandbuffer.begin(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)?;

    let region = vk::BufferCopy {
        src_offset: 0,
        dst_offset: offset,
        size,
    };

    commandbuffer.copy_buffer(src_buffer, dst_buffer, &[region]);

    commandbuffer.end()?;

    commandbuffer.submit(queue, &[], &[], vk::Fence::null(), &[])?;

    // Wait for operation to complete
    device::queue_wait_idle(commandpool.device(), queue)?;

    commandpool.free(commandbuffer);
    Ok(())
}
