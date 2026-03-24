#version 450

layout(location = 0) in vec4 fragColor;
layout(location = 1) in vec2 fragTexCoord;
layout(location = 2) in float fragOow;
layout(location = 3) in vec4 fragConstantColor;

layout(binding = 0) uniform sampler2D texSampler;

// Push constants (constant color moved to vertex attribute)
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

layout(location = 0) out vec4 outColor;

// CombineFunction enum values
const int CF_ZERO = 0;
const int CF_LOCAL = 1;
const int CF_LOCAL_ALPHA = 2;
const int CF_SCALE_OTHER = 3;
const int CF_SCALE_OTHER_ADD_LOCAL = 4;
const int CF_SCALE_OTHER_ADD_LOCAL_ALPHA = 5;
const int CF_SCALE_OTHER_MINUS_LOCAL = 6;
const int CF_SCALE_OTHER_MINUS_LOCAL_ADD_LOCAL = 7;
const int CF_SCALE_OTHER_MINUS_LOCAL_ADD_LOCAL_ALPHA = 8;
const int CF_SCALE_MINUS_LOCAL_ADD_LOCAL = 9;
const int CF_SCALE_MINUS_LOCAL_ADD_LOCAL_ALPHA = 16;

const int CO_ITERATED = 0;
const int CO_TEXTURE = 1;
const int CO_CONSTANT = 2;

const int CL_ITERATED = 0;
const int CL_CONSTANT = 1;

const int CFR_ZERO = 0;
const int CFR_LOCAL = 1;
const int CFR_OTHER_ALPHA = 2;
const int CFR_LOCAL_ALPHA = 3;
const int CFR_TEXTURE_ALPHA = 4;
const int CFR_TEXTURE_RGB = 5;
const int CFR_ONE = 8;
const int CFR_ONE_MINUS_LOCAL = 9;
const int CFR_ONE_MINUS_OTHER_ALPHA = 10;
const int CFR_ONE_MINUS_LOCAL_ALPHA = 11;
const int CFR_ONE_MINUS_TEXTURE_ALPHA = 12;

vec3 computeColorFactor(int factor, vec3 localRGB, float localA, float otherA, vec4 texColor) {
    if (factor == CFR_ZERO) return vec3(0.0);
    if (factor == CFR_LOCAL) return localRGB;
    if (factor == CFR_OTHER_ALPHA) return vec3(otherA);
    if (factor == CFR_LOCAL_ALPHA) return vec3(localA);
    if (factor == CFR_TEXTURE_ALPHA) return vec3(texColor.a);
    if (factor == CFR_TEXTURE_RGB) return texColor.rgb;
    if (factor == CFR_ONE) return vec3(1.0);
    if (factor == CFR_ONE_MINUS_LOCAL) return vec3(1.0) - localRGB;
    if (factor == CFR_ONE_MINUS_OTHER_ALPHA) return vec3(1.0 - otherA);
    if (factor == CFR_ONE_MINUS_LOCAL_ALPHA) return vec3(1.0 - localA);
    if (factor == CFR_ONE_MINUS_TEXTURE_ALPHA) return vec3(1.0 - texColor.a);
    return localRGB;
}

float computeAlphaFactor(int factor, float localA, float otherA, vec4 texColor) {
    if (factor == CFR_ZERO) return 0.0;
    if (factor == CFR_LOCAL) return localA;
    if (factor == CFR_OTHER_ALPHA) return otherA;
    if (factor == CFR_LOCAL_ALPHA) return localA;
    if (factor == CFR_TEXTURE_ALPHA) return texColor.a;
    if (factor == CFR_ONE) return 1.0;
    if (factor == CFR_ONE_MINUS_LOCAL) return 1.0 - localA;
    if (factor == CFR_ONE_MINUS_OTHER_ALPHA) return 1.0 - otherA;
    if (factor == CFR_ONE_MINUS_LOCAL_ALPHA) return 1.0 - localA;
    if (factor == CFR_ONE_MINUS_TEXTURE_ALPHA) return 1.0 - texColor.a;
    return localA;
}

