use std::{cell::RefCell, collections::HashSet, sync::{Arc, Mutex}};
use std::sync::RwLock;

use vulkano::{command_buffer::{AutoCommandBufferBuilder, BufferCopy, CopyBufferInfo}, device::DeviceOwned, pipeline::{self, DynamicState, GraphicsPipeline, Pipeline, PipelineLayout, PipelineShaderStageCreateInfo, graphics::{GraphicsPipelineCreateInfo, depth_stencil::{DepthState, DepthStencilState}, vertex_input::{VertexBuffersCollection, VertexDefinition, VertexInputState}, viewport}}, render_pass::{RenderPass, Subpass}, sync::GpuFuture};

use vulkano::pipeline::layout::PipelineDescriptorSetLayoutCreateInfo;

use vulkano::pipeline::graphics::color_blend::{ColorBlendAttachmentState, ColorBlendState};

use vulkano::pipeline::graphics::viewport::*;
use vulkano::pipeline::graphics::vertex_input::Vertex;
use vulkano::buffer::BufferContents;

extern crate vulkano_shaders;

#[derive(BufferContents, Vertex)]
#[repr(C)]
struct TriVertex {
    #[format(R32G32B32_SFLOAT)]
    pos : [f32; 3],
    #[format(R8G8B8_UNORM)]
    color : [u8; 3],
}

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

use crate::render::{self, Render, mesh_pool, subpass_node::RenderSubpass};
use crate::render::subpass_node::SubpassHandle;
use crate::RenderGlobalParams;

pub struct TrianglePass {
    pub pipeline : Arc<GraphicsPipeline>,
    pub handle : SubpassHandle,
    pub tri_color : Arc<Mutex<[f32; 4]>>,
    pub tri_mesh : Arc<mesh_pool::MeshDesc>,
}

impl TrianglePass {
    pub fn new(renderer : &Render, params: Arc<RwLock<RenderGlobalParams>>) -> Self {
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
        
        let vertex_input_state = TriVertex::per_vertex()
            .definition(&vshader.entry_point("main").unwrap())
            .unwrap();

        let subpass = Subpass::from(renderer.get_main_renderpass().clone(), 0).unwrap(); 
            
        let mut info = GraphicsPipelineCreateInfo {
            stages : stages.into_iter().collect(),
            vertex_input_state : Some(vertex_input_state),
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
            color_blend_state: Some(ColorBlendState::with_attachment_states(subpass.num_color_attachments(), ColorBlendAttachmentState::default())), 
            subpass: Some(subpass.into()),
            ..GraphicsPipelineCreateInfo::layout(layout)
        };
        
        info.dynamic_state.insert(DynamicState::Viewport);

        let pipeline_orig = GraphicsPipeline::new(device.clone(), None, info).unwrap();
        
        let tri_color_orig = Arc::new(Mutex::new([1f32, 0f32, 0f32, 1f32]));

        let pipeline = pipeline_orig.clone();
        let tri_color = tri_color_orig.clone(); 

        let vinfo = TriVertex::per_vertex();

        let tri_mesh = renderer.mesh_pool().alloc_mesh(vinfo.stride, 0, 3, 0);
        
        println!("tri_mesh : stride {}", vinfo.stride);
        
        let num_verts = tri_mesh.num_verts();
        
        let dst_buffer = tri_mesh.vertex_buffer().unwrap();
        let dst_offset = tri_mesh.vertex_byte_offset();
        
        let fence = renderer.run_async_commands(|cmd, alloc| {
            let staging = alloc.allocate_slice::<TriVertex>(num_verts as u64).unwrap();
            
            {
                let mut verts = staging.write().unwrap();
                verts[0] = TriVertex {
                    pos : [0.0, 0.5, 0.5],
                    color : [255u8, 0u8, 0u8]
                };

                verts[1] = TriVertex {
                    pos : [-0.5, -0.5, 0.5],
                    color : [0u8, 255u8, 0u8]
                };

                verts[2] = TriVertex {
                    pos : [0.5, -0.5, 0.5],
                    color : [0u8, 0u8, 255u8]
                };
            }
 
            cmd.copy_buffer(CopyBufferInfo {
                regions : [BufferCopy {
                    src_offset : 0,
                    dst_offset,
                    size : staging.size(),
                    ..Default::default()
                }].into(),
                ..CopyBufferInfo::buffers(staging.clone(), dst_buffer)
            }).unwrap();
        }).unwrap();

        fence.wait(None).unwrap(); 
        
        let tri_mesh_orig = tri_mesh.clone();

        let hndl = renderer.register_node(RenderSubpass::Normal, String::from("triangle"), move |cmd, ctx| { 
            let params_const = params.read().unwrap(); 

            tri_mesh.update_lfu(ctx.frame_no as u64);

            cmd.set_viewport(0, vec![ctx.viewport.clone()].into()).unwrap();
            cmd.bind_pipeline_graphics(pipeline.clone()).unwrap();
            cmd.bind_vertex_buffers(0u32, (tri_mesh.vertex_buffer().unwrap(),)).unwrap(); 

            if pipeline.layout().push_constant_ranges().len() != 0 {
                let floats : [f32; 16] = params_const.view_projection.as_slice().try_into().unwrap();
                cmd.push_constants(pipeline.layout().clone(), 0, floats).unwrap();
            }
            unsafe { cmd.draw(tri_mesh.num_verts(), 1, tri_mesh.vertex_offset(), 0).unwrap() };
        });
        

        Self { pipeline : pipeline_orig, handle : hndl, tri_color : tri_color_orig, tri_mesh : tri_mesh_orig }
    }
}

