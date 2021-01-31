use std::ffi::CString;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to load vulkan library")]
    LoadingError,
    #[error("Vulkan Error {0}")]
    VulkanError(#[from] ash::vk::Result),
    #[error("Vulkan is not available and/or unsupported")]
    VulkanUnsupported,
    #[error("Vulkan Instance creation error")]
    InstanceError(#[from] ash::InstanceError),
    #[error("Missing required extensions: {0:?}")]
    MissingExtensions(Vec<CString>),
    #[error("Missing required instance layers: {0:?}")]
    MissingLayers(Vec<CString>),
}
