use std::sync::Arc;

use winit::{
    application::ApplicationHandler, event::WindowEvent, event_loop::EventLoop, window::Window
};


mod vkstate;
mod render;

#[derive(Default)]
struct App {
    window : Option<Arc<Window>>,
    vkstate : Option<vkstate::State>,
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
                
                unsafe {
                    self.vkstate.as_ref().unwrap().device.wait_idle().unwrap();
                }
                
                let _ = self.vkstate.take();
                event_loop.exit();
            },
            WindowEvent::RedrawRequested => {
                // todo: draw logic
                self.window.as_ref().unwrap().request_redraw();
            },
            _ => {},
        }
    }

    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        self.window = Some(Arc::new(event_loop.create_window(Window::default_attributes()).unwrap()));
        
        let resolution = self.window.as_ref().unwrap().inner_size();
        
        self.vkstate = Some(vkstate::State::new_for_rendering(self.window.as_ref().unwrap().clone(), [resolution.width, resolution.height]));
    }
}

fn main() {

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    
    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}


