
use vulkano::pipeline::graphics::viewport::Viewport;

use vulkano::command_buffer::{AutoCommandBufferBuilder, PrimaryAutoCommandBuffer};

use std::sync::{Arc, Mutex};

#[derive(Copy, Clone)]
pub enum RenderSubpass {
    Normal, // color write, depth write + depth test
    Count,
}

pub struct SubpassContext
{
    pub frame_no : usize,
    pub ffid : u8, // frame-in-flight index 
    pub numff : u8, // num frames in flight 
    pub backbuf_id : u8, 
    pub viewport : Viewport, 
}

struct SubpassNode {
    enabled : bool,
    name : String,
    callback : Box<dyn SubpassCallback>,
}

pub trait SubpassCallback : FnMut(&mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>, &SubpassContext) + Send + Sync + 'static {}

impl<T> SubpassCallback for T 
    where T : FnMut(&mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>, &SubpassContext) + Send + Sync + 'static 
{} 


pub struct SubpassHandle {
    id : u32, 
    subpass : RenderSubpass,
    nodes : NodeList,
}

impl SubpassHandle {
    pub fn toggle(&self) {
        self.nodes.access_node_internal(self, |node| { node.as_mut().unwrap().enabled = !node.as_ref().unwrap().enabled;})
    }

    // TODO: more optimal 
    pub fn is_enabled(&self) -> bool {
        self.nodes.access_node_internal(self, |node| { node.as_ref().unwrap().enabled }) 
    }
}

impl Drop for SubpassHandle {
    fn drop(&mut self) {
        self.nodes.access_node_internal(self, |node| { *node = None; }); 
    }
}

#[derive(Clone)]
pub struct NodeList {
    nodes : Arc<Mutex<Vec<Vec<Option<SubpassNode>>>>>,
}


impl NodeList {
    pub fn new() -> Self {
        let subpass_count = RenderSubpass::Count as usize;
        let mut nodes = Vec::with_capacity(subpass_count);

        for i in 0 .. subpass_count {
            nodes.push(Vec::new());
        }

        NodeList { nodes : Arc::new(Mutex::new(nodes)) }
    }

    pub fn register_node(&self, subpass : RenderSubpass, name : String, callback : impl SubpassCallback) 
        -> SubpassHandle
    {
        assert!((subpass as usize) < (RenderSubpass::Count as usize));
        
        let node = SubpassNode {
            enabled : true,
            name, 
            callback : Box::new(callback),
        };
        
        let node_id;
        let mut nodes = self.nodes.lock().unwrap();

        let nodes = &mut nodes[subpass as usize];
        
        match nodes.iter_mut().enumerate().find(|(i, v)| Option::is_none(v)) {
            Some((i, v)) => {
                *v = Some(node);
                node_id = i;
            },
            None => {
                node_id = nodes.len();
                nodes.push(Some(node));
            }
        };
        
        SubpassHandle {
            id : node_id as u32,
            subpass,
            nodes : self.clone()
        }
    } 

    fn access_node_internal<F, R>(&self, hndl : &SubpassHandle, callback : F) -> R
        where F : FnOnce(&mut Option<SubpassNode>) -> R
    {
        let mut lock = self.nodes.lock().unwrap();

        let mut node = &mut lock[hndl.subpass as usize][hndl.id as usize];
        callback(node)
    }
    
    pub fn run_nodes(&self, subpass : RenderSubpass, 
        cmd : &mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>,
        ctx : &SubpassContext) 
    {
        assert!((subpass as usize) < (RenderSubpass::Count as usize));
        let mut nodes = self.nodes.lock().unwrap();
        let list = &mut nodes[subpass as usize];


        list.iter_mut().for_each(|node_opt| {
            match node_opt {
                Some(node) => if node.enabled {
                    (*node.callback)(cmd, ctx);
                },
                None => {}
            }
        });
    }
}

