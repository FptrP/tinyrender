#version 460 core

layout (location = 0) in vec3 pos;
layout (location = 1) in vec3 color;

layout (location = 0) out vec3 out_color;

void main() {
  const vec3[] tri = vec3[3](
    vec3(0, -0.5, 0.5),
    vec3(-0.5, 0.5, 0.5),
    vec3(0.5, 0.5, 0.5)
  );

  gl_Position = vec4(pos, 1.0);
  out_color = color;
}