void main() {
    vec2 uv = fragTexCoord / pc.textureSize;
    vec4 texColor = texture(texSampler, uv);

    // Chromakey
    if (pc.chromakeyEnabled > 0.5) {
        float threshold = 0.01;
        if (texColor.r < threshold && texColor.g < threshold && texColor.b < threshold) {
            discard;
        }
    }

    // --- Color Combine (RGB) ---
    vec3 c_other;
    if (pc.colorCombineOther == CO_TEXTURE) {
        c_other = texColor.rgb;
    } else if (pc.colorCombineOther == CO_CONSTANT) {
        c_other = fragConstantColor.rgb;
    } else {
        c_other = fragColor.rgb;
    }

    vec3 c_local;
    if (pc.colorCombineLocal == CL_CONSTANT) {
        c_local = fragConstantColor.rgb;
    } else {
        c_local = fragColor.rgb;
    }

    // --- Alpha Combine ---
    float a_local;
    if (pc.alphaCombineLocal == CL_CONSTANT) {
        a_local = fragConstantColor.a;
    } else {
        a_local = fragColor.a;
    }

    float a_other;
    if (pc.alphaCombineOther == CO_TEXTURE) {
        a_other = texColor.a;
    } else if (pc.alphaCombineOther == CO_CONSTANT) {
        a_other = fragConstantColor.a;
    } else {
        a_other = fragColor.a;
    }

    vec3 c_factor = computeColorFactor(pc.colorCombineFactor, c_local, a_local, a_local, texColor);

    vec3 resultRGB;
    if (pc.colorCombineFunction == CF_ZERO) {
        resultRGB = vec3(0.0);
    } else if (pc.colorCombineFunction == CF_LOCAL) {
        resultRGB = c_local;
    } else if (pc.colorCombineFunction == CF_LOCAL_ALPHA) {
        resultRGB = vec3(a_local);
    } else if (pc.colorCombineFunction == CF_SCALE_OTHER) {
        resultRGB = c_factor * c_other;
    } else if (pc.colorCombineFunction == CF_SCALE_OTHER_ADD_LOCAL) {
        resultRGB = c_factor * c_other + c_local;
    } else if (pc.colorCombineFunction == CF_SCALE_OTHER_ADD_LOCAL_ALPHA) {
        resultRGB = c_factor * c_other + vec3(a_local);
    } else if (pc.colorCombineFunction == CF_SCALE_OTHER_MINUS_LOCAL) {
        resultRGB = c_factor * (c_other - c_local);
    } else if (pc.colorCombineFunction == CF_SCALE_OTHER_MINUS_LOCAL_ADD_LOCAL) {
        resultRGB = c_factor * (c_other - c_local) + c_local;
    } else if (pc.colorCombineFunction == CF_SCALE_OTHER_MINUS_LOCAL_ADD_LOCAL_ALPHA) {
        resultRGB = c_factor * (c_other - c_local) + vec3(a_local);
    } else if (pc.colorCombineFunction == CF_SCALE_MINUS_LOCAL_ADD_LOCAL) {
        resultRGB = (vec3(1.0) - c_factor) * c_local;
    } else if (pc.colorCombineFunction == CF_SCALE_MINUS_LOCAL_ADD_LOCAL_ALPHA) {
        resultRGB = vec3(a_local) - c_factor * c_local;
    } else {
        resultRGB = texColor.rgb * fragColor.rgb;
    }

    float a_factor = computeAlphaFactor(pc.alphaCombineFactor, a_local, a_other, texColor);

    float resultA;
    if (pc.alphaCombineFunction == CF_ZERO) {
        resultA = fragColor.a;
    } else if (pc.alphaCombineFunction == CF_LOCAL) {
        resultA = a_local;
    } else if (pc.alphaCombineFunction == CF_LOCAL_ALPHA) {
        resultA = a_local;
    } else if (pc.alphaCombineFunction == CF_SCALE_OTHER) {
        resultA = a_factor * a_other;
    } else if (pc.alphaCombineFunction == CF_SCALE_OTHER_ADD_LOCAL) {
        resultA = a_factor * a_other + a_local;
    } else if (pc.alphaCombineFunction == CF_SCALE_OTHER_ADD_LOCAL_ALPHA) {
        resultA = a_factor * a_other + a_local;
    } else if (pc.alphaCombineFunction == CF_SCALE_OTHER_MINUS_LOCAL) {
        resultA = a_factor * (a_other - a_local);
    } else if (pc.alphaCombineFunction == CF_SCALE_OTHER_MINUS_LOCAL_ADD_LOCAL) {
        resultA = a_factor * (a_other - a_local) + a_local;
    } else if (pc.alphaCombineFunction == CF_SCALE_OTHER_MINUS_LOCAL_ADD_LOCAL_ALPHA) {
        resultA = a_factor * (a_other - a_local) + a_local;
    } else if (pc.alphaCombineFunction == CF_SCALE_MINUS_LOCAL_ADD_LOCAL) {
        resultA = (1.0 - a_factor) * a_local;
    } else if (pc.alphaCombineFunction == CF_SCALE_MINUS_LOCAL_ADD_LOCAL_ALPHA) {
        resultA = a_local - a_factor * a_local;
    } else {
        resultA = texColor.a * fragColor.a;
    }

    resultRGB = clamp(resultRGB, 0.0, 1.0);
    resultA = clamp(resultA, 0.0, 1.0);

    // --- Fog ---
    if (pc.fogMode > 0) {
        float w = (fragOow > 0.0001) ? 1.0 / fragOow : 65536.0;
        float fogIndex = clamp(log2(max(w, 1.0)) * 4.0, 0.0, 63.0);
        float fogFactor = fogIndex / 63.0;
        vec3 fogCol = vec3(pc.fogColorR, pc.fogColorG, pc.fogColorB);
        resultRGB = mix(resultRGB, fogCol, fogFactor);
    }

    outColor = vec4(resultRGB, resultA);
}
