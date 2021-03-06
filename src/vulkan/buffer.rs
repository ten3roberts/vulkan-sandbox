//! This module contains low level buffer helper functions
use std::{mem, rc::Rc};

use ash::vk;
use vk::DeviceSize;
use vk_mem::Allocator;

use super::{commands::*, context::VulkanContext, Error, Extent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// Defines the type of a buffer
pub enum BufferType {
    /// Vertex buffer
    Vertex,
    /// 16 bit index buffer
    Index16,
    /// 32 bit index buffer
    Index32,
    /// Uniform buffer
    Uniform,
    /// Storage buffer
    Storage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// Defines the expected usage pattern of a buffer
pub enum BufferUsage {
    /// Buffer data will be set once or rarely and frequently times
    /// Uses temporary staging buffers and optimizes for GPU read access
    Staged,
    /// Buffer data will seldom be set but frequently times
    /// Uses a persistent staging buffer and optimizes for GPU read access
    StagedPersistent,

    /// Buffer data is often updated and frequently used
    /// Uses temporarily mapped host memory
    Mapped,

    /// Buffer data is very often updated and frequently used
    /// Uses persistently mapped memory
    MappedPersistent,
}

/// Higher level construct abstracting buffer and buffer memory for index,
/// vertex and uniform use
/// buffer usage
pub struct Buffer {
    context: Rc<VulkanContext>,
    buffer: vk::Buffer,
    allocation: vk_mem::Allocation,
    allocation_info: vk_mem::AllocationInfo,

    // Maximum allocated size of the buffer
    size: DeviceSize,
    ty: BufferType,
    usage: BufferUsage,

    // If a staging buffer is persisted
    staging_buffer: Option<(vk::Buffer, vk_mem::Allocation, vk_mem::AllocationInfo)>,
}

impl Buffer {
    /// Creates a new buffer with size and uninitialized contents.
    pub fn new_uninit(
        context: Rc<VulkanContext>,
        ty: BufferType,
        usage: BufferUsage,
        size: DeviceSize,
    ) -> Result<Self, Error> {
        // Calculate the buffer usage flags
        let vk_usage = match ty {
            BufferType::Vertex => vk::BufferUsageFlags::VERTEX_BUFFER,
            BufferType::Index16 | BufferType::Index32 => vk::BufferUsageFlags::INDEX_BUFFER,
            BufferType::Uniform => vk::BufferUsageFlags::UNIFORM_BUFFER,
            BufferType::Storage => vk::BufferUsageFlags::STORAGE_BUFFER,
        } | match usage {
            BufferUsage::Mapped | BufferUsage::MappedPersistent => vk::BufferUsageFlags::default(),
            BufferUsage::Staged | BufferUsage::StagedPersistent => {
                vk::BufferUsageFlags::TRANSFER_DST
            }
        };

        let memory_usage = match usage {
            BufferUsage::Staged | BufferUsage::StagedPersistent => vk_mem::MemoryUsage::GpuOnly,
            BufferUsage::Mapped | BufferUsage::MappedPersistent => vk_mem::MemoryUsage::CpuToGpu,
        };

        let flags = match usage {
            BufferUsage::MappedPersistent => vk_mem::AllocationCreateFlags::MAPPED,
            _ => vk_mem::AllocationCreateFlags::NONE,
        };

        // Create the main GPU side buffer
        let buffer_info = vk::BufferCreateInfo::builder()
            .size(size as _)
            .usage(vk_usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let allocator = context.allocator();

        // Create the buffer
        let (buffer, allocation, allocation_info) = allocator.create_buffer(
            &buffer_info,
            &vk_mem::AllocationCreateInfo {
                usage: memory_usage,
                flags,
                ..Default::default()
            },
        )?;

        Ok(Self {
            size,
            context,
            buffer,
            allocation,
            allocation_info,
            ty,
            usage,
            staging_buffer: None,
        })
    }
    /// Creates a new buffer and fills it with vertex data using staging
    /// buffer. Buffer will be the same size as provided data.
    pub fn new<T>(
        context: Rc<VulkanContext>,
        ty: BufferType,
        usage: BufferUsage,
        data: &[T],
    ) -> Result<Self, Error> {
        let size = (mem::size_of::<T>() * data.len()) as DeviceSize;

        let mut buffer = Self::new_uninit(context, ty, usage, size)?;
        // Fill the buffer with provided data
        buffer.fill(0, data)?;
        Ok(buffer)
    }

    /// Update the buffer data by mapping memory and filling it using the
    /// provided closure.
    /// `len`: Specifies the number of items of T to map into slice. (is ignored with persistent
    /// usage).
    /// `offset`: Specifies the offset in items T into buffer to map.
    pub fn write_slice<T, F>(
        &mut self,
        len: DeviceSize,
        offset: DeviceSize,
        write_func: F,
    ) -> Result<(), Error>
    where
        F: FnOnce(&mut [T]),
    {
        let size = len * mem::size_of::<T>() as u64;
        self.write(size, offset * mem::size_of::<T>() as u64, |ptr| {
            write_func(unsafe { std::slice::from_raw_parts_mut(ptr as *mut T, len as usize) })
        })
    }

    /// Update the buffer data by mapping memory and filling it using the
    /// provided closure
    /// `size`: Specifies the number of bytes to map (is ignored with persistent
    /// usage)
    /// `offset`: Specifies the offset in bytes into buffer to map
    pub fn write<F>(
        &mut self,
        size: DeviceSize,
        offset: DeviceSize,
        write_func: F,
    ) -> Result<(), Error>
    where
        F: FnOnce(*mut u8),
    {
        if size > self.size {
            return Err(Error::BufferOverflow {
                size,
                max_size: self.size,
            });
        }
        match self.usage {
            BufferUsage::Staged => self.write_staged(size, offset, write_func),
            BufferUsage::StagedPersistent => self.write_staged_persistent(offset, write_func),
            BufferUsage::Mapped => self.write_mapped(offset, write_func),
            BufferUsage::MappedPersistent => {
                self.write_mapped_persistent(size, offset, write_func)
            }
        }
    }

    // Updates memory by mapping and unmapping
    // Will map the whole buffer
    fn write_mapped_persistent<F>(
        &self,
        size: DeviceSize,
        offset: DeviceSize,
        write_func: F,
    ) -> Result<(), Error>
    where
        F: FnOnce(*mut u8),
    {
        let allocator = self.context.allocator();
        let mapped = self.allocation_info.get_mapped_data();

        unsafe {
            write_func(mapped.offset(offset as _));
        }

        allocator.flush_allocation(&self.allocation, offset as _, size as _)?;

        Ok(())
    }

    // Updates memory by mapping and unmapping
    // Will map the whole buffer
    fn write_mapped<F>(&self, offset: DeviceSize, write_func: F) -> Result<(), Error>
    where
        F: FnOnce(*mut u8),
    {
        let allocator = self.context.allocator();
        let mapped = allocator.map_memory(&self.allocation)?;

        unsafe {
            write_func(mapped.offset(offset as _));
        }

        allocator.unmap_memory(&self.allocation)?;
        Ok(())
    }

    fn write_staged<F>(
        &self,
        size: DeviceSize,
        offset: DeviceSize,
        write_func: F,
    ) -> Result<(), Error>
    where
        F: FnOnce(*mut u8),
    {
        let allocator = self.context.allocator();
        // Create a new or reuse staging buffer
        let (staging_buffer, staging_allocation, staging_info) =
            create_staging(allocator, size as _, true)?;

        let mapped = staging_info.get_mapped_data();

        // Use the write function to write into the mapped memory
        write_func(mapped);

        copy(
            self.context.transfer_pool(),
            self.context.graphics_queue(),
            staging_buffer,
            self.buffer,
            size as _,
            offset,
        )?;

        // Destroy the staging buffer
        allocator.destroy_buffer(staging_buffer, &staging_allocation)?;

        Ok(())
    }

    fn write_staged_persistent<F>(
        &mut self,
        offset: DeviceSize,
        write_func: F,
    ) -> Result<(), Error>
    where
        F: FnOnce(*mut u8),
    {
        let allocator = self.context.allocator();

        let (staging_buffer, staging_memory, _) = match &self.staging_buffer {
            Some(v) => v,
            // Create persistent staging buffer
            None => {
                self.staging_buffer = Some(create_staging(allocator, self.size, false)?);
                self.staging_buffer.as_ref().unwrap()
            }
        };

        // Map the staging buffer
        let mapped = allocator.map_memory(&staging_memory)?;

        // Use the write function to write into the mapped memory
        write_func(mapped);

        copy(
            self.context.transfer_pool(),
            self.context.graphics_queue(),
            *staging_buffer,
            self.buffer,
            self.size as _,
            offset,
        )?;

        // Unmap but keep staging buffer
        allocator.unmap_memory(&staging_memory)?;
        Ok(())
    }

    /// Fills the buffer  with provided data
    /// Uses write internally
    /// data cannot be larger in size than maximum buffer size
    pub fn fill<T: Sized>(&mut self, offset: DeviceSize, data: &[T]) -> Result<(), Error> {
        let size = mem::size_of::<T>() * data.len();

        self.write(size as _, offset, |mapped| unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr() as *const T as *const u8, mapped, size)
        })
    }

    pub fn size(&self) -> DeviceSize {
        self.size
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

impl AsRef<vk::Buffer> for Buffer {
    fn as_ref(&self) -> &vk::Buffer {
        &self.buffer
    }
}

impl Into<vk::Buffer> for &Buffer {
    fn into(self) -> vk::Buffer {
        self.buffer
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
            allocator.unmap_memory(&memory).unwrap();
            allocator.destroy_buffer(buffer, &memory).unwrap();
        }
    }
}

/// Creates a suitable general purpose staging buffer
pub fn create_staging(
    allocator: &Allocator,
    size: DeviceSize,
    mapped: bool,
) -> Result<(vk::Buffer, vk_mem::Allocation, vk_mem::AllocationInfo), Error> {
    let (buffer, allocation, allocation_info) = allocator.create_buffer(
        &vk::BufferCreateInfo::builder()
            .size(size)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE),
        &vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::CpuToGpu,
            flags: if mapped {
                vk_mem::AllocationCreateFlags::MAPPED
            } else {
                vk_mem::AllocationCreateFlags::NONE
            },
            ..Default::default()
        },
    )?;

    Ok((buffer, allocation, allocation_info))
}

