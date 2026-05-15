extern crate nalgebra as na;

use std::{sync::Arc, time::Instant};
use std::sync::RwLock;

use na::{Matrix4, Vector3};
use winit::event::KeyEvent;
use winit::{
    application::ApplicationHandler, dpi::{PhysicalSize, Size}, event::WindowEvent, event_loop::EventLoop, keyboard::{KeyCode, PhysicalKey}, window::{Window, WindowAttributes}
};



use crate::scene_render::SceneRenderer;
use crate::{camera_controller::CameraController, render::Render};

use crate::scene::Scene;


mod vkstate;
mod render;
mod triangle;
mod scene;
mod scene_render;
mod camera_controller;

#[derive(Default, Clone)]
pub struct RenderGlobalParams {
    pub view : Matrix4<f32>,
    pub projection : Matrix4<f32>,
    pub view_projection : Matrix4<f32>,
}

#[repr(usize)]
enum CameraMove {
    Forward = 0,
    Backward = 1,
    Left = 2, 
    Right = 3,
    Up = 4,
    Down = 5,
}

//#[derive(Default)]
struct App {
    window : Option<Arc<Window>>,
    render : Option<Render>,
    tri : Option<triangle::TrianglePass>,
    app_start : Option<std::time::Instant>,
    prev_frame_start : Option<std::time::Instant>,
    camera : CameraController,
    pressed_directions : [bool; 16],
    render_params : Arc<RwLock<RenderGlobalParams>>,
    mouse_look : bool,
    scene : Option<Arc<Scene>>,
    scene_render : Option<SceneRenderer>,
    scene_path : String,
}

impl App {
    fn camera_update(&mut self, dt : f32) {
        
        let dir_move = |forward, backward| {
            let f = if forward { 1.0 } else { 0.0 };
            let b = if backward { 1.0 } else { 0.0 };
            f - b 
        };

        let x = dir_move(self.pressed_directions[CameraMove::Right as usize], 
            self.pressed_directions[CameraMove::Left as usize]); 
        let y = dir_move(self.pressed_directions[CameraMove::Up as usize], 
            self.pressed_directions[CameraMove::Down as usize]);
        let z = dir_move(self.pressed_directions[CameraMove::Backward as usize], 
            self.pressed_directions[CameraMove::Forward as usize]);

        let movement = na::vector![x, y, z] * dt;
        
        self.camera.update_position_rel(&movement);
    }
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
            WindowEvent::CursorMoved { position, .. } => {
                let new_pos = na::point![position.x as f32, position.y as f32]; 
                if self.mouse_look {
                    self.camera.update_mouse_pos(new_pos);            
                } else {
                    self.camera.prev_mouse_pos = new_pos;
                }
            },
            WindowEvent::KeyboardInput { event, ..} => {

                let mut handle_axis = |ev : &KeyEvent, axis| {
                    self.pressed_directions[axis as usize] = match ev.state {
                        winit::event::ElementState::Pressed => true,
                        winit::event::ElementState::Released => false,
                    };
                };

                match event.physical_key {
                    PhysicalKey::Code(KeyCode::KeyW) 
                    | PhysicalKey::Code(KeyCode::ArrowUp) => 
                        handle_axis(&event, CameraMove::Forward),
                    PhysicalKey::Code(KeyCode::KeyS) 
                    | PhysicalKey::Code(KeyCode::ArrowDown) => 
                        handle_axis(&event, CameraMove::Backward), 
                    PhysicalKey::Code(KeyCode::KeyA) 
                    | PhysicalKey::Code(KeyCode::ArrowLeft) =>
                        handle_axis(&event, CameraMove::Left), 
                    PhysicalKey::Code(KeyCode::KeyD)
                    | PhysicalKey::Code(KeyCode::ArrowRight) => 
                        handle_axis(&event, CameraMove::Right),
                    PhysicalKey::Code(KeyCode::KeyF) => { 
                        if event.state == winit::event::ElementState::Pressed {
                            self.mouse_look = !self.mouse_look
                        }
                    },
                    _ => (),
                } 
            },
            WindowEvent::RedrawRequested => {
                // todo: draw logic
                //
                
                let elapsed_s = self.app_start.as_ref().unwrap().elapsed().as_secs_f32();
                
                let dt = {
                    let time = self.prev_frame_start.unwrap_or_else(Instant::now);
                    let dt = time.elapsed().as_secs_f32();
                    self.prev_frame_start = Some(Instant::now());
                    dt
                };
                
                println!("FPS : {}", 1.0/dt);

                self.camera_update(dt);
                
                {
                    let mut params_w = self.render_params.write().unwrap();
                    params_w.view = self.camera.view_matrix();
                    params_w.projection = self.camera.projection_matrix();
                    params_w.view_projection = params_w.projection * params_w.view;
                }
                
                {
                    let scene_render = self.scene_render.as_ref().unwrap();
                    scene_render.update_camera(&self.camera);
                }

                let render = self.render.as_mut().unwrap();
                let window = self.window.as_ref().unwrap(); 
                let tri = self.tri.as_ref().unwrap();
                
                
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
        
        let triangle_pass = triangle::TrianglePass::new(
            self.render.as_ref().unwrap(), 
            self.render_params.clone());

        triangle_pass.handle.toggle(); 
        
        self.tri = Some(triangle_pass);
        
        if !self.scene_path.is_empty() {
            let scene = Scene::import_gltf(self.render.as_ref().unwrap(), &self.scene_path);
            self.scene = Some(scene.clone());
            self.scene_render = Some(SceneRenderer::new(self.render.as_ref().unwrap(), scene));
        }
        self.app_start = Some(Instant::now());
    }
}

fn main() {

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);
    
    let mut app = App {
        camera : CameraController::new(na::point![0f32, 0f32, 1f32], f32::to_radians(60f32), 640.0/480.0, 1e-3, 100.0), 
        window : None,
        render : None,
        tri : None,
        app_start : None,
        prev_frame_start : None,
        mouse_look : false,
        pressed_directions : [false; 16],
        render_params : Arc::new(RwLock::new(RenderGlobalParams::default())),
        scene : None,
        scene_render : None,
        scene_path : String::from("assets/water_bottle/WaterBottle.gltf"),
    };
    event_loop.run_app(&mut app).unwrap();
}


