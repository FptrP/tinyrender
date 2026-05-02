
use std::sync::Arc;

use vulkano::{Validated, command_buffer::{AutoCommandBufferBuilder, RenderPassBeginInfo, SubpassBeginInfo, SubpassEndInfo, allocator::StandardCommandBufferAllocator}, device::Device, format::Format, image::ImageLayout, pipeline::graphics::viewport::Viewport, swapchain::{SwapchainCreateInfo, SwapchainPresentInfo}, sync::{AccessFlags, GpuFuture, PipelineStages, future::FenceSignalFuture}};


use vulkano::render_pass::{SubpassDescription, SubpassDependency, AttachmentDescription, AttachmentReference, RenderPass, Framebuffer, RenderPassCreateInfo};
use vulkano::render_pass::FramebufferCreateInfo;

use vulkano::image::view::{ImageView, ImageViewCreateInfo};
use vulkano::command_buffer::PrimaryAutoCommandBuffer;
use vulkano::VulkanError;

use crate::{render::subpass_node::{NodeList, RenderSubpass}, vkstate};

pub mod subpass_node;


pub struct Render
{
    vkstate : vkstate::State,
    pub main_renderpass : Arc<RenderPass>,
    
    subpasses : subpass_node::NodeList,

    cmd_allocator : Arc<StandardCommandBufferAllocator>,
    framebuffers : Vec<Arc<Framebuffer>>,
    frames_in_flight : Vec<Option<Arc<FenceSignalFuture<Box<dyn GpuFuture>>>>>,
    frame_index : usize,
    pub recreate_swapchain : bool,
}

impl Render {
    
    fn create_main_renderpass(device : Arc<Device>, color_fmt : Format) -> Arc<RenderPass> {
        //vulkano::single_pass_renderpass!()
        
        let rpinfo = RenderPassCreateInfo {
            attachments : vec![AttachmentDescription {
                format : color_fmt,
                load_op : vulkano::render_pass::AttachmentLoadOp::Clear,
                store_op : vulkano::render_pass::AttachmentStoreOp::Store,
                initial_layout : vulkano::image::ImageLayout::Undefined,
                final_layout : vulkano::image::ImageLayout::PresentSrc,
                ..Default::default()
            }],
            subpasses : vec![SubpassDescription {
                color_attachments : vec![Some(AttachmentReference {
                    attachment : 0u32,
                    layout : ImageLayout::ColorAttachmentOptimal,
                    ..Default::default()
                })],
                ..Default::default()
            }],
            dependencies : vec![SubpassDependency {
                src_subpass : None,
                dst_subpass : Some(0u32),
                src_stages : PipelineStages::ALL_COMMANDS,
                dst_stages : PipelineStages::ALL_COMMANDS,
                src_access : AccessFlags::MEMORY_READ,
                dst_access : AccessFlags::MEMORY_WRITE,
                ..Default::default()
            },
            SubpassDependency {
                src_subpass : Some(0u32),
                dst_subpass : None,
                src_stages : PipelineStages::ALL_COMMANDS,
                dst_stages : PipelineStages::ALL_COMMANDS,
                src_access : AccessFlags::MEMORY_WRITE,
                dst_access : AccessFlags::MEMORY_READ,
                ..Default::default()
            }],
            ..Default::default()
        };

        RenderPass::new(device, rpinfo).unwrap()
    }
    
    fn create_framebuffers(ctx : &vkstate::State, render_pass : &Arc<RenderPass>) -> Vec<Arc<Framebuffer>> {
        ctx.backbuffers.iter().map(|img| {
            let view_info = ImageViewCreateInfo::from_image(&img);
            let backbuffer_view = ImageView::new(img.clone(), view_info).unwrap();
            let fbinfo = FramebufferCreateInfo {
                attachments : vec![backbuffer_view],
                extent : img.extent()[..2].try_into().unwrap(),
                layers : 1u32, 
                ..Default::default()
            };

            Framebuffer::new(render_pass.clone(), fbinfo).unwrap()
        }).collect()
    }

    pub fn new(ctx : vkstate::State) -> Self {
        let main_rp = Self::create_main_renderpass(ctx.device.clone(), ctx.swapchain.as_ref().unwrap().image_format());
        let framebuffers = Self::create_framebuffers(&ctx, &main_rp);
        
        let cmd_allocator = Arc::new(StandardCommandBufferAllocator::new(ctx.device.clone(), Default::default())); 

        Self {
            vkstate : ctx,
            main_renderpass : main_rp,
            framebuffers,
            cmd_allocator,
            frames_in_flight : [None, None, None].into(),
            frame_index : 0,
            recreate_swapchain : false,
            subpasses : NodeList::new(),
        }

    }
    
