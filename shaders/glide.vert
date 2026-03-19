#version 450

// Vertex attributes from buffer
layout(location = 0) in vec2 inPosition;  // Screen-space position (x, y)
layout(location = 1) in vec4 inColor;     // RGBA color
layout(location = 2) in vec2 inTexCoord;  // Texture coordinates (s, t)
layout(location = 3) in float inOow;      // Reciprocal W (1/w) for depth

// Push constants for screen dimensions and rendering state
layout(push_constant) uniform PushConstants {
    vec2 screenSize;      // Screen width and height
    vec2 textureSize;     // Current texture width and height
    vec3 chromakeyColor;  // Chromakey color (RGB)
    float chromakeyEnabled; // 1.0 = enabled, 0.0 = disabled
    int colorCombineFunction; // CombineFunction enum
    int colorCombineOther;    // CombineOther enum
    int alphaCombineFunction; // CombineFunction enum
    int alphaCombineLocal;    // CombineLocal enum
    vec4 constantColor;       // Constant color (RGBA, normalized)
} pc;

// Outputs to fragment shader
layout(location = 0) out vec4 fragColor;
layout(location = 1) out vec2 fragTexCoord;

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
}
