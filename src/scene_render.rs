
use std::sync::Arc;
use std::sync::Mutex;


use crate::render;
use crate::render::Render;
use crate::render::subpass_node::RenderSubpass;
use crate::render::subpass_node::SubpassHandle;
use crate::scene::InstanceData;
use crate::scene::Scene;

use vulkano::buffer::Buffer;
use vulkano::buffer::BufferCreateInfo;
use vulkano::buffer::BufferUsage;
use vulkano::buffer::IndexBuffer;
use vulkano::buffer::Subbuffer;
use vulkano::buffer::BufferContents;
use vulkano::descriptor_set::DescriptorBufferInfo;
use vulkano::descriptor_set::DescriptorSet;
use vulkano::descriptor_set::WriteDescriptorSet;
use vulkano::descriptor_set::layout;
use vulkano::descriptor_set::layout::DescriptorSetLayout;
use vulkano::descriptor_set::layout::DescriptorType;
use vulkano::device::Device;
use vulkano::memory::allocator::AllocationCreateInfo;
use vulkano::memory::allocator::MemoryTypeFilter;
use vulkano::pipeline::GraphicsPipeline;
use vulkano::pipeline::Pipeline;
use vulkano::pipeline::graphics::{GraphicsPipelineCreateInfo, depth_stencil::{DepthState, DepthStencilState}};
use vulkano::pipeline::graphics::color_blend::{ColorBlendState, ColorBlendAttachmentState};
use vulkano::pipeline::DynamicState;

use vulkano::pipeline::PipelineShaderStageCreateInfo;
use vulkano::pipeline::graphics::vertex_input::Vertex;
use vulkano::pipeline::graphics::vertex_input::VertexDefinition;
use vulkano::pipeline::layout::{PipelineLayout, PipelineDescriptorSetLayoutCreateInfo};
use vulkano::render_pass::Subpass;

mod shaders {
    pub mod vs_pnuv {
        vulkano_shaders::shader!{
            ty : "vertex",
            path : "shaders/scene/base.vert"
        }
    }
    pub mod ps_pnuv {
        vulkano_shaders::shader!{
            ty : "fragment",
            path : "shaders/scene/base.frag"
        }
    }
}

#[repr(C)]
#[derive(BufferContents, Default, Clone)]
struct FrameConsts {
    view_projection : [f32; 16],
    view : [f32; 16],
    inverse_view : [f32; 16],
}

struct SceneShared {
    pipeline : Arc<GraphicsPipeline>,
    scene : Arc<Scene>,
    frame_consts : Vec<Subbuffer<FrameConsts>>, // ring buffer
    frame_sets : Vec<Arc<DescriptorSet>>,
    current_consts : Mutex<FrameConsts>,
}

pub struct SceneRenderer {
    color_pass_node : SubpassHandle,
    shared_state : Arc<SceneShared>,
}

impl SceneRenderer {
    pub fn new(render : &Render, scene : Arc<Scene>) -> Self
    {
        let pipeline = Self::create_color_pipeline(render);
        let frame_consts = Self::create_global_const(render);
        let frame_sets = Self::create_frame_sets(render, 
            pipeline.layout().set_layouts()[0].clone(), &scene, &frame_consts);
        
        let shared_state = Arc::new(SceneShared {
            pipeline, scene, frame_consts, frame_sets, 
            current_consts : Mutex::new(Default::default())
        });

        let color_pass_node = Self::create_subpass(render, shared_state.clone());

        Self {
            shared_state,
            color_pass_node
        }
    }
    

    pub fn update_camera(&self, camera : &crate::CameraController)
    {
        let projection = camera.projection_matrix();
        let view  = camera.view_matrix();

        let mut lock = self.shared_state.current_consts.lock().unwrap();
        lock.view_projection = (projection * view).as_slice().try_into().unwrap();
        lock.view = view.as_slice().try_into().unwrap();
        lock.inverse_view = view.try_inverse().unwrap().as_slice().try_into().unwrap();
    }
    
    fn create_subpass(render : &Render, shared_state : Arc<SceneShared>) -> SubpassHandle {
        render.register_node(RenderSubpass::Normal, String::from("scene_render"), move |cmd, ctx| {
            println!("ffid {}", ctx.ffid);

            let const_buf = shared_state.frame_consts[ctx.ffid as usize].clone();
            {
                let lock = shared_state.current_consts.lock().unwrap();
                *const_buf.write().unwrap() = lock.clone();
            }
            
            let desc_set = shared_state.frame_sets[ctx.ffid as usize].clone();
            
            cmd.set_viewport(0, vec![ctx.viewport.clone()].into()).unwrap();
            cmd.bind_pipeline_graphics(shared_state.pipeline.clone()).unwrap();
            
            let mut bound_vbuf : Option<u32> = None;
            let mut bound_index : Option<u8> = None; // 
            
            for instance in shared_state.scene.instances.iter() {
                let mesh_id = instance.mesh_id;
                
                let mesh = &shared_state.scene.meshes[mesh_id as usize].mesh_desc;
            
                mesh.update_lfu(ctx.frame_no);
                
                if mesh.num_verts() != 0 && bound_vbuf != Some(mesh.vpool_id()) {
                    cmd.bind_vertex_buffers(0, [
                        mesh.vertex_buffer().unwrap()
                    ]).unwrap();
                    bound_vbuf = Some(mesh.vpool_id());
                }
                
                let use_index = mesh.has_indices();
                if use_index && bound_index != Some(mesh.index_size_bytes()) {
                    match mesh.index_size_bytes() {
                        1 => cmd.bind_index_buffer(IndexBuffer::U8(mesh.index_buffer().unwrap())).unwrap(),
                        2 => cmd.bind_index_buffer(IndexBuffer::U16(mesh.index_buffer().unwrap().reinterpret::<[u16]>())).unwrap(),
                        4 => cmd.bind_index_buffer(IndexBuffer::U32(mesh.index_buffer().unwrap().reinterpret::<[u32]>())).unwrap(),
                        _ => unreachable!()
                    };
                    bound_index = Some(mesh.index_size_bytes());
                }

                let offset = instance.handle.offset_bytes() as u32;
                let desc_set_offset = desc_set.clone().offsets([offset]);
                cmd.bind_descriptor_sets(vulkano::pipeline::PipelineBindPoint::Graphics, shared_state.pipeline.layout().clone(), 0, desc_set_offset).unwrap();
                
                unsafe {
                    if use_index {
                        cmd.draw_indexed(mesh.num_indices(), 1, mesh.index_offset(), mesh.vertex_offset() as i32, 0).unwrap();
                    } else { 
                        cmd.draw(mesh.num_verts(), 1, mesh.vertex_offset(), 0).unwrap();
                    }
                }
            }

        })
    }

