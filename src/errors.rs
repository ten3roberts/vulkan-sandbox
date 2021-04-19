use crate::resources;
use crate::vulkan;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    VulkanError(#[from] vulkan::Error),
    #[error("Unable to load resource using sparse buffer accessor")]
    SparseAccessor,
    #[error("{0}")]
    ResourceError(#[from] resources::Error),
}
