#version 450

layout(location = 0) in vec2 inPosition;
layout(location = 1) in vec4 inColor;
layout(location = 2) in vec2 inTexCoord;
layout(location = 3) in float inOow;
layout(location = 4) in vec4 inConstantColor;
layout(location = 5) in uint inCombineFlags;
layout(location = 6) in uint inTexIndex;
layout(location = 7) in float inDepth;
layout(location = 8) in float inTexQ;
layout(location = 9) in float inFog;
layout(location = 10) in vec2 inTexCoord1;
layout(location = 11) in float inTexQ1;
layout(location = 12) in uint inTexIndex1;

layout(push_constant) uniform PushConstants {
    vec2 screenSize;
    float chromakeyR;
    float chromakeyG;
    float chromakeyB;
    int fogMode;
    float fogColorR;
    float fogColorG;
    float fogColorB;
    float gammaR;
    float gammaG;
    float gammaB;
    uint fogTable[16];
    uint texCombine[2];
    uint alphaTest;
} pc;

layout(location = 0) out vec4 fragColor;
layout(location = 1) out vec2 fragTexCoord;
layout(location = 2) out float fragOow;
layout(location = 3) out vec4 fragConstantColor;
layout(location = 4) flat out uint fragCombineFlags;
layout(location = 5) flat out uint fragTexIndex;
layout(location = 6) out float fragTexQ;
layout(location = 7) out float fragFog;
layout(location = 8) out float fragDepth;
layout(location = 9) out vec2 fragTexCoord1;
layout(location = 10) out float fragTexQ1;
layout(location = 11) flat out uint fragTexIndex1;

void main() {
    vec2 ndc;
    ndc.x = (inPosition.x / pc.screenSize.x) * 2.0 - 1.0;
    ndc.y = (inPosition.y / pc.screenSize.y) * 2.0 - 1.0;
    gl_Position = vec4(ndc, inDepth, 1.0);
    fragColor = inColor;
    fragTexCoord = inTexCoord;
    fragOow = inOow;
    fragConstantColor = inConstantColor;
    fragCombineFlags = inCombineFlags;
    fragTexIndex = inTexIndex;
    fragTexQ = inTexQ;
    fragFog = inFog;
    fragDepth = inDepth;
    fragTexCoord1 = inTexCoord1;
    fragTexQ1 = inTexQ1;
    fragTexIndex1 = inTexIndex1;
}