/// Copies the contents of one buffer to another
/// `commandpool`: pool to allocate transfer command buffer
/// Does not wait for operation to complete
pub fn copy(
    commandpool: &CommandPool,
    queue: vk::Queue,
    src_buffer: vk::Buffer,
    dst_buffer: vk::Buffer,
    size: DeviceSize,
    offset: DeviceSize,
) -> Result<(), Error> {
    let region = vk::BufferCopy {
        src_offset: 0,
        dst_offset: offset,
        size,
    };

    commandpool.single_time_command(queue, |commandbuffer| {
        commandbuffer.copy_buffer(src_buffer, dst_buffer, &[region]);
    })
}

pub fn copy_to_image(
    commandpool: &CommandPool,
    queue: vk::Queue,
    buffer: vk::Buffer,
    image: vk::Image,
    layout: vk::ImageLayout,
    extent: Extent,
) -> Result<(), Error> {
    let region = vk::BufferImageCopy {
        buffer_offset: 0,
        buffer_row_length: 0,
        buffer_image_height: 0,
        image_subresource: vk::ImageSubresourceLayers {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            mip_level: 0,
            base_array_layer: 0,
            layer_count: 1,
        },
        image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
        image_extent: vk::Extent3D {
            width: extent.width,
            height: extent.height,
            depth: 1,
        },
    };

    commandpool.single_time_command(queue, |commandbuffer| {
        commandbuffer.copy_buffer_image(buffer, image, layout, &[region])
    })
}
