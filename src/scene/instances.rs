
use std::sync::{Arc, Mutex};
use vulkano::buffer::{BufferContents, BufferCreateInfo, BufferUsage, Subbuffer};
use vulkano::buffer::Buffer;
use vulkano::command_buffer::{BufferCopy, CopyBufferInfo};
use vulkano::memory::allocator::{AllocationCreateInfo, MemoryTypeFilter};

use crate::render::{self, Render};
use crate::render::subpass_node::{RenderSubpass, SubpassHandle};

struct AllocState<T> {
    val : T,
    dirty : bool,
}

type InstList<T> = Arc<Mutex<Vec<Option<AllocState<T>>>>>;

pub struct InstanceManager<T> {
    max_instances : u64, 
    buffer : Subbuffer<[T]>,
    instance_states : InstList<T>, 
    update_instances : SubpassHandle,
}

pub struct InstanceHandle<T> {
   index : usize,
   nodes : InstList<T>,
}

impl<T> InstanceManager<T> 
    where T : BufferContents + Clone,
{
    
    pub fn new(render : &Render, max_instances : u64) -> Self {
        
        let buffer = Buffer::new_slice::<T>(
            render.mem_allocator().clone(), 
            BufferCreateInfo {
                usage : BufferUsage::UNIFORM_BUFFER | BufferUsage::TRANSFER_DST,
                ..Default::default()
            }, 
            AllocationCreateInfo {
                memory_type_filter : MemoryTypeFilter::PREFER_DEVICE, 
                ..Default::default()
            }, 
            max_instances).unwrap();
        
        let instance_states = Arc::new(Mutex::new(Vec::<Option<AllocState<T>>>::with_capacity(max_instances as usize)));
        
        let update_instances = {
            let instance_states = instance_states.clone();
            let buffer = buffer.clone();
            
            render.register_node(RenderSubpass::BeforeRender, String::from("prepare_instances"), move |cmd, ctx| {
                let mut instances = instance_states.lock().unwrap();

                for (index, inst) in instances.iter_mut().enumerate() {
                    if inst.is_none() { continue; }
                    let inst = inst.as_mut().unwrap();

                    if !inst.dirty { continue; }
                    
                    let staging = ctx.staging_allocator.allocate_sized::<T>().unwrap();
                    let mut wr = staging.write().unwrap();
                    *wr = inst.val.clone();
                    
                    cmd.copy_buffer(CopyBufferInfo {
                        regions : [
                            BufferCopy {
                                dst_offset : (size_of::<T>() * index) as u64,
                                size : size_of::<T>() as u64,
                                ..Default::default()
                            }
                        ].into(),
                        ..CopyBufferInfo::buffers(staging.clone(), buffer.clone())
                    }).unwrap();
                    inst.dirty = false;
                }
            })
        };

        Self {
            max_instances,
            buffer,
            instance_states,
            update_instances,
        }
    }

    pub fn alloc(&self, val : T) -> InstanceHandle<T> {
        let mut instances = self.instance_states.lock().unwrap();
        let place = instances.iter_mut().enumerate().find(|(i, v)| v.is_none());
        let id = match place {
            Some((i, v)) => {
                *v = Some(AllocState {
                    val,
                    dirty : true,
                });
                i
            },
            None => {
                instances.push(Some(AllocState { val, dirty : true }));
                instances.len() - 1
            }
        };
        
        std::mem::drop(instances);

        InstanceHandle {
            index : id,
            nodes : self.instance_states.clone() 
        }
    }

    pub fn instance_buffer(&self) -> Subbuffer<[T]> { self.buffer.clone() }
}

impl<T> Drop for InstanceHandle<T>
{
    fn drop(&mut self) {
        let mut lock = self.nodes.lock().unwrap();
        lock[self.index] = None;
    }
}

impl<T> InstanceHandle<T>
    where T : BufferContents + Clone 
{
    pub fn update(&self, val : T) {
        let mut lock = self.nodes.lock().unwrap();
        *lock[self.index].as_mut().unwrap() = AllocState {
            val, 
            dirty : true
        };
    }

    pub fn offset_bytes(&self) -> u64 {
        (self.index as u64) * (size_of::<T>() as u64)
    }
}       
