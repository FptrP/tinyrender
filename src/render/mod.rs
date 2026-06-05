
use std::sync::Arc;

use vulkano::{Validated, buffer::{Buffer, BufferUsage, allocator::{SubbufferAllocator, SubbufferAllocatorCreateInfo}}, command_buffer::{AutoCommandBufferBuilder, RenderPassBeginInfo, SubpassBeginInfo, SubpassEndInfo, allocator::StandardCommandBufferAllocator}, device::{Device, Queue}, format::Format, image::{Image, ImageAspects, ImageCreateInfo, ImageLayout, ImageUsage}, memory::allocator::{AllocationCreateInfo, MemoryTypeFilter, StandardMemoryAllocator}, pipeline::graphics::viewport::Viewport, swapchain::{SwapchainCreateInfo, SwapchainPresentInfo}, sync::{AccessFlags, DependencyFlags, GpuFuture, PipelineStages, future::FenceSignalFuture}};


use vulkano::render_pass::{SubpassDescription, SubpassDependency, AttachmentDescription, AttachmentReference, RenderPass, Framebuffer, RenderPassCreateInfo};
use vulkano::render_pass::FramebufferCreateInfo;

use vulkano::image::view::{ImageView, ImageViewCreateInfo};
use vulkano::command_buffer::PrimaryAutoCommandBuffer;
use vulkano::VulkanError;

use vulkano::descriptor_set::allocator::StandardDescriptorSetAllocator;

use crate::{render::subpass_node::{NodeList, RenderSubpass}, vkstate};

pub mod subpass_node;
pub mod mesh_pool;

const DEPTH_FMT : Format = Format::D24_UNORM_S8_UINT;

pub struct Render
{
    vkstate : vkstate::State,
    pub main_renderpass : Arc<RenderPass>,
    depth_view : Arc<ImageView>,
    subpasses : subpass_node::NodeList,

    mesh_allocator : mesh_pool::MeshPool,
    cmd_allocator : Arc<StandardCommandBufferAllocator>,
    subbuffer_allocator : Arc<SubbufferAllocator>,
    descriptor_set_allocator : Arc<StandardDescriptorSetAllocator>,        

    framebuffers : Vec<Arc<Framebuffer>>,
    frames_in_flight : Vec<Option<Arc<FenceSignalFuture<Box<dyn GpuFuture>>>>>,
    frame_index : usize,
    frame_no : u64,
    pub recreate_swapchain : bool,
}

impl Render {
    
    fn create_main_renderpass(device : Arc<Device>, color_fmt : Format) -> Arc<RenderPass> { 
        let rpinfo = RenderPassCreateInfo {
            attachments : vec![
                AttachmentDescription {
                    format : color_fmt,
                    load_op : vulkano::render_pass::AttachmentLoadOp::Clear,
                    store_op : vulkano::render_pass::AttachmentStoreOp::Store,
                    initial_layout : vulkano::image::ImageLayout::Undefined,
                    final_layout : vulkano::image::ImageLayout::PresentSrc,
                    ..Default::default()
                },
                AttachmentDescription {
                    format : DEPTH_FMT,
                    load_op : vulkano::render_pass::AttachmentLoadOp::Clear,
                    store_op : vulkano::render_pass::AttachmentStoreOp::DontCare,
                    initial_layout : vulkano::image::ImageLayout::Undefined,
                    final_layout : vulkano::image::ImageLayout::DepthStencilAttachmentOptimal,
                    ..Default::default()
                }
            ],
            subpasses : vec![SubpassDescription {
                color_attachments : vec![Some(AttachmentReference {
                    attachment : 0u32,
                    layout : ImageLayout::ColorAttachmentOptimal,
                    ..Default::default()
                })],
                depth_stencil_attachment : Some(AttachmentReference {
                    attachment : 1u32,
                    layout : ImageLayout::DepthStencilAttachmentOptimal,
                    //aspects : ImageAspects::DEPTH,
                    ..Default::default()
                }),
                ..Default::default()
            }],
            dependencies : vec![SubpassDependency {
                src_subpass : None,
                dst_subpass : Some(0u32),
                src_stages : PipelineStages::ALL_COMMANDS,
                dst_stages : PipelineStages::COLOR_ATTACHMENT_OUTPUT,
                src_access : AccessFlags::MEMORY_READ|AccessFlags::MEMORY_WRITE,
                dst_access : AccessFlags::COLOR_ATTACHMENT_WRITE,
                dependency_flags : DependencyFlags::BY_REGION,
                ..Default::default()
            },
            SubpassDependency {
                src_subpass : Some(0u32),
                dst_subpass : None,
                src_stages : PipelineStages::COLOR_ATTACHMENT_OUTPUT,
                dst_stages : PipelineStages::ALL_COMMANDS,
                src_access : AccessFlags::COLOR_ATTACHMENT_WRITE,
                dst_access : AccessFlags::MEMORY_READ|AccessFlags::MEMORY_WRITE,
                dependency_flags : DependencyFlags::BY_REGION,
                ..Default::default()
            }],
            ..Default::default()
        };

        RenderPass::new(device, rpinfo).unwrap()
    }
    
