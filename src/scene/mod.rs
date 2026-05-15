
use gltf::Gltf;

use gltf::json::Path;
use gltf::json::extensions::mesh;
use gltf::mesh::util::ReadIndices;
use vulkano::buffer::{BufferContents, Subbuffer};
use vulkano::buffer::allocator::SubbufferAllocator;
use vulkano::command_buffer::{AutoCommandBufferBuilder, BufferCopy, CopyBufferInfo, PrimaryAutoCommandBuffer};
use vulkano::pipeline::graphics::vertex_input::Vertex;
use std::sync::Arc;

use crate::render::mesh_pool::MeshDesc;
use crate::render::{self, Render};
use crate::scene::instances::{InstanceHandle, InstanceManager};

pub mod instances;

#[repr(C)]
#[derive(BufferContents, Vertex, Clone)]
pub struct VertexPNUV {
    #[format(R32G32B32_SFLOAT)]
    pub pos : [f32; 3],
    #[format(R32G32B32_SFLOAT)]
    pub norm : [f32; 3],
    #[format(R32G32_SFLOAT)]
    pub uv : [f32; 2],
}

enum IndexBufferProxy {
    None,
    U8(Vec<u8>),
    U16(Vec<u16>),
    U32(Vec<u32>),
}

struct ImportedMesh {
    verts : Vec<VertexPNUV>,
    indices : IndexBufferProxy
}



impl ImportedMesh {
    fn new<'a, 'b, F>(reader : gltf::mesh::Reader<'a, 'b, F>) -> Self 
        where F : Clone + Fn(gltf::Buffer<'a>) -> Option<&'b [u8]>
    {
        let mut mesh = ImportedMesh {
            verts : Vec::new(),
            indices : IndexBufferProxy::None, 
        };
       
        match reader.read_indices() { 
            Some(ReadIndices::U8(iter)) => {
                mesh.indices = IndexBufferProxy::U8(iter.collect());
            },
            Some(ReadIndices::U16(iter)) => {
                mesh.indices = IndexBufferProxy::U16(iter.collect());
            },
            Some(ReadIndices::U32(iter)) => {
                mesh.indices = IndexBufferProxy::U32(iter.collect());
            },
            None => {
            },
        }
        
        if reader.read_positions().is_none() {
            return mesh;
        }
        
        let mut pos_iter = reader.read_positions().unwrap();
        let num_verts = pos_iter.len();
        
        if num_verts == 0 {
            return  mesh;
        }
        
        let mut norm_iter = reader.read_normals();
        let mut uv_iter = reader.read_tex_coords(0).map(|iter| iter.into_f32());

        mesh.verts.reserve(num_verts);
        
        for _ in 0 .. num_verts {
            let pos = pos_iter.next().unwrap();
            let norm = match &mut norm_iter {
                Some(iter) => iter.next().unwrap(),
                None => [0f32, 0f32, 0f32],
            };
            
            let uv = match &mut uv_iter {
                Some(iter) => iter.next().unwrap(),
                None => [0f32, 0f32]
            };
            mesh.verts.push(VertexPNUV {
                pos, norm, uv
            });
        }
    
        mesh
    }

    fn index_size_bytes(&self) -> u8 {
        match self.indices {
            IndexBufferProxy::None => 0u8,
            IndexBufferProxy::U8(_) => 1u8,
            IndexBufferProxy::U16(_) => 2u8,
            IndexBufferProxy::U32(_) => 4u8,
        }
    }

    fn index_count(&self) -> usize {
        match &self.indices {
            IndexBufferProxy::None => 0,
            IndexBufferProxy::U8(v) => v.len(),
            IndexBufferProxy::U16(v) => v.len(),
            IndexBufferProxy::U32(v) => v.len(),
        }
    }

    fn vertex_count(&self) -> usize {
        self.verts.len()
    }
}

pub struct SceneMesh {
   pub mesh_desc : Arc<MeshDesc>,
   //vertex format? 
   //
   //
}

pub struct Material {
}

#[repr(C)]
#[repr(align(64))]
#[derive(BufferContents, Clone)]
pub struct InstanceData {
    transform : [f32; 16], // model -> world 
    inverse_transform : [f32; 16], // for normals
}

pub struct Instance {
    pub mesh_id : u32,
    pub material_id : u32,
    pub handle : InstanceHandle<InstanceData>,
}

pub struct Scene {
    pub meshes : Vec<SceneMesh>,
    pub materials : Vec<Material>,
    pub instances : Vec<Instance>,
    pub instance_manager : InstanceManager<InstanceData>,
}

fn alloc_upload_staging<T>(alloc : &SubbufferAllocator, data : &[T]) -> Subbuffer<[u8]> 
    where T : BufferContents + Clone,
{
    //let size = size_of::<T>() * data.len();

    let staging = alloc.allocate_slice::<T>(data.len() as u64).unwrap();
    {
        let mut writer = staging.write().unwrap();
        for (i, v) in data.iter().enumerate() {
            writer[i] = v.clone();
        }
    }
    staging.into_bytes()
}

impl Scene {
    pub fn new(render : &Render, max_instances : u64) -> Scene {
        Scene {
            meshes : Vec::new(),
            materials : Vec::new(),
            instances : Vec::new(),
            instance_manager : InstanceManager::new(render, max_instances)
        }
    }

