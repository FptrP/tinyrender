
use std::sync::{Arc, Mutex};

use std::sync::atomic::AtomicU64;

use std::ops::Range;
use nalgebra::{OPoint, allocator};
use vulkano::DeviceSize;
use vulkano::buffer::{Buffer, BufferCreateInfo, BufferUsage, Subbuffer};
use vulkano::device::Device;
use vulkano::memory::allocator::{AllocationCreateInfo, MemoryTypeFilter, StandardMemoryAllocator};

// FrameCleanupSystem
// 

pub struct MeshDesc {
    num_indices : u32,
    num_verts : u32,
    vpool_id : u32,
    vertex_offset : u32,
    index_offset : u32, 
    index_size : u8, // 0, 1, 2 or 4 bytes
    vertex_stride : u32,
    last_frame_used : AtomicU64,
    pool : MeshPool,
}

impl MeshDesc {
    pub fn num_verts(&self) -> u32 { self.num_verts }
    pub fn num_indices(&self) -> u32 { self.num_indices }
    pub fn vpool_id(&self) -> u32 { self.vpool_id }
    pub fn vertex_offset(&self) -> u32 { self.vertex_offset }
    pub fn vertex_stride(&self) -> u32 { self.vertex_stride }
    pub fn vertex_byte_offset(&self) -> u64 { (self.vertex_stride * self.vertex_offset) as u64 }
    pub fn index_offset(&self) -> u32 { self.index_offset }
    pub fn index_size_bytes(&self) -> u8 { self.index_size }
    pub fn index_offset_bytes(&self) -> u64 { (self.index_size as u64) * (self.index_offset as u64) } 

    pub fn has_indices(&self) -> bool { self.num_indices != 0 }
    pub fn update_lfu(&self, lfu : u64) { self.last_frame_used.fetch_max(lfu, std::sync::atomic::Ordering::Relaxed); }

    pub fn vertex_buffer(&self) -> Option<Subbuffer<[u8]>> {
        if self.num_verts == 0 {
            return None;
        }
        let pool = self.pool.inner.lock().unwrap();
        Some(pool.vertex_pools[self.vpool_id as usize]
            .as_ref().unwrap().buffer.clone())
    }

    pub fn index_buffer(&self) -> Option<Subbuffer<[u8]>> {
        if !self.has_indices() {
            return None;
        }

        let pool = self.pool.inner.lock().unwrap();
        Some(pool.index_pool.buffer.clone())
    }
}

impl Drop for MeshDesc {
    fn drop(&mut self) {
        let vbuf_offset = self.vertex_offset * self.vertex_stride;
        let vbuf_end = vbuf_offset + self.num_verts * self.vertex_stride;
        
        let vertex_bytes = vbuf_offset .. vbuf_end;
 
        let index_start = self.index_offset * self.index_size as u32;
        let index_end = index_start + self.num_indices * self.index_size as u32;

        let mut pool = self.pool.inner.lock().unwrap();
        pool.garbage.push(PendingMesh { 
            vpool_id : self.vpool_id,
            vert_bytes : vertex_bytes,
            index_bytes : index_start .. index_end,
            last_frame_used : self.last_frame_used.load(std::sync::atomic::Ordering::Relaxed),
        });
    }
}

struct PendingMesh
{
    vpool_id : u32,
    vert_bytes : Range<u32>,    
    index_bytes : Range<u32>,
    last_frame_used : u64,
}

struct BufferPool {
    buffer : Subbuffer<[u8]>,
    free_list : Vec<Range<u32>>,
    used_list : Vec<Range<u32>>
}

