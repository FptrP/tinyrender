#version 460 core

void main() {
  const vec3[] tri = vec3[3](
    vec3(0, -0.5, 0.5),
    vec3(-0.5, 0.5, 0.5),
    vec3(0.5, 0.5, 0.5)
  );

  gl_Position = vec4(tri[gl_VertexIndex], 1.0);
}
