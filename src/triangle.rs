use std::sync::Arc;

use vulkano::{device::DeviceOwned, pipeline::{GraphicsPipeline, PipelineLayout, PipelineShaderStageCreateInfo, graphics::{GraphicsPipelineCreateInfo, viewport}}, render_pass::{RenderPass, Subpass}};

use vulkano::pipeline::layout::PipelineDescriptorSetLayoutCreateInfo;

use vulkano::pipeline::graphics::color_blend::{ColorBlendAttachmentState, ColorBlendState};

use vulkano::pipeline::graphics::viewport::*;

extern crate vulkano_shaders;

mod vs {
    vulkano_shaders::shader!{
        ty : "vertex",
        path : r"./shaders/tri/t.vert"
    }
}

mod ps {
    vulkano_shaders::shader!{
        ty : "fragment",
        path : r"./shaders/tri/t.frag"
    }
}

pub struct TrianglePass {
    pub pipeline : Arc<GraphicsPipeline>
}

impl TrianglePass {
    pub fn new(rp : Arc<RenderPass>) -> Self {
        let device = rp.device();
        let vshader = vs::load(device.clone()).unwrap();
        let pshader = ps::load(device.clone()).unwrap();
        
        let stages = [
            PipelineShaderStageCreateInfo::new(vshader.entry_point("main").unwrap()),
            PipelineShaderStageCreateInfo::new(pshader.entry_point("main").unwrap())
        ];
        
        let layout = PipelineLayout::new(
            device.clone(),
            PipelineDescriptorSetLayoutCreateInfo::from_stages(&stages)
                .into_pipeline_layout_create_info(device.clone())
                .unwrap(),
        ).unwrap();
        
        let subpass = Subpass::from(rp.clone(), 0).unwrap();
        let viewport = Viewport {
            offset: [0.0, 0.0],
            extent: [640.0, 480.0],
            depth_range: 0.0..=1.0,
        };

        let info = GraphicsPipelineCreateInfo {
            stages : stages.into_iter().collect(),
            vertex_input_state : Some(Default::default()),
            input_assembly_state : Some(Default::default()),
            rasterization_state: Some(Default::default()),
            multisample_state: Some(Default::default()),
            viewport_state : Some(ViewportState { 
                viewports : [viewport].into_iter().collect(), 
                ..Default::default()}),
            color_blend_state: Some(ColorBlendState::with_attachment_states(
                subpass.num_color_attachments(),
                ColorBlendAttachmentState::default(),
            )),
            subpass: Some(subpass.into()),
            ..GraphicsPipelineCreateInfo::layout(layout)
        };

        let pipeline = GraphicsPipeline::new(device.clone(), None, info).unwrap();
        Self { pipeline }
    }
}