impl BufferPool {
    fn new(allocator : Arc<StandardMemoryAllocator>, buffer_size : u32, is_index : bool) -> Self {
        
        let usage = if is_index { BufferUsage::INDEX_BUFFER } else { BufferUsage::VERTEX_BUFFER };

        let buffer = Buffer::new_slice::<u8>(
            allocator.clone(),
            BufferCreateInfo {
                usage : BufferUsage::TRANSFER_DST | usage,
                ..Default::default()
            }, 
            AllocationCreateInfo {
                memory_type_filter : MemoryTypeFilter::PREFER_DEVICE,
                ..Default::default()
            }, 
            buffer_size as DeviceSize).unwrap(); 
        
        Self {
            buffer,
            free_list : vec![0u32..buffer_size],
            used_list : Vec::new(),
        }
    }

    // returns offset in elements on successfull allocation
    fn try_allocate(&mut self, elem_size : u32, num_elems : u32) -> Option<u32> {
        
        let byte_size = elem_size * num_elems;
        assert!(byte_size > 0);

        let res = self.free_list.iter().enumerate().filter(|(i, r)| {
            let aligned_start = (r.start + elem_size - 1u32)/elem_size * elem_size; 
            aligned_start < r.end && (r.end - aligned_start) >= byte_size
        }).min_by_key(|(i, r)| { r.end - r.start });
        

        res
            .map(|(i, r)| { (i, r.clone()) })
            .map(|(i, r)| {
            // remove elem i, replace it with  
            let aligned_start = (r.start + elem_size - 1u32)/elem_size * elem_size; 
            let alloc_end = aligned_start + byte_size;
            
            self.used_list.push(aligned_start..alloc_end);
            let mut index = i;
            let _ = self.free_list.remove(index);
            
            if aligned_start > r.start {
                self.free_list.insert(index, r.start..aligned_start);            
                index += 1;
            }  
            
            if alloc_end < r.end {
                //Some(alloc_end..r.end)
                self.free_list.insert(index, r.start..aligned_start);
            }
            
            
            aligned_start / elem_size
        })
    }
    

    fn try_free(&mut self, exp_range : Range<u32>) -> bool {
        let alloc_range = self.used_list.iter().enumerate()
            .find_map(|(i, r)| {
                if r.clone() == exp_range {
                    Some((i, r.clone()))
                } else {
                    None
                }
            });
        
        if alloc_range.is_none() {
            return false;
        }
        
        let _ = self.used_list.remove(alloc_range.unwrap().0);

                       
        if self.free_list.is_empty() {
            self.free_list.push(exp_range);
            return true;
        }

        let dst = self.free_list.iter().enumerate().find_map(|(i, r)| {
            if exp_range.end <= r.start {
                Some(i)
            } else {
                None
            }
        });
                
        match dst {
            Some(0) => {
                let right = self.free_list[0].clone();
                if exp_range.end == right.start {
                    self.free_list[0] = exp_range.start..right.end;
                } else {
                    self.free_list.insert(0, exp_range);
                }
            },
            Some(i) => {
                let right = self.free_list[i].clone();
                let left = self.free_list[i - 1].clone();

                if left.end == exp_range.start && exp_range.end == right.start {
                    self.free_list[i - 1] = left.start .. right.end;
                    self.free_list.remove(i);
                } else if left.end == exp_range.start {
                    self.free_list[i - 1] = left.start .. exp_range.end;
                } else if exp_range.end == right.start {
                    self.free_list[i] = exp_range.start .. right.end;
                } else {
                    self.free_list.insert(i, exp_range);
                }
            },
            None => {
                let left = self.free_list.last().unwrap();
                if left.end == exp_range.start {
                    *self.free_list.last_mut().unwrap() = left.start .. exp_range.end;
                } else {
                    self.free_list.push(exp_range);
                }
            },
        };
        true
    }
}

struct MeshPoolInternal {
    allocator : Arc<StandardMemoryAllocator>,

    index_pool : BufferPool,
    vertex_pools : Vec<Option<BufferPool>>,
    garbage : Vec<PendingMesh>,

    vertex_pool_size : u32,
    vertex_pool_limit : u32,
    allocated_vert_pools : u32,
}

