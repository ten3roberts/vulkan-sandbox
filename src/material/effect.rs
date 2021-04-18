use crate::vulkan;
use vulkan::Pipeline;

/// A material effect is shared among several materials and define the pipelines associated for each
/// renderpass.
pub struct MaterialEffect {
    passes: Vec<Pipeline>,
}

impl MaterialEffect {
    pub fn new(passes: Vec<Pipeline>) -> Self {
        Self { passes }
    }

    pub fn pass(&self, index: usize) -> &Pipeline {
        &self.passes[index]
    }
}
