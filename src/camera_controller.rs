use core::f32;

use na::{Matrix4, Point2, Point3, Vector2, Vector3};

pub struct CameraController {
    // x - right, y - up, z - backwards 
    pub yaw : f32,
    pub pitch : f32,
    pub prev_mouse_pos : na::Point2<f32>, 
    pub sensitivity : f32,

    pub global_pos : na::Point3<f32>,

    pub x : Vector3<f32>,
    pub y : Vector3<f32>,
    pub z : Vector3<f32>,

    pub fovy : f32,
    pub aspect : f32, 
    pub znear : f32,
    pub zfar : f32,
}

impl CameraController {

    pub fn new(global_pos : Point3<f32>, fovy : f32, aspect : f32, znear : f32, zfar : f32) -> Self {
        let mut controller = Self {
            yaw : 0f32,
            pitch : 0f32,
            prev_mouse_pos : na::point![0f32, 0f32],
            sensitivity : 0.01f32,
            global_pos,
            fovy,
            aspect,
            znear,
            zfar,
            x : na::zero(),
            y : na::vector![0f32, 1f32, 0f32],
            z : na::zero(),
        };
        
        controller.recalc_vectors();
        controller
    }

    fn recalc_vectors(&mut self) {
        // forward direction = -z 
        let dir_x = f32::cos(self.pitch) * f32::cos(self.yaw);
        let dir_z = -f32::cos(self.pitch) * f32::sin(self.yaw);
        let dir_y = f32::sin(self.pitch);
        
        let up = na::vector![0f32, 1f32, 0f32];

        self.z = -na::vector![dir_x, dir_y, dir_z].normalize();
        self.x = up.cross(&self.z).normalize();
        self.y = self.z.cross(&self.x).normalize();
        
        //println!("yaw {}, pitch {}", f32::to_degrees(self.yaw), f32::to_degrees(self.pitch));
        //println!("x {}, y {}, z {}", self.x, self.y, self.z);
    }
    
    pub fn update_mouse_pos(&mut self, new_pos : Point2<f32>) {
        //print!("{}", new_pos);

        let dx = new_pos.x - self.prev_mouse_pos.x;
        let dy = new_pos.y - self.prev_mouse_pos.y;

        self.yaw -= self.sensitivity * dx;
        
        if self.yaw < 0f32 {
            self.yaw += f32::consts::PI * 2.0;
        }
        if self.yaw > f32::consts::PI * 2.0 {
            self.yaw -= f32::consts::PI * 2.0;
        }
        
        self.pitch = (self.pitch - self.sensitivity * dy).clamp(-f32::consts::PI * 0.5 + 0.1, f32::consts::PI * 0.5 - 0.1);
        self.prev_mouse_pos = new_pos;

        self.recalc_vectors();
    }
    
    pub fn update_position_rel(&mut self, movement : &Vector3<f32>) {
        self.global_pos += movement.x * self.x + movement.y * self.y + movement.z * self.z;
    }

    pub fn view_matrix(&self) -> Matrix4<f32> {
        let x = self.x;
        let y = self.y;
        let z = self.z;
        let origin = self.global_pos.coords;
        
        Matrix4::new(
            x.x, x.y, x.z, -origin.dot(&x),
            y.x, y.y, y.z, -origin.dot(&y),
            z.x, z.y, z.z, -origin.dot(&z), 
            0f32, 0f32, 0f32, 1f32
        )
    }

    pub fn projection_matrix(&self) -> Matrix4<f32> {
        projection(self.fovy, self.aspect, self.znear, self.zfar)
    }
}


pub fn look_at_rh(eye : &Point3<f32>, center : &Point3<f32>, up : &Vector3<f32>) -> Matrix4<f32>
{
  let z = (eye - center).normalize(); 
  let x : Vector3<f32> = up.cross(&z).normalize();
  let y = z.cross(&x).normalize();

  let origin : Vector3<f32> = Vector3::new(eye.x, eye.y, eye.z);
  
  Matrix4::new(
    x.x, x.y, x.z, -origin.dot(&x),
    y.x, y.y, y.z, -origin.dot(&y),
    z.x, z.y, z.z, -origin.dot(&z), 
    0f32, 0f32, 0f32, 1f32
  )
}

pub fn projection(fovy_rad : f32, aspect : f32, znear : f32, zfar : f32) -> Matrix4<f32>
{
  assert!(znear > 0f32 && zfar > znear);
  // TODO: check xc and yc signs in inference. also m43 sign might be wrong
  let xc = 1f32/(aspect * f32::tan(fovy_rad/2.0));
  let yc = -1f32/f32::tan(fovy_rad/2.0);
  let za = -zfar/(zfar - znear);
  let zb = -znear * zfar/(zfar - znear);

  Matrix4::new(
    xc, 0f32, 0f32, 0f32,
    0f32, yc, 0f32, 0f32,
    0f32, 0f32, za, zb,
    0f32, 0f32, -1f32, 0f32 
  )
}

// transforms point on viewport with coords in range [-1, 1] x [-1, 1] to camera space 
pub fn viewport_to_camera(p : Point2<f32>, fovy_rads : f32, aspect : f32, znear : f32) -> Vector3<f32>
{
  // tg(fovy/2) = (y_plane/2)/znear
  // x_plane = y_plane * width/height
  let yp = f32::tan(fovy_rads * 0.5f32) * znear;
  let xp = yp * aspect;

  Vector3::new(p.x * xp, -p.y * yp, -znear)
}
