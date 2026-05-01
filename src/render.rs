
use vulkano::render_pass::RenderPass;

pub enum RenderSubpass {
    Normal = 0, // color write, depth write + depth test
}

pub struct Render
{
    pub renderpass : RenderPass,
}

impl Render {

}