    pub fn import_gltf(render : &Render, location : &str) -> Arc<Scene> {
        
        let scene_path = std::path::Path::new(location);
        let scene_dir = scene_path.parent().unwrap();

        let Gltf{document, blob} = Gltf::open(&scene_path).unwrap();
        let buffers = gltf::import_buffers(&document, Some(&scene_dir), blob).unwrap();

        let get_buffer_data = |b : gltf::buffer::Buffer| {
            Some(buffers[b.index()].0.as_slice())
        };
        
        let mut imported_meshes = Vec::new();

        for src_mesh in document.meshes() {
            for prim in src_mesh.primitives() {
                
                
                //prim.indices().unwrap().index()

                let reader = prim.reader(get_buffer_data);
                let mesh = ImportedMesh::new(reader);
                imported_meshes.push(mesh);
            }
            break; // 1 mesh for now 
        }

        let mut scene = Scene::new(render, 1024);
        scene.meshes.reserve(imported_meshes.len());
        
        for src_mesh in imported_meshes.iter() {
            let vertex_stride = VertexPNUV::per_vertex().stride;
            let index_size = src_mesh.index_size_bytes();
            let num_indices = src_mesh.index_count() as u32;
            let num_verts = src_mesh.verts.len() as u32;
            let mesh = render.mesh_pool().alloc_mesh(vertex_stride, index_size, num_verts, num_indices);

            scene.meshes.push(SceneMesh {
                mesh_desc : mesh
            });
        }

        render.run_async_commands(|cmd, alloc| {
            for (i, src_mesh) in imported_meshes.into_iter().enumerate() {
                let dst_mesh = &scene.meshes[i]; 
                
                if dst_mesh.mesh_desc.num_verts() != 0 {
                    let staging = alloc_upload_staging(alloc, &src_mesh.verts); 
                    
                    cmd.copy_buffer(CopyBufferInfo {
                        regions : [
                            BufferCopy {
                                src_offset : 0,
                                dst_offset : dst_mesh.mesh_desc.vertex_byte_offset(),
                                size : staging.size(),
                                ..Default::default()
                            }
                        ].into(),
                        ..CopyBufferInfo::buffers(staging, dst_mesh.mesh_desc.vertex_buffer().unwrap())
                   }).unwrap();
    
                }
                
                let index_staging = match src_mesh.indices {
                    IndexBufferProxy::None => None,
                    IndexBufferProxy::U8(v) => Some(alloc_upload_staging(alloc, &v)),
                    IndexBufferProxy::U16(v) => Some(alloc_upload_staging(alloc, &v)),
                    IndexBufferProxy::U32(v) => Some(alloc_upload_staging(alloc, &v)),
                };

                if let Some(index_staging) = index_staging {
                    cmd.copy_buffer(CopyBufferInfo {
                        regions : [
                            BufferCopy {
                                dst_offset : dst_mesh.mesh_desc.index_offset_bytes(),
                                size : index_staging.size(),
                                ..Default::default()
                            }
                        ].into(),
                        ..CopyBufferInfo::buffers(index_staging, dst_mesh.mesh_desc.index_buffer().unwrap())
                    }).unwrap();

                }

            }
        }).unwrap().wait(None).unwrap();
        
        let data = InstanceData {
            transform : na::one::<na::Matrix4<f32>>().as_slice().try_into().unwrap(),
            inverse_transform : na::one::<na::Matrix4<f32>>().as_slice().try_into().unwrap(),
        };
        
        for (i, _) in scene.meshes.iter().enumerate() {
            let handle = scene.instance_manager.alloc(data.clone()); 
            
            scene.instances.push(Instance {
                mesh_id : i as u32, 
                material_id : 0,
                handle
            });
        }

        // TODO: add one instance 

        //scene.instance_manager.alloc(val)

        Arc::new(scene)
    }

}
