#version 460 core

layout(location = 0) out vec4 out_color;

layout (push_constant) uniform PushConsts {
  vec4 tri_color;
};

void main() {
  out_color = tri_color;
}
