use super::renderpass::*;
use super::Error;
use ash::version::DeviceV1_0;
use ash::Device;
use std::io::{Read, Seek};
use std::{ffi::CString, rc::Rc};

use ash::vk;

pub struct Pipeline {
    device: Rc<Device>,
    pipeline: vk::Pipeline,
}

impl Pipeline {
    pub fn new<R>(
        device: Rc<Device>,
        mut vertexshader: R,
        mut fragmentshader: R,
        extent: vk::Extent2D,
        layout: &PipelineLayout,
        renderpass: &RenderPass,
        vertex_binding: vk::VertexInputBindingDescription,
        vertex_attributes: &[vk::VertexInputAttributeDescription],
    ) -> Result<Self, Error>
    where
        R: Read + Seek,
    {
        // Read and create the shader modules
        let vert_code = ash::util::read_spv(&mut vertexshader)?;
        let frag_code = ash::util::read_spv(&mut fragmentshader)?;

        let vertexshader = create_shadermodule(&device, &vert_code)?;
        let fragmentshader = create_shadermodule(&device, &frag_code)?;

        let entrypoint = CString::new("main").unwrap();

        let shader_stages = [
            vk::PipelineShaderStageCreateInfo::builder()
                .module(vertexshader)
                .stage(vk::ShaderStageFlags::VERTEX)
                .name(&entrypoint)
                .build(),
            vk::PipelineShaderStageCreateInfo::builder()
                .module(fragmentshader)
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .name(&entrypoint)
                .build(),
        ];

        let vertex_binding_descriptions = [vertex_binding];

        // No vertices for now
        let vertex_input_info = vk::PipelineVertexInputStateCreateInfo::builder()
            .vertex_binding_descriptions(&vertex_binding_descriptions)
            .vertex_attribute_descriptions(&vertex_attributes);

        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(false);

        let viewports = [vk::Viewport {
            x: 0.0f32,
            y: 0.0f32,
            width: extent.width as _,
            height: extent.height as _,
            min_depth: 0.0f32,
            max_depth: 1.0f32,
        }];

        let scissors = [vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        }];

        let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
            .viewports(&viewports)
            .scissors(&scissors);

        let rasterizer = vk::PipelineRasterizationStateCreateInfo::builder()
            // Clamp pixels outside far and near
            .depth_clamp_enable(false)
            // If true: Discard all pixels
            .rasterizer_discard_enable(false)
            .polygon_mode(vk::PolygonMode::FILL)
            .line_width(1.0)
            .cull_mode(vk::CullModeFlags::BACK)
            .front_face(vk::FrontFace::CLOCKWISE)
            .depth_bias_enable(false)
            .depth_bias_constant_factor(0.0)
            .depth_bias_clamp(0.0)
            .depth_bias_slope_factor(0.0);

        let multisampling = vk::PipelineMultisampleStateCreateInfo::builder()
            .sample_shading_enable(false)
            .rasterization_samples(vk::SampleCountFlags::TYPE_1)
            .min_sample_shading(1.0)
            .alpha_to_coverage_enable(false)
            .alpha_to_one_enable(false);

        let color_blend_attachments = [vk::PipelineColorBlendAttachmentState::builder()
            .color_write_mask(
                vk::ColorComponentFlags::R
                    | vk::ColorComponentFlags::G
                    | vk::ColorComponentFlags::B
                    | vk::ColorComponentFlags::A,
            )
            .blend_enable(false)
            .src_color_blend_factor(vk::BlendFactor::ONE)
            .dst_color_blend_factor(vk::BlendFactor::ZERO)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
            .alpha_blend_op(vk::BlendOp::ADD)
            .build()];

        let color_blending = vk::PipelineColorBlendStateCreateInfo::builder()
            .logic_op_enable(false)
            .attachments(&color_blend_attachments)
            .logic_op(vk::LogicOp::COPY);

        let create_info = vk::GraphicsPipelineCreateInfo::builder()
            .stages(&shader_stages)
            .vertex_input_state(&vertex_input_info)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterizer)
            .multisample_state(&multisampling)
            .color_blend_state(&color_blending)
            .layout(layout.layout)
            .render_pass(renderpass.renderpass())
            .subpass(0)
            .build();

        let pipeline = unsafe {
            device
                .create_graphics_pipelines(vk::PipelineCache::null(), &[create_info], None)
                .map_err(|(_, e)| e)?
        }[0];

        // Destroy shader modules
        unsafe { device.destroy_shader_module(vertexshader, None) };
        unsafe { device.destroy_shader_module(fragmentshader, None) };

        Ok(Pipeline { device, pipeline })
    }

    pub fn pipeline(&self) -> vk::Pipeline {
        self.pipeline
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        unsafe { self.device.destroy_pipeline(self.pipeline, None) }
    }
}

pub struct PipelineLayout {
    layout: vk::PipelineLayout,
    device: Rc<Device>,
}

impl PipelineLayout {
    pub fn new(device: Rc<Device>, set_layouts: &[vk::DescriptorSetLayout]) -> Result<Self, Error> {
        let pipeline_layout_info = vk::PipelineLayoutCreateInfo::builder()
            .set_layouts(set_layouts)
            .push_constant_ranges(&[]);

        let layout = unsafe { device.create_pipeline_layout(&pipeline_layout_info, None)? };

        Ok(PipelineLayout { device, layout })
    }

    pub fn layout(&self) -> vk::PipelineLayout {
        self.layout
    }
}

impl Drop for PipelineLayout {
    fn drop(&mut self) {
        unsafe { self.device.destroy_pipeline_layout(self.layout, None) }
    }
}

fn create_shadermodule(device: &Device, code: &[u32]) -> Result<vk::ShaderModule, Error> {
    let create_info = vk::ShaderModuleCreateInfo::builder().code(code);
    let shadermodule = unsafe { device.create_shader_module(&create_info, None)? };
    Ok(shadermodule)
}
