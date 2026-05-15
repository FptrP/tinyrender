#version 460 core

layout (location = 0) in vec3 pos;
layout (location = 1) in vec3 norm;
layout (location = 2) in vec2 uv;

layout (location = 0) out vec2 out_uv;
layout (location = 1) out vec3 out_norm;

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
  gl_Position = gConsts.viewProjection * instConsts.model * vec4(pos, 1.0);
  
  out_norm = normalize(vec3(transpose(instConsts.inverse_model) * vec4(norm, 0.0)));
  out_uv = uv;
}
