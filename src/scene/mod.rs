
use gltf::Gltf;

use gltf::json::Path;
use gltf::mesh::util::ReadIndices;
use vulkano::buffer::{BufferContents, Subbuffer};
use vulkano::buffer::allocator::SubbufferAllocator;
use vulkano::command_buffer::{AutoCommandBufferBuilder, BufferCopy, CopyBufferInfo, PrimaryAutoCommandBuffer};
use vulkano::pipeline::graphics::vertex_input::Vertex;
use std::collections::HashMap;
use std::sync::Arc;

use crate::render::mesh_pool::MeshDesc;
use crate::render::{self, Render};
use crate::scene::instances::{InstanceHandle, InstanceManager};

pub mod instances;
pub mod mesh;
pub mod material;

pub struct Material {}

#[repr(C)]
#[repr(align(64))]
#[derive(BufferContents, Clone)]
pub struct InstanceData {
    transform : [f32; 16], // model -> world А электотческт
    inverse_transform : [f32; 16], // for normals
}

pub struct Instance {
    pub mesh_id : u32,
    pub material_id : u32,
    pub handle : InstanceHandle<InstanceData>,
}

pub struct Primitive {
    pub mesh_id : u32,
    pub material_id : u32,
}

pub struct Node {
    pub name : String,
    pub transform : na::Matrix4<f32>, 
    pub inst_data : Option<InstanceHandle<InstanceData>>,
    pub children : Vec<u32>,
    pub parent : u32,
    pub primitives : Vec<Primitive>,
}

pub struct Scene {
    pub meshes : Vec<mesh::SceneMesh>,
    pub materials : Vec<Material>,
    pub instance_manager : Arc<InstanceManager<InstanceData>>,
    pub nodes : Vec<Option<Node>>,
}


struct ResImportList {
    // mesh id -> Scene Nodes  
    meshes : HashMap<u32, Vec<u32>>,
    // materials 
    // 
}

impl Scene {
    pub fn new(render : &Render, max_instances : u64) -> Scene {
        Scene {
            meshes : Vec::new(),
            materials : Vec::new(),
            instance_manager : Arc::new(InstanceManager::new(render, max_instances)),
            nodes : vec![Some(
                Node {
                    name : String::from("scene_root"),
                    transform : na::one(),
                    inst_data : None,
                    children : Vec::new(),
                    parent : 0u32, // self id
                    primitives : Vec::new(),
                }
            )],
        }
    }

    pub fn add_node(&mut self, parent : u32, name : String, transform : na::Matrix4<f32>) -> u32 {
        assert!(self.nodes.len() > 0 && (parent as usize) < self.nodes.len()); // at least root node must be available
        assert!(self.nodes[parent as usize].is_some());
        
        let new_node = Node {
            name, 
            transform,
            inst_data : None,
            children : Vec::new(),
            parent,
            primitives : Vec::new(),
        };

        let new_id = match self.nodes.iter_mut().enumerate().find(|(i, v)| v.is_none()){
            Some((i, n)) => {
                *n = Some(new_node);
                i as u32
            },
            None => {
                self.nodes.push(Some(new_node));
                (self.nodes.len() - 1) as u32
            }
        };
        
        self.nodes[parent as usize].as_mut().unwrap().children.push(new_id);
        new_id
    }
    
    pub fn remove_node(&mut self, id : u32) {

    }

    pub fn modify_node<F, R>(&mut self, id : u32, cb : F) -> R
        where F : FnOnce(&mut Node) -> R 
    {
        assert!((id as usize) < self.nodes.len());
        cb(self.nodes[id as usize].as_mut().unwrap())
    }
    
    fn import_gltf_node(&mut self, parent : u32, gltf_node : gltf::Node, resources : &mut ResImportList) {
        let transform = gltf_node.transform().matrix().into();
        let node_id = self.add_node(parent, String::from(gltf_node.name().unwrap_or("")), transform);
        
        if let Some(gltf_mesh) = gltf_node.mesh() {
            let import_mesh = gltf_mesh.index() as u32;
            match resources.meshes.get_mut(&import_mesh) {
                Some(node_list) => node_list.push(node_id),
                None => { resources.meshes.insert(import_mesh, vec![node_id]); },
            };

        } 
        
        for child in gltf_node.children() {
            self.import_gltf_node(node_id, child, resources);
        }
    }

