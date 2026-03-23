#version 450

// Vertex attributes from buffer
layout(location = 0) in vec2 inPosition;  // Screen-space position (x, y)
layout(location = 1) in vec4 inColor;     // RGBA color
layout(location = 2) in vec2 inTexCoord;  // Texture coordinates (s, t)
layout(location = 3) in float inOow;      // Reciprocal W (1/w) for depth

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
    vec4 constantColor;
    int colorCombineFactor;
    int alphaCombineFactor;
    int fogMode;
    float fogColorR;
    float fogColorG;
    float fogColorB;
} pc;

// Outputs to fragment shader
layout(location = 0) out vec4 fragColor;
layout(location = 1) out vec2 fragTexCoord;
layout(location = 2) out float fragOow;

void main() {
    // Convert screen coordinates to normalized device coordinates
    // Glide uses screen space (0,0) to (width, height)
    // Vulkan NDC is (-1,-1) to (1,1) with Y pointing down
    vec2 ndc;
    ndc.x = (inPosition.x / pc.screenSize.x) * 2.0 - 1.0;
    ndc.y = (inPosition.y / pc.screenSize.y) * 2.0 - 1.0;

    // Use oow for depth: larger oow = closer (smaller depth in Vulkan)
    // Vulkan depth range is 0.0 (near) to 1.0 (far)
    // oow is 1/w, so larger oow means closer to camera
    // We invert it: depth = 1.0 - oow to map closer objects to smaller depth values
    float depth = 1.0 - inOow;

    gl_Position = vec4(ndc, depth, 1.0);
    fragColor = inColor;

    // Pass texture coordinates as-is - normalize in fragment shader
    fragTexCoord = inTexCoord;

    // Pass oow to fragment shader for fog calculation
    fragOow = inOow;
}
