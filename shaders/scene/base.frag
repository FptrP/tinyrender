#version 460 core 


layout (location = 0) in vec2 in_uv;
layout (location = 1) in vec3 in_norm;

layout (location = 0) out vec4 out_color;

layout (set = 0, binding = 0) uniform FrameConsts {
  mat4 viewProjection;
  mat4 view;
  mat4 inverseView;
} gConsts;


layout (set = 0, binding = 1) uniform InstanceConsts {
  mat4 model;
  mat4 inverse_model;
} instConsts;

void main() {
  float k = max(dot(in_norm, normalize(vec3(1, -1, 1))), 0.1);  
  out_color = vec4(pow(k * vec3(in_uv, 1), vec3(1.0/2.2)), 1);
}