    fn create_depth_view(vk : &vkstate::State, extent : [u32; 2]) -> Arc<ImageView> {
        let depth_image = Image::new(vk.memory_allocator.clone(), 
            ImageCreateInfo {
                image_type : vulkano::image::ImageType::Dim2d,
                format : DEPTH_FMT,
                extent : [extent[0], extent[1], 1],
                usage : ImageUsage::DEPTH_STENCIL_ATTACHMENT|ImageUsage::TRANSIENT_ATTACHMENT,
                ..Default::default()
            }, 
            AllocationCreateInfo {
                memory_type_filter : MemoryTypeFilter::PREFER_DEVICE,
                ..Default::default()
            }).unwrap();

        ImageView::new(depth_image.clone(), ImageViewCreateInfo::from_image(&depth_image)).unwrap()
    }

    fn create_framebuffers(ctx : &vkstate::State, render_pass : &Arc<RenderPass>, depth_view : &Arc<ImageView>) -> Vec<Arc<Framebuffer>> {
        ctx.backbuffers.iter().map(|img| {
            let view_info = ImageViewCreateInfo::from_image(&img);
            let backbuffer_view = ImageView::new(img.clone(), view_info).unwrap();
            let fbinfo = FramebufferCreateInfo {
                attachments : vec![backbuffer_view, depth_view.clone()],
                extent : img.extent()[..2].try_into().unwrap(),
                layers : 1u32, 
                ..Default::default()
            };

            Framebuffer::new(render_pass.clone(), fbinfo).unwrap()
        }).collect()
    }

