use ash::vk;

pub trait Vertex {
    fn binding_description() -> vk::VertexInputBindingDescription;
    fn attribute_descriptions() -> &'static [vk::VertexInputAttributeDescription];
}
