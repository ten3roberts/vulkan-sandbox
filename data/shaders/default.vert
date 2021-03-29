#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec3 normal;
layout(location = 2) in vec2 texCoord;

layout(location = 0) out vec4 fragColor;
layout(location = 1) out vec2 fragTexCoord;

layout(binding = 0) uniform UniformBufferObject {
  mat4 mvp;
} ubo;

void main() {
  gl_Position = ubo.mvp * vec4(inPosition, 1.0);
  fragColor = vec4(0.0, 0.0, 0.0, 1.0);
  fragTexCoord = texCoord;
}