    pub fn import_gltf(render : &Render, location : &str) -> Arc<Scene> {
        
        let scene_path = std::path::Path::new(location);
        let scene_dir = scene_path.parent().unwrap();

        let Gltf{document, blob} = Gltf::open(&scene_path).unwrap();
        
        let mut scene = Scene::new(render, 1024);
        
        let mut resource_list = ResImportList {
            meshes : HashMap::new(),
        };

        if let Some(g_scene) = document.default_scene() {
            for root_node in g_scene.nodes() {
                scene.import_gltf_node(0, root_node, &mut resource_list); 
            }
        }

        let buffers = gltf::import_buffers(&document, Some(&scene_dir), blob).unwrap();
        
        let mut mesh_data = Vec::<mesh::MeshData>::new();
        let mut mesh_gltf_index = Vec::<u32>::new();

        for gmesh in document.meshes() {
            let mesh_index = gmesh.index() as u32;    
            
            if resource_list.meshes.get(&mesh_index).is_none() {
                continue;
            }

            for gprim in gmesh.primitives() {
                mesh_data.push(mesh::load_gltf_prim_data(gprim, &buffers));
                mesh_gltf_index.push(mesh_index);
            }
        } 
        
        scene.meshes = mesh::load_meshes(render, mesh_data);
     
        for (i, _) in scene.meshes.iter().enumerate() {
            let orig_index = mesh_gltf_index[i];
            let node_ids = resource_list.meshes.get(&orig_index).unwrap();
            for node_id in node_ids {
                let node = scene.nodes[*node_id as usize].as_mut().unwrap();

                node.primitives.push(Primitive {
                    mesh_id : i as u32,
                    material_id : 0,
                });
            }
        }

        let instance_manager = scene.instance_manager.clone();

        scene.walk_nodes_mut(0, |node, id, transform| {
            if node.primitives.len() == 0{
                return;
            }

            let matrix_data = InstanceData {
                transform : transform.as_slice().try_into().unwrap(),
                inverse_transform : transform.try_inverse().unwrap().as_slice().try_into().unwrap(),
            };
            
            node.inst_data = Some(instance_manager.alloc(matrix_data));
        });
 
        Arc::new(scene)
    }

    fn walk_nodes_mut_internal<F>(&mut self, start_node : u32, cb : &mut F, initial_transform : &na::Matrix4<f32>) 
        where F : FnMut(&mut Node, u32, &na::Matrix4<f32>)
    {
        // node 0 - root - do not change
        assert!(self.nodes.len() > (start_node as usize) && self.nodes[start_node as usize].is_some());
        let node = self.nodes[start_node as usize].as_mut().unwrap();

        let transform = initial_transform * node.transform;

        cb(node, start_node, &transform);

        for i in 0 .. node.children.len() {
            let id = self.nodes[start_node as usize].as_ref().unwrap().children[i];
            self.walk_nodes_mut_internal(id, cb, &transform);
        }
    }
    
    pub fn walk_nodes_mut<F>(&mut self, start_node : u32, mut cb : F) 
        where  F : FnMut(&mut Node, u32, &na::Matrix4<f32>)
    {
        let transform : na::Matrix4<f32> = na::one();
        self.walk_nodes_mut_internal(start_node, &mut cb, &transform);
    }

    fn walk_nodes_internal<F>(&self, start_node : u32, cb : &mut F, initial_transform : &na::Matrix4<f32>) 
        where F : FnMut(&Node, u32, &na::Matrix4<f32>)
    {
        // node 0 - root - do not change
        assert!(self.nodes.len() > (start_node as usize) && self.nodes[start_node as usize].is_some());
        let node = self.nodes[start_node as usize].as_ref().unwrap();

        let transform = initial_transform * node.transform;

        cb(node, start_node, &transform);

        for id in node.children.iter() {
            self.walk_nodes_internal(*id, cb, &transform);
        }
    }
    
    pub fn walk_nodes<F>(&self, start_node : u32, mut cb : F) 
        where  F : FnMut(&Node, u32, &na::Matrix4<f32>)
    {
        let transform : na::Matrix4<f32> = na::one();
        self.walk_nodes_internal(start_node, &mut cb, &transform);
    }

}