    pub fn new(ctx : vkstate::State) -> Self {
        let main_rp = Self::create_main_renderpass(ctx.device.clone(), ctx.swapchain.as_ref().unwrap().image_format()); 
        let depth_view = Self::create_depth_view(&ctx, ctx.swapchain.as_ref().unwrap().image_extent()); 
        let framebuffers = Self::create_framebuffers(&ctx, &main_rp, &depth_view);
        
        let cmd_allocator = Arc::new(StandardCommandBufferAllocator::new(ctx.device.clone(), Default::default())); 
        
        let subbuffer_allocator = SubbufferAllocator::new(
            ctx.memory_allocator.clone(),
            SubbufferAllocatorCreateInfo {
                memory_type_filter : MemoryTypeFilter::PREFER_HOST|MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                arena_size : 2048u64 << 10u64,
                buffer_usage : BufferUsage::TRANSFER_DST|BufferUsage::TRANSFER_SRC|BufferUsage::UNIFORM_BUFFER,
                ..Default::default()
            });

        const INDEX_POOL_SIZE : u32 = 8u32 << 20u32;
        const VERTEX_POOL_SIZE : u32 = 16u32 << 20u32;

        let mesh_pool = mesh_pool::MeshPool::new(
            ctx.memory_allocator.clone(), 
            INDEX_POOL_SIZE, 
            VERTEX_POOL_SIZE, 
            4);
        
        let descriptor_set_allocator = StandardDescriptorSetAllocator::new(
            ctx.device.clone(), Default::default());

        Self {
            vkstate : ctx,
            main_renderpass : main_rp,
            framebuffers,
            cmd_allocator,
            frames_in_flight : [None, None, None].into(),
            frame_index : 0,
            frame_no : 0,
            recreate_swapchain : false,
            subpasses : NodeList::new(),
            depth_view,
            mesh_allocator : mesh_pool,
            subbuffer_allocator : Arc::new(subbuffer_allocator),
            descriptor_set_allocator : Arc::new(descriptor_set_allocator),
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
            frame_no : self.frame_no,
            ffid : self.frame_index as u8,
            numff : self.frames_in_flight.len() as u8,
            backbuf_id : backbuf_id as u8,
            viewport : Viewport {
                offset : [0.0, 0.0],
                extent : [main_extent[0] as f32, main_extent[1] as f32],
                depth_range : 0.0..=1.0,
            },
            staging_allocator : self.subbuffer_allocator.clone(),
            descriptor_set_allocator : self.descriptor_set_allocator.clone()
        };
        
        self.subpasses.run_nodes(RenderSubpass::BeforeRender, &mut cmd_builder, &normal_ctx);

        cmd_builder.begin_render_pass(
            RenderPassBeginInfo {
                clear_values : vec![Some([0.0, 0.0, 0.0, 1.0].into()), 
                    Some(vulkano::format::ClearValue::DepthStencil((1f32, 0u32)))], 
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
        
        if let Some(mut fence) = self.frames_in_flight[self.frame_index].take() {
            fence.cleanup_finished();
            fence.wait(None).unwrap();
            fence.cleanup_finished();
        }

        let cmd = self.record_command_buffer(backbuffer_id as usize);

        let prev_frame = (self.frame_index + self.frames_in_flight.len() - 1) % self.frames_in_flight.len();
                
        if self.frame_no >= self.frames_in_flight.len() as u64 {
            let safe_frame = self.frame_no - self.frames_in_flight.len() as u64;
            self.mesh_allocator.collect_garbage(safe_frame);
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
                println!("Submit err");
                None
            },
            Err(e) => panic!("{}", e),
        };

        self.frames_in_flight[self.frame_index] = future;
        self.frame_index = (self.frame_index + 1) % self.frames_in_flight.len();
        self.frame_no += 1;
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
        
        self.depth_view = Self::create_depth_view(&self.vkstate, new_extent);

        self.framebuffers = Self::create_framebuffers(
            &self.vkstate, 
            &self.main_renderpass,
            &self.depth_view);
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

    pub fn mesh_pool(&self) -> &mesh_pool::MeshPool {
        &self.mesh_allocator
    }

    pub fn staging_buf_allocator(&self) -> &Arc<SubbufferAllocator> {
        &self.subbuffer_allocator
    }
    
    pub fn command_buf_allocator(&self) -> &Arc<StandardCommandBufferAllocator> {
        &self.cmd_allocator
    }

    pub fn main_queue_family(&self) -> u32 { 
        self.vkstate.main_queue_family
    }

    pub fn main_queue(&self) -> &Arc<Queue> {
        &self.vkstate.main_queue
    }
    pub fn mem_allocator(&self) -> &Arc<StandardMemoryAllocator> {
        &self.vkstate.memory_allocator
    }
    pub fn run_async_commands<F>(&self, callback : F) 
        -> Result<FenceSignalFuture<impl GpuFuture>, Validated<VulkanError>> 
        where F: FnOnce(&mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>, &SubbufferAllocator)
    {
        let mut cmd = AutoCommandBufferBuilder::primary(
                self.command_buf_allocator().clone(), self.main_queue_family(), 
                vulkano::command_buffer::CommandBufferUsage::OneTimeSubmit).unwrap();
        
        callback(&mut cmd, &self.subbuffer_allocator);
        
        let submit = cmd.build()?;

        vulkano::sync::now(self.get_device().clone())
            .then_execute(self.main_queue().clone(), submit)
            .unwrap()
            .then_signal_fence_and_flush()
    }

    pub fn descriptor_set_allocator(&self) -> &Arc<StandardDescriptorSetAllocator> {
        &self.descriptor_set_allocator
    }

    pub fn get_subpass(&self, id : RenderSubpass) -> Option<vulkano::render_pass::Subpass> {
        match id {
            RenderSubpass::Normal => Some(vulkano::render_pass::Subpass::from(self.main_renderpass.clone(), 0).unwrap()),
            _ => None,
        }
    }

    pub fn get_num_frames_in_flight(&self) -> usize {
        self.frames_in_flight.len()
    }
} 
