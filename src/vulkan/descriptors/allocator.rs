use ash::vk;
use ash::Device;
use ash::{version::DeviceV1_0, vk::DescriptorType};
use std::rc::Rc;

use crate::vulkan::Error;

pub use vk::DescriptorSetLayout;

struct Pool {
    pool: vk::DescriptorPool,
    set_count: u32,
    allocated: u32,
}

impl Pool {
    /// Creates a new fresh pool
    pub fn new(
        device: &Device,
        set_count: u32,
        sizes: &[(DescriptorType, f32)],
    ) -> Result<Self, Error> {
        let sizes = sizes
            .iter()
            .map(|(ty, rel)| vk::DescriptorPoolSize {
                ty: *ty,
                descriptor_count: (rel * set_count as f32) as u32,
            })
            .collect::<Vec<_>>();

        let create_info = vk::DescriptorPoolCreateInfo {
            max_sets: set_count,
            pool_size_count: sizes.len() as u32,
            p_pool_sizes: sizes.as_ptr(),
            ..Default::default()
        };

        let pool = unsafe { device.create_descriptor_pool(&create_info, None)? };
        Ok(Self {
            pool,
            set_count,
            allocated: 0,
        })
    }
}

/// Creates a new descriptor allocator. Stores several pools contains `set_count` available
/// descriptors each. `sizes` describes the relative count for each descriptor size. Allocates new
/// pools when no free are available
pub struct DescriptorAllocator {
    device: Rc<Device>,
    /// Describes the relative sizes of the allocated pools.
    sizes: Vec<(DescriptorType, f32)>,
    set_count: u32,
    /// A list of pools with atleast 1 descriptor remaining.
    pools: Vec<Pool>,
    /// A list of completely full pools.
    full_pools: Vec<Pool>,
}

impl DescriptorAllocator {
    /// Creates a new descriptor allocator. Stores several pools contains `set_count` available
    /// descriptors each. `sizes` describes the relative
    pub fn new(device: Rc<Device>, sizes: Vec<(DescriptorType, f32)>, set_count: u32) -> Self {
        Self {
            device,
            sizes,
            set_count,
            pools: Vec::new(),
            full_pools: Vec::new(),
        }
    }

    /// Allocates a descriptor set for each element in `layouts`. Will allocate a new pool if no free pools
    /// are available. Correctly handles when descriptor set count is more than preferred `set_count`
    pub fn allocate(
        &mut self,
        layouts: &[DescriptorSetLayout],
    ) -> Result<Vec<vk::DescriptorSet>, Error> {
        let mut alloc_info = vk::DescriptorSetAllocateInfo {
            descriptor_pool: vk::DescriptorPool::null(),
            descriptor_set_count: layouts.len() as u32,
            p_set_layouts: layouts.as_ptr(),
            ..Default::default()
        };

        // Iterate and find a free pool
        for (i, pool) in self.pools.iter_mut().enumerate() {
            alloc_info.descriptor_pool = pool.pool;

            match unsafe { self.device.allocate_descriptor_sets(&alloc_info) } {
                Ok(sets) => {
                    pool.allocated += alloc_info.descriptor_set_count;
                    // Pool is full, move to full pools
                    if pool.allocated >= pool.set_count {
                        self.full_pools.push(self.pools.swap_remove(i));
                    }
                    return Ok(sets);
                }
                // Not enough room in pool for sets and sizes
                Err(vk::Result::ERROR_OUT_OF_POOL_MEMORY_KHR) => continue,
                Err(e) => return Err(e.into()),
            }
        }

        // No free pool found. Allocate a new pool. Override set count if requested descriptor
        // count is more.
        let pool = self.allocate_pool(self.set_count.max(alloc_info.descriptor_set_count))?;
        alloc_info.descriptor_pool = pool.pool;

        let sets = unsafe { self.device.allocate_descriptor_sets(&alloc_info)? };

        Ok(sets)
    }

    /// Resets all allocated pools and descriptor sets.
    pub fn reset(&mut self) -> Result<(), Error> {
        for pool in self
            .pools
            .iter_mut()
            .chain(&mut self.full_pools)
            .filter(|pool| pool.allocated != 0)
        {
            pool.allocated = 0;
            unsafe {
                self.device
                    .reset_descriptor_pool(pool.pool, Default::default())?
            }
        }

        // These pools are no longer full
        self.full_pools.clear();

        Ok(())
    }

    // Clears and destroys all allocated pools.
    pub fn clear(&mut self) {
        for pool in self.pools.drain(..).chain(self.full_pools.drain(..)) {
            unsafe { self.device.destroy_descriptor_pool(pool.pool, None) }
        }
    }

    /// Allocates a new pool with `set_count` descriptors. Ignores `self.set_count`
    fn allocate_pool(&mut self, set_count: u32) -> Result<&mut Pool, Error> {
        log::debug!("Allocating new pool of {} sets", set_count);
        let pool = Pool::new(&self.device, self.set_count, &self.sizes)?;
        self.pools.push(pool);
        Ok(self.pools.last_mut().unwrap())
    }

    // Diagnostics
    /// Returns the total number of allocated pools
    pub fn total_pool_count(&self) -> usize {
        self.pools.len() + self.full_pools.len()
    }

    pub fn full_pool_count(&self) -> usize {
        self.full_pools.len()
    }
}

impl Drop for DescriptorAllocator {
    fn drop(&mut self) {
        self.clear();
    }
}