impl MeshPoolInternal {
    fn alloc_verts(&mut self, vert_size : u32, vert_count : u32) -> Option<(u32, u32)> {
        let alloc = self.vertex_pools.iter_mut().enumerate().find_map(|(i, vpool)| {
            match vpool {
                Some(valloc) => valloc.try_allocate(vert_size, vert_count).map(|offs| (i as u32, offs)),
                None => None,
            }
        });
        
        if alloc.is_some() {
            return alloc;
        }

        // try extend if possible 
        if self.allocated_vert_pools >= self.vertex_pool_limit {
            return None;
        }
        
        let free_cell = self.vertex_pools.iter().enumerate().find_map(|(i, vpool)| {
            if vpool.is_none() { Some(i) } else { None }
        });
        
        let vbuffer = BufferPool::new(self.allocator.clone(), self.vertex_pool_size, false);

        match free_cell {
            Some(i) => self.vertex_pools[i] = Some(vbuffer),
            None => self.vertex_pools.push(Some(vbuffer)),
        }
        
        self.alloc_verts(vert_size, vert_count)
    }
}

#[derive(Clone)]
pub struct MeshPool {
    inner : Arc<Mutex<MeshPoolInternal>>
}

impl MeshPool {
    pub fn new(allocator : Arc<StandardMemoryAllocator>, index_pool_size : u32, vertex_pool_size : u32, vertex_pool_limit : u32) -> Self {
       let index_pool = BufferPool::new(allocator.clone(), index_pool_size, true); 
       let vertex_pool = BufferPool::new(allocator.clone(), vertex_pool_size, false);
       
       let pool = MeshPoolInternal {
           allocator : allocator,
           index_pool,
           vertex_pools : vec![Some(vertex_pool)],
           garbage : Vec::new(),
           vertex_pool_size,
           vertex_pool_limit,
           allocated_vert_pools : 1u32,
       };

        Self {
            inner : Arc::new(Mutex::new(pool))
        }
    }
    
    pub fn collect_garbage(&self, safe_frame : u64) {
        let mut pool = self.inner.lock().unwrap();
        let removed_elems = pool.garbage.extract_if(.., |desc| desc.last_frame_used <= safe_frame).collect::<Vec<_>>();
        
        for elem in removed_elems {
            if !elem.index_bytes.is_empty() {
                let removed = pool.index_pool.try_free(elem.index_bytes);
                assert!(removed);
            }
            
            if !elem.vert_bytes.is_empty() {
                let removed = pool.vertex_pools[elem.vpool_id as usize].as_mut().unwrap().try_free(elem.vert_bytes);
                assert!(removed);
            }
        }
    }
    
    pub fn alloc_mesh(&self, vertex_stride : u32, index_stride : u8, vertex_count : u32, index_count : u32) -> Arc<MeshDesc> {
        assert!(index_stride == 0 || index_stride == 1 || index_stride == 2 || index_stride == 4);
        let has_indices = index_count * index_stride as u32 > 0;
        let has_vertices = vertex_count * vertex_stride > 0; 
        assert!(has_vertices || has_indices);
        
        let mut pool = self.inner.lock().unwrap();

        let index_offset = if has_indices {
            pool.index_pool.try_allocate(index_stride as u32, index_count)
                .expect("Index buffer - out of memory")
        } else {
            0u32
        };
        
        let (vpool_id, vertex_offset) = if has_vertices {
            pool.alloc_verts(vertex_count, vertex_stride).expect("Vertex buffer - out of memory")
        } else {
            (0u32, 0u32)
        };

        let mesh_desc = MeshDesc {
            num_indices : index_count,
            num_verts : vertex_count,
            vpool_id,
            vertex_offset,
            index_offset,
            index_size : index_stride,
            vertex_stride,
            last_frame_used : AtomicU64::new(0),
            pool : self.clone(),
        };


        Arc::new(mesh_desc)
    }

}


// Arc<Mesh> -> Arc<MeshPool> -> offsets in buffer 
// How to delete mesh? 
// If mesh stores frame_no 