    fn patch_pipeline_layout(layout_info : &mut PipelineDescriptorSetLayoutCreateInfo)  {
        let desc_set = layout_info.set_layouts[0].bindings.get_mut(&1);
        if let Some(binding) = desc_set {
            if binding.descriptor_type == DescriptorType::UniformBuffer {
                binding.descriptor_type = DescriptorType::UniformBufferDynamic;
            }
        }
    }
    
    fn create_color_pipeline(render : &Render) -> Arc<GraphicsPipeline> {
        let device = render.get_device().clone();
        let vshader = shaders::vs_pnuv::load(device.clone()).unwrap();
        let pshader = shaders::ps_pnuv::load(device.clone()).unwrap();
        
        let stages = [
            PipelineShaderStageCreateInfo::new(vshader.entry_point("main").unwrap()),
            PipelineShaderStageCreateInfo::new(pshader.entry_point("main").unwrap())
        ];
        
        let mut pipeline_desc_set = PipelineDescriptorSetLayoutCreateInfo::from_stages(&stages);
        Self::patch_pipeline_layout(&mut pipeline_desc_set);

        let layout = PipelineLayout::new(
            device.clone(),
            pipeline_desc_set.into_pipeline_layout_create_info(device.clone()).unwrap())
            .unwrap();
        
        let subpass = render.get_subpass(RenderSubpass::Normal).unwrap();
        
        let vertex_input_state = crate::scene::VertexPNUV::per_vertex()
            .definition(&vshader.entry_point("main").unwrap())
            .unwrap();

        let mut info = GraphicsPipelineCreateInfo {
            stages : stages.into_iter().collect(),
            vertex_input_state : Some(vertex_input_state),
            input_assembly_state : Some(Default::default()),
            rasterization_state: Some(Default::default()),
            multisample_state: Some(Default::default()),
            
            depth_stencil_state : Some(DepthStencilState {
                depth : Some(DepthState {
                    write_enable : true,
                    compare_op : vulkano::pipeline::graphics::depth_stencil::CompareOp::LessOrEqual
                }),
                ..Default::default()
            }),
            viewport_state : Some(Default::default()),
            color_blend_state: Some(ColorBlendState::with_attachment_states(subpass.num_color_attachments(), ColorBlendAttachmentState::default())), 
            subpass: Some(subpass.into()),
            ..GraphicsPipelineCreateInfo::layout(layout)
        };
        
        info.dynamic_state.insert(DynamicState::Viewport);

        GraphicsPipeline::new(device, None, info).unwrap()
    }

    fn create_global_const(render : &Render) -> Vec<Subbuffer<FrameConsts>> {
        let nff = render.get_num_frames_in_flight();
        
        let mut results = Vec::with_capacity(nff);
        
        for _ in 0..nff {
            let buf = Buffer::new_sized::<FrameConsts>(render.mem_allocator().clone(),
                BufferCreateInfo {
                    usage : BufferUsage::UNIFORM_BUFFER,
                    ..Default::default()
                },
                AllocationCreateInfo {
                    memory_type_filter : MemoryTypeFilter::PREFER_HOST | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                    ..Default::default()
                }).unwrap();
            
            results.push(buf);
        }
        results
    }

    fn create_frame_sets(render : &Render, layout : Arc<DescriptorSetLayout>, scene: &Arc<Scene>, frame_consts : &Vec<Subbuffer<FrameConsts>>) 
        -> Vec<Arc<DescriptorSet>>
    {
        let mut result = Vec::<Arc<DescriptorSet>>::with_capacity(frame_consts.len());
        
        for i in 0..frame_consts.len() {
            let set = DescriptorSet::new(
                render.descriptor_set_allocator().clone(), 
                layout.clone(), 
                [
                    WriteDescriptorSet::buffer(0, frame_consts[i].clone()),
                    WriteDescriptorSet::buffer_with_range(1, DescriptorBufferInfo {
                        buffer : scene.instance_manager.instance_buffer().as_bytes().clone(),
                        range : 0..(size_of::<InstanceData>() as u64), 
                    })
                ], 
                []).unwrap();
            result.push(set);
        }
        result
    }
}
