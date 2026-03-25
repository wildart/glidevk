#version 450

layout(location = 0) in vec2 inPosition;
layout(location = 1) in vec4 inColor;
layout(location = 2) in vec2 inTexCoord;
layout(location = 3) in float inOow;
layout(location = 4) in vec4 inConstantColor;
layout(location = 5) in uint inCombineFlags;
layout(location = 6) in uint inTexIndex;

layout(push_constant) uniform PushConstants {
    vec2 screenSize;
    float chromakeyR;
    float chromakeyG;
    float chromakeyB;
    int fogMode;
    float fogColorR;
    float fogColorG;
    float fogColorB;
} pc;

layout(location = 0) out vec4 fragColor;
layout(location = 1) out vec2 fragTexCoord;
layout(location = 2) out float fragOow;
layout(location = 3) out vec4 fragConstantColor;
layout(location = 4) flat out uint fragCombineFlags;
layout(location = 5) flat out uint fragTexIndex;

void main() {
    vec2 ndc;
    ndc.x = (inPosition.x / pc.screenSize.x) * 2.0 - 1.0;
    ndc.y = (inPosition.y / pc.screenSize.y) * 2.0 - 1.0;
    gl_Position = vec4(ndc, 1.0 - inOow, 1.0);
    fragColor = inColor;
    fragTexCoord = inTexCoord;
    fragOow = inOow;
    fragConstantColor = inConstantColor;
    fragCombineFlags = inCombineFlags;
    fragTexIndex = inTexIndex;
}
