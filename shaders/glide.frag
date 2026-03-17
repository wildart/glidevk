#version 450

layout(location = 0) in vec4 fragColor;
layout(location = 1) in vec2 fragTexCoord;

layout(binding = 0) uniform sampler2D texSampler;

// Push constants (must match vertex shader layout)
layout(push_constant) uniform PushConstants {
    vec2 screenSize;
    vec2 textureSize;
    vec3 chromakeyColor;
    float chromakeyEnabled;
} pc;

layout(location = 0) out vec4 outColor;

void main() {
    // Normalize texture coordinates by base texture size
    // Glide coords are in base texture space, scaled by aspect ratio
    // pc.textureSize contains the calculated base dimensions (e.g., 256x128 for 2:1 aspect)
    vec2 uv = fragTexCoord / pc.textureSize;

    // Sample the texture
    vec4 texColor = texture(texSampler, uv);

    // Chromakey: discard black pixels (RGB all near zero)
    // Use tight threshold for exact color match
    if (pc.chromakeyEnabled > 0.5) {
        float threshold = 0.01;
        if (texColor.r < threshold && texColor.g < threshold && texColor.b < threshold) {
            discard;
        }
    }

    // Multiply by vertex color (Glide color modulation)
    outColor = texColor * fragColor;
}
