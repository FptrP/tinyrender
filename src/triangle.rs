use std::{cell::RefCell, collections::HashSet, sync::Arc, sync::Mutex};

use vulkano::{device::DeviceOwned, pipeline::{self, DynamicState, GraphicsPipeline, Pipeline, PipelineLayout, PipelineShaderStageCreateInfo, graphics::{GraphicsPipelineCreateInfo, depth_stencil::{DepthState, DepthStencilState}, viewport}}, render_pass::{RenderPass, Subpass}};

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

use crate::render::{self, Render, subpass_node::RenderSubpass};
use crate::render::subpass_node::SubpassHandle;

pub struct TrianglePass {
    pub pipeline : Arc<GraphicsPipeline>,
    pub handle : SubpassHandle,
    pub tri_color : Arc<Mutex<[f32; 4]>>,
}

impl TrianglePass {
    pub fn new(renderer : &Render) -> Self {
        let device = renderer.get_device();
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
        
        let subpass = Subpass::from(renderer.get_main_renderpass().clone(), 0).unwrap(); 
            
        let mut info = GraphicsPipelineCreateInfo {
            stages : stages.into_iter().collect(),
            vertex_input_state : Some(Default::default()),
            input_assembly_state : Some(Default::default()),
            rasterization_state: Some(Default::default()),
            multisample_state: Some(Default::default()),
            
            depth_stencil_state : Some(DepthStencilState {
                depth : Some(DepthState {
                    write_enable : true,
                    compare_op : pipeline::graphics::depth_stencil::CompareOp::LessOrEqual
                }),
                ..Default::default()
            }),

            viewport_state : Some(ViewportState { 
                ..Default::default()}),
            color_blend_state: Some(ColorBlendState::with_attachment_states(
                subpass.num_color_attachments(),
                ColorBlendAttachmentState::default(),
            )), 
            subpass: Some(subpass.into()),
            ..GraphicsPipelineCreateInfo::layout(layout)
        };
        
        info.dynamic_state.insert(DynamicState::Viewport);

        let pipeline_orig = GraphicsPipeline::new(device.clone(), None, info).unwrap();
        
        let tri_color_orig = Arc::new(Mutex::new([1f32, 0f32, 0f32, 1f32]));

        let pipeline = pipeline_orig.clone();
        let tri_color = tri_color_orig.clone(); 

        let hndl = renderer.register_node(RenderSubpass::Normal, String::from("triangle"), move |cmd, ctx| { 
            cmd.set_viewport(0, vec![ctx.viewport.clone()].into()).unwrap();
            cmd.bind_pipeline_graphics(pipeline.clone()).unwrap();
            cmd.push_constants(pipeline.layout().clone(), 0, *tri_color.lock().unwrap()).unwrap();
            unsafe { cmd.draw(3, 1, 0, 0).unwrap() };
        });
        

        Self { pipeline : pipeline_orig, handle : hndl, tri_color : tri_color_orig }
    }
}

