use super::{descriptors::DescriptorLayoutCache, Error};
use super::{renderpass::*, Extent};
use ash::version::DeviceV1_0;
use ash::Device;
use std::io::{Read, Seek};
use std::{ffi::CString, rc::Rc};

use ash::vk;

mod shader;
use shader::*;

pub struct Pipeline {
    device: Rc<Device>,
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
}

impl Pipeline {
    pub fn new<R>(
        device: Rc<Device>,
        layout_cache: &mut DescriptorLayoutCache,
        mut vertexshader: R,
        mut fragmentshader: R,
        extent: Extent,
        renderpass: &RenderPass,
        vertex_binding: vk::VertexInputBindingDescription,
        vertex_attributes: &[vk::VertexInputAttributeDescription],
        samples: vk::SampleCountFlags,
    ) -> Result<Self, Error>
    where
        R: Read + Seek,
    {
        // Read and create the shader modules
        let mut vert_code = Vec::new();
        vertexshader.read_to_end(&mut vert_code)?;

        let mut frag_code = Vec::new();
        fragmentshader.read_to_end(&mut frag_code)?;

        let vertexshader = ShaderModule::new(&device, &mut vertexshader)?;
        let fragmentshader = ShaderModule::new(&device, &mut fragmentshader)?;

        let layout = shader::reflect(&device, &[&vertexshader, &fragmentshader], layout_cache)?;

        let entrypoint = CString::new("main").unwrap();

        let shader_stages = [
            vk::PipelineShaderStageCreateInfo::builder()
                .module(vertexshader.module)
                .stage(vk::ShaderStageFlags::VERTEX)
                .name(&entrypoint)
                .build(),
            vk::PipelineShaderStageCreateInfo::builder()
                .module(fragmentshader.module)
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
            extent: extent.into(),
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
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::CLOCKWISE)
            .depth_bias_enable(false)
            .depth_bias_constant_factor(0.0)
            .depth_bias_clamp(0.0)
            .depth_bias_slope_factor(0.0);

        let multisampling = vk::PipelineMultisampleStateCreateInfo::builder()
            .sample_shading_enable(false)
            .rasterization_samples(samples)
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

        let depth_stencil = vk::PipelineDepthStencilStateCreateInfo {
            s_type: vk::StructureType::PIPELINE_DEPTH_STENCIL_STATE_CREATE_INFO,
            depth_test_enable: vk::TRUE,
            depth_write_enable: vk::TRUE,
            depth_compare_op: vk::CompareOp::LESS,
            depth_bounds_test_enable: vk::FALSE,
            stencil_test_enable: vk::FALSE,
            min_depth_bounds: 0.0,
            max_depth_bounds: 1.0,
            ..Default::default()
        };

        let create_info = vk::GraphicsPipelineCreateInfo::builder()
            .stages(&shader_stages)
            .vertex_input_state(&vertex_input_info)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterizer)
            .multisample_state(&multisampling)
            .color_blend_state(&color_blending)
            .depth_stencil_state(&depth_stencil)
            .layout(layout)
            .render_pass(renderpass.renderpass())
            .subpass(0)
            .build();

        let pipeline = unsafe {
            device
                .create_graphics_pipelines(vk::PipelineCache::null(), &[create_info], None)
                .map_err(|(_, e)| e)?
        }[0];

        // Destroy shader modules
        vertexshader.destroy(&device);
        fragmentshader.destroy(&device);

        Ok(Pipeline {
            device,
            pipeline,
            layout,
        })
    }

    /// Returns the raw vulkan pipeline handle.
    pub fn pipeline(&self) -> vk::Pipeline {
        self.pipeline
    }

    // Returns the pipeline layout.
    pub fn layout(&self) -> vk::PipelineLayout {
        self.layout
    }
}

impl AsRef<vk::Pipeline> for Pipeline {
    fn as_ref(&self) -> &vk::Pipeline {
        &self.pipeline
    }
}

impl Into<vk::Pipeline> for &Pipeline {
    fn into(self) -> vk::Pipeline {
        self.pipeline
    }
}

impl AsRef<vk::PipelineLayout> for Pipeline {
    fn as_ref(&self) -> &vk::PipelineLayout {
        &self.layout
    }
}

impl Into<vk::PipelineLayout> for &Pipeline {
    fn into(self) -> vk::PipelineLayout {
        self.layout
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        unsafe { self.device.destroy_pipeline(self.pipeline, None) }
        unsafe { self.device.destroy_pipeline_layout(self.layout, None) }
    }
}