    pub fn record_command_buffer(&self, backbuf_id : usize)
        -> Arc<PrimaryAutoCommandBuffer>
    {
        let mut cmd_builder = AutoCommandBufferBuilder::primary(
            self.cmd_allocator.clone(), self.vkstate.main_queue_family, 
            vulkano::command_buffer::CommandBufferUsage::OneTimeSubmit).unwrap();
        
        let main_extent = self.vkstate.swapchain.as_ref().unwrap().image_extent();

        let normal_ctx = subpass_node::SubpassContext {
            frame_no : 0,
            ffid : self.frame_index as u8,
            numff : self.frames_in_flight.len() as u8,
            backbuf_id : backbuf_id as u8,
            viewport : Viewport {
                offset : [0.0, 0.0],
                extent : [main_extent[0] as f32, main_extent[1] as f32],
                depth_range : 0.0..=1.0,
            },
        };

        cmd_builder.begin_render_pass(
            RenderPassBeginInfo {
                clear_values : vec![Some([0.0, 0.0, 0.0, 1.0].into())], 
                ..RenderPassBeginInfo::framebuffer(self.framebuffers[backbuf_id].clone())
            }, 
            SubpassBeginInfo {
                contents : vulkano::command_buffer::SubpassContents::Inline,
                ..Default::default()
            }).unwrap(); 
        
        self.subpasses.run_nodes(RenderSubpass::Normal, &mut cmd_builder, &normal_ctx);

        cmd_builder.end_render_pass(SubpassEndInfo::default()).unwrap();
        cmd_builder.build().unwrap()
    }

    pub fn draw_frame(&mut self) 
    {
        if self.recreate_swapchain {
            return;
        }

        let swapchain = self.vkstate.swapchain.as_ref().unwrap();     
        
        let (backbuffer_id, suboptimal, acquire_future) = 
            match vulkano::swapchain::acquire_next_image(swapchain.clone(), None)
            .map_err(Validated::unwrap)
        {
            Ok(v) => v,
            Err(VulkanError::OutOfDate) => {
                self.recreate_swapchain = true;
                return;
            },
            Err(e) => panic!("{}", e),
        };
        
        self.recreate_swapchain |= suboptimal;
        
        let cmd = self.record_command_buffer(backbuffer_id as usize);

        let prev_frame = (self.frame_index + self.frames_in_flight.len() - 1) % self.frames_in_flight.len();

        
        if let Some(fence) = self.frames_in_flight[self.frame_index].take() {
            fence.wait(None).unwrap();
        }

        let previous_future = match &self.frames_in_flight[prev_frame] {
            None => {
                let mut f = vulkano::sync::now(self.vkstate.device.clone());
                f.cleanup_finished();
                f.boxed()
            },
            Some(prev_fence) => prev_fence.clone().boxed()
        };
        
        let submit = previous_future
            .join(acquire_future)
            .then_execute(self.vkstate.main_queue.clone(), cmd)
            .unwrap()
            .then_swapchain_present(self.vkstate.main_queue.clone(), SwapchainPresentInfo::swapchain_image_index(swapchain.clone(), backbuffer_id))
            .boxed()
            .then_signal_fence_and_flush()
            .map_err(Validated::unwrap);
        
        let future = match submit {
            Ok(f) => Some(Arc::new(f)),
            Err(VulkanError::OutOfDate) => {
                self.recreate_swapchain = true;
                None
            },
            Err(e) => panic!("{}", e),
        };

        self.frames_in_flight[self.frame_index] = future;
        self.frame_index = (self.frame_index + 1) % self.frames_in_flight.len();
    }

    pub fn recreate_swapchain(&mut self, new_extent : [u32; 2]) {
        let swapchain_old = self.vkstate.swapchain.as_ref().unwrap();
        let resized = new_extent != swapchain_old.image_extent();

        if !resized && !self.recreate_swapchain {
            return;
        }
        println!("[render] recreate swapchain {}x{} -> {}x{}",
            swapchain_old.image_extent()[0],
            swapchain_old.image_extent()[1],
            new_extent[0],
            new_extent[1]);

        let (swapchain, images) = swapchain_old.recreate(
            SwapchainCreateInfo {
                image_extent : new_extent,
              ..swapchain_old.create_info()
            }).unwrap();

        self.vkstate.swapchain = Some(swapchain);
        self.vkstate.backbuffers = images;

        self.framebuffers = Self::create_framebuffers(&self.vkstate, 
            &self.main_renderpass);
        self.recreate_swapchain = false;
    }
    

    pub fn get_device(&self) -> &Arc<Device> {
        &self.vkstate.device
    }

    pub fn get_main_renderpass(&self) -> &Arc<RenderPass> {
        &self.main_renderpass
    }
    
    pub fn register_node(&self, pass : subpass_node::RenderSubpass, 
        name : String, callback : impl subpass_node::SubpassCallback) 
        -> subpass_node::SubpassHandle
    {
        self.subpasses.register_node(pass, name, callback)
    }
}
