#version 450

// Vertex attributes from buffer
layout(location = 0) in vec2 inPosition;       // Screen-space position (x, y)
layout(location = 1) in vec4 inColor;          // RGBA color (iterated vertex color)
layout(location = 2) in vec2 inTexCoord;       // Texture coordinates (s, t)
layout(location = 3) in float inOow;           // Reciprocal W (1/w) for depth
layout(location = 4) in vec4 inConstantColor;  // Constant color from grConstantColorValue

// Push constants for screen dimensions and rendering state
layout(push_constant) uniform PushConstants {
    vec2 screenSize;
    vec2 textureSize;
    vec3 chromakeyColor;
    float chromakeyEnabled;
    int colorCombineFunction;
    int colorCombineOther;
    int alphaCombineFunction;
    int alphaCombineLocal;
    int colorCombineFactor;
    int alphaCombineFactor;
    int colorCombineLocal;
    int alphaCombineOther;
    int fogMode;
    float fogColorR;
    float fogColorG;
    float fogColorB;
} pc;

// Outputs to fragment shader
layout(location = 0) out vec4 fragColor;
layout(location = 1) out vec2 fragTexCoord;
layout(location = 2) out float fragOow;
layout(location = 3) out vec4 fragConstantColor;

void main() {
    vec2 ndc;
    ndc.x = (inPosition.x / pc.screenSize.x) * 2.0 - 1.0;
    ndc.y = (inPosition.y / pc.screenSize.y) * 2.0 - 1.0;

    float depth = 1.0 - inOow;

    gl_Position = vec4(ndc, depth, 1.0);
    fragColor = inColor;
    fragTexCoord = inTexCoord;
    fragOow = inOow;
    fragConstantColor = inConstantColor;
}
