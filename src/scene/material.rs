use std::collections::HashMap;
use std::sync::Arc;

use image::DynamicImage;
use vulkano::{buffer::Subbuffer, descriptor_set::DescriptorSet};
use vulkano::descriptor_set::layout::DescriptorSetLayout;

use vulkano::image::sampler::Sampler;
use vulkano::image::view::ImageView;

use crate::{gen_pool::{GenPool, GenPoolId}, render::Render};

struct ImageSampler {
    image_id : ImageId,
    sampler_id : u32,
}

struct Material {
    bindings : HashMap<u32, ImageSampler>,
    layout_id : Arc<DescriptorSetLayout>,
    descriptor_set : Arc<DescriptorSet>,
    last_frame_used : u64,
}

struct ImageDesc {
    path : String,
    materials : Vec<u32>,
    view : Option<Arc<ImageView>>,
}

type ImageId = GenPoolId<ImageDesc>;

pub struct MaterialManager {
    images : GenPool<ImageDesc>,
    materials : GenPool<Material>,
    samplers : Vec<Arc<Sampler>>,

    //layouts : Vec<Arc<DescriptorSetLayout>>, -- seems useless 
}

impl MaterialManager {
    
    pub fn new() -> Self {
        Self {
            images : GenPool::new(),
            materials : GenPool::new(),
            samplers : Vec::new()
        }
    }

    pub fn load_images(&mut self, render : &Render, image_paths : Vec<String>) -> Vec<ImageId> {
        

        let image_cpu = image::open(image_paths[0].clone()).unwrap(); // .to_rgba8();
        todo!()
    }

    pub fn load_all_images(&mut self, render : &Render) {

    }
}

struct StagingImage {
    buffer : Subbuffer<[u8]>,
    width : u32,
    height : u32,
    // todo: format 
}

impl StagingImage {
    fn load(render : &Render, path : String) -> Self {
        let image_cpu = image::open(path).unwrap().to_rgba8();
        let width = image_cpu.width();
        let height = image_cpu.height();

        let buffer = render
            .staging_buf_allocator()
            .allocate_slice::<u32>((width * height) as u64)
            .unwrap();
        {
            let mut lock = buffer.write().unwrap();

            for y in 0 .. height {
                for x in 0 .. width {
                    let pix = image_cpu.get_pixel(x, y);
                    let val = u32::from_le_bytes(pix.0);

                    lock[(y * width + x) as usize] = val;
                }
            }
        }

        Self {
            buffer : buffer.into_bytes(), width, height,
        }
    }
}
