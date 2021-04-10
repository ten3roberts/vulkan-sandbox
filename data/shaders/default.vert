#version 460
#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec3 normal;
layout(location = 2) in vec2 texCoord;

layout(location = 0) out vec4 fragColor;
layout(location = 1) out vec2 fragTexCoord;

/* layout(binding = 0, set = 1) uniform UniformBufferObject { */
/*   mat4 mvp; */
/* } ubo; */

struct ObjectData {
  mat4 mvp;
};

layout(std140,set = 1, binding = 0) readonly buffer ObjectBuffer{ 
  ObjectData objects[];
} objectBuffer;

void main() {
  gl_Position = objectBuffer.objects[gl_BaseInstance].mvp * vec4(inPosition, 1.0);
  fragColor = vec4(0.0, 0.0, 0.0, 1.0);
  fragTexCoord = texCoord;
}
