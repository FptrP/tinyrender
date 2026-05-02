use std::{sync::Arc, time::Instant};

use winit::{
    application::ApplicationHandler, dpi::{PhysicalSize, Size}, event::WindowEvent, event_loop::EventLoop, window::{Window, WindowAttributes}
};


use crate::render::Render;


mod vkstate;
mod render;
mod triangle;

#[derive(Default)]
struct App {
    window : Option<Arc<Window>>,
    render : Option<Render>,
    tri : Option<triangle::TrianglePass>,
    app_start : Option<std::time::Instant>,
}

impl ApplicationHandler for App {

    fn window_event(
            &mut self,
            event_loop: &winit::event_loop::ActiveEventLoop,
            window_id: winit::window::WindowId,
            event: winit::event::WindowEvent,
        ) {

        match  event {
            WindowEvent::CloseRequested => {
                println!("[App] close");
                
                event_loop.exit();
            },
            WindowEvent::RedrawRequested => {
                // todo: draw logic
                //
                let render = self.render.as_mut().unwrap();
                let window = self.window.as_ref().unwrap();
                
                let tri = self.tri.as_ref().unwrap();
                let elapsed_s = self.app_start.as_ref().unwrap().elapsed().as_secs_f32();

                {
                    let mut color = tri.tri_color.lock().unwrap();
                    color[0] = 0.5 * f32::cos(elapsed_s) + 0.5;
                    color[1] = 0.5 * f32::sin(elapsed_s) + 0.5; 
                }

                render.draw_frame();
                
                if render.recreate_swapchain {
                    let res = [window.inner_size().width,
                        window.inner_size().height];
                    render.recreate_swapchain(res);
                }
                //render.recreate_swapchain()
                self.window.as_ref().unwrap().request_redraw();
            },
            _ => {},
        }
    }

    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {

        let winfo = Window::default_attributes()
            .with_inner_size(PhysicalSize::new(640, 480));

        self.window = Some(Arc::new(event_loop.create_window(winfo).unwrap()));
        
        let resolution = self.window.as_ref().unwrap().inner_size();
        let vkstate = vkstate::State::new_for_rendering(self.window.as_ref().unwrap().clone(), [resolution.width, resolution.height]);
        

        self.render = Some(Render::new(vkstate));
        self.tri = Some(
            triangle::TrianglePass::new(
                self.render.as_ref().unwrap()
                ));

        self.app_start = Some(Instant::now());
    }
}

fn main() {

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    
    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}


