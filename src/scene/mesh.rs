
use std::sync::Arc;

use vulkano::command_buffer::{BufferCopy, CopyBufferInfo};
use vulkano::pipeline::graphics::vertex_input::Vertex;
use vulkano::buffer::BufferContents;
use vulkano::buffer::{Subbuffer, allocator::SubbufferAllocator};

use gltf::mesh::util::ReadIndices;

use crate::render::Render;
use crate::render::mesh_pool::MeshDesc;

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

pub enum IndexBufferProxy {
    None,
    U8(Vec<u8>),
    U16(Vec<u16>),
    U32(Vec<u32>),
}

pub struct MeshData {
    pub verts : Vec<VertexPNUV>,
    pub indices : IndexBufferProxy,
    pub bmin : na::Point3<f32>,
    pub bmax : na::Point3<f32>,
}

impl MeshData {
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
   pub bmin : na::Point3<f32>,
   pub bmax : na::Point3<f32>, 
}


pub fn load_gltf_prim_data<'a>(prim : gltf::Primitive<'a>, g_bufs : &Vec<gltf::buffer::Data>) -> MeshData {
    let cb = |b : gltf::buffer::Buffer| {
        //get_buffers(buf)
        Some(g_bufs[b.index()].0.as_slice())
    };
    
    let mut mesh = MeshData {
        verts : Vec::new(),
        indices : IndexBufferProxy::None,
        bmin : na::point![0f32, 0f32, 0f32],
        bmax : na::point![0f32, 0f32, 0f32],
    };
                
    let bounds = prim.attributes().find_map(|attr| {
        if attr.0 == gltf::Semantic::Positions && attr.1.min().is_some() && attr.1.max().is_some() {
            Some((attr.1.min().unwrap(), attr.1.max().unwrap()))
        } else {
            None
        }
    });
                
    match bounds {
        Some((vmin, vmax)) => {
            for i in 0 .. 3 {
                mesh.bmin[i] = vmin.get(i)
                    .and_then(|v| v.as_f64())
                    .and_then(|v| Some(v as f32))
                    .unwrap_or(0f32);
                mesh.bmax[i] = vmax.get(i)
                    .and_then(|v| v.as_f64())
                    .and_then(|v| Some(v as f32))
                    .unwrap_or(0f32);
            }
        },
        None => {},
    };

    let reader = prim.reader(cb);
    
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
        None => {},
    } 
    
    if reader.read_positions().is_none() {
        return mesh;
    }
        
    let mut pos_iter = reader.read_positions().unwrap();
    let num_verts = pos_iter.len();
        
    if num_verts == 0 {
        return mesh;
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

pub fn load_meshes(render : &Render, src : Vec<MeshData>) -> Vec<SceneMesh> {
    let mut result = Vec::<SceneMesh>::with_capacity(src.len());
    
    for src_mesh in src.iter() {
        let vertex_stride = VertexPNUV::per_vertex().stride;
        let index_size = src_mesh.index_size_bytes();
        let num_indices = src_mesh.index_count() as u32;
        let num_verts = src_mesh.verts.len() as u32;
        let mesh = render.mesh_pool().alloc_mesh(vertex_stride, index_size, num_verts, num_indices);

        result.push(SceneMesh {
            mesh_desc : mesh,
            bmin : src_mesh.bmin,
            bmax : src_mesh.bmax
        });
    }
    
    render.run_async_commands(|cmd, alloc| {
        for (i, src_mesh) in src.into_iter().enumerate() {
            let dst_mesh = &result[i]; 
                
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

    result
}

