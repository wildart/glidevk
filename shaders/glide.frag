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
    int colorCombineFunction;
    int colorCombineOther;
    int alphaCombineFunction;
    int alphaCombineLocal;
    vec4 constantColor;
} pc;

layout(location = 0) out vec4 outColor;

// CombineFunction enum values (matching Rust repr(i32))
const int CF_ZERO = 0;
const int CF_LOCAL = 1;
const int CF_LOCAL_ALPHA = 2;
const int CF_SCALE_OTHER = 3;
const int CF_SCALE_OTHER_ADD_LOCAL = 4;
const int CF_SCALE_OTHER_ADD_LOCAL_ALPHA = 5;
const int CF_SCALE_OTHER_MINUS_LOCAL = 6;
const int CF_SCALE_OTHER_MINUS_LOCAL_ADD_LOCAL = 7;
const int CF_SCALE_OTHER_MINUS_LOCAL_ADD_LOCAL_ALPHA = 8;

// CombineOther enum values
const int CO_ITERATED = 0;
const int CO_TEXTURE = 1;
const int CO_CONSTANT = 2;

// CombineLocal enum values
const int CL_ITERATED = 0;
const int CL_CONSTANT = 1;

void main() {
    // Normalize texture coordinates by base texture size
    vec2 uv = fragTexCoord / pc.textureSize;

    // Sample the texture
    vec4 texColor = texture(texSampler, uv);

    // Chromakey: discard pixels matching the chromakey color
    if (pc.chromakeyEnabled > 0.5) {
        float threshold = 0.01;
        if (texColor.r < threshold && texColor.g < threshold && texColor.b < threshold) {
            discard;
        }
    }

    // --- Color Combine (RGB) ---
    // Determine "other" color source
    vec3 c_other;
    if (pc.colorCombineOther == CO_TEXTURE) {
        c_other = texColor.rgb;
    } else if (pc.colorCombineOther == CO_CONSTANT) {
        c_other = pc.constantColor.rgb;
    } else {
        c_other = fragColor.rgb; // CO_ITERATED
    }

    // "local" color is always iterated vertex color for color combine
    vec3 c_local = fragColor.rgb;

    // "factor" for scaling is the local color
    vec3 c_factor = c_local;

    // Evaluate color combine function
    vec3 resultRGB;
    if (pc.colorCombineFunction == CF_ZERO) {
        resultRGB = vec3(0.0);
    } else if (pc.colorCombineFunction == CF_LOCAL) {
        resultRGB = c_local;
    } else if (pc.colorCombineFunction == CF_LOCAL_ALPHA) {
        resultRGB = vec3(fragColor.a);
    } else if (pc.colorCombineFunction == CF_SCALE_OTHER) {
        resultRGB = c_factor * c_other;
    } else if (pc.colorCombineFunction == CF_SCALE_OTHER_ADD_LOCAL) {
        resultRGB = c_factor * c_other + c_local;
    } else if (pc.colorCombineFunction == CF_SCALE_OTHER_ADD_LOCAL_ALPHA) {
        resultRGB = c_factor * c_other + vec3(fragColor.a);
    } else if (pc.colorCombineFunction == CF_SCALE_OTHER_MINUS_LOCAL) {
        resultRGB = c_factor * (c_other - c_local);
    } else if (pc.colorCombineFunction == CF_SCALE_OTHER_MINUS_LOCAL_ADD_LOCAL) {
        resultRGB = c_factor * (c_other - c_local) + c_local;
    } else if (pc.colorCombineFunction == CF_SCALE_OTHER_MINUS_LOCAL_ADD_LOCAL_ALPHA) {
        resultRGB = c_factor * (c_other - c_local) + vec3(fragColor.a);
    } else {
        // Default fallback: texture * iterated
        resultRGB = texColor.rgb * fragColor.rgb;
    }

    // --- Alpha Combine ---
    // Determine "local" alpha source
    float a_local;
    if (pc.alphaCombineLocal == CL_CONSTANT) {
        a_local = pc.constantColor.a;
    } else {
        a_local = fragColor.a; // CL_ITERATED
    }

    // Evaluate alpha combine function (simplified for common cases)
    float resultA;
    if (pc.alphaCombineFunction == CF_ZERO) {
        resultA = 1.0; // Zero alpha means opaque (no alpha blending)
    } else if (pc.alphaCombineFunction == CF_LOCAL) {
        resultA = a_local;
    } else if (pc.alphaCombineFunction == CF_LOCAL_ALPHA) {
        resultA = a_local;
    } else if (pc.alphaCombineFunction == CF_SCALE_OTHER) {
        resultA = a_local * texColor.a;
    } else if (pc.alphaCombineFunction == CF_SCALE_OTHER_ADD_LOCAL) {
        resultA = a_local * texColor.a + a_local;
    } else {
        // Default fallback
        resultA = texColor.a * fragColor.a;
    }

    outColor = vec4(clamp(resultRGB, 0.0, 1.0), clamp(resultA, 0.0, 1.0));
}
