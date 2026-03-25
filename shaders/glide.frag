#version 450
#extension GL_EXT_nonuniform_qualifier : require

layout(location = 0) in vec4 fragColor;
layout(location = 1) in vec2 fragTexCoord;
layout(location = 2) in float fragOow;
layout(location = 3) in vec4 fragConstantColor;
layout(location = 4) flat in uint fragCombineFlags;
layout(location = 5) flat in uint fragTexIndex;

layout(binding = 0) uniform sampler2D textures[2048];

layout(push_constant) uniform PushConstants {
    vec2 screenSize;
    float chromakeyEnabled;
    float chromakeyR;
    float chromakeyG;
    float chromakeyB;
    int fogMode;
    float fogColorR;
    float fogColorG;
    float fogColorB;
} pc;

layout(location = 0) out vec4 outColor;

// Unpack combine flags from per-vertex uint
int getCombineFunc()   { return int(fragCombineFlags & 0x1Fu); }
int getAlphaFunc()     { return int((fragCombineFlags >> 5) & 0x1Fu); }
int getColorFactor()   { return int((fragCombineFlags >> 10) & 0xFu); }
int getAlphaFactor()   { return int((fragCombineFlags >> 14) & 0xFu); }
int getColorOther()    { return int((fragCombineFlags >> 18) & 0x3u); }
int getAlphaLocal()    { return int((fragCombineFlags >> 20) & 0x3u); }
int getColorLocal()    { return int((fragCombineFlags >> 22) & 0x3u); }
int getAlphaOther()    { return int((fragCombineFlags >> 24) & 0x3u); }

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
    // Compute UV normalization from actual texture dimensions (replaces push constant texture_size)
    ivec2 ts = textureSize(textures[nonuniformEXT(fragTexIndex)], 0);
    float base = float(max(ts.x, ts.y));
    vec2 baseSize = 256.0 * vec2(ts) / base;
    vec2 uv = fragTexCoord / baseSize;

    vec4 texColor = texture(textures[nonuniformEXT(fragTexIndex)], uv);

    if (pc.chromakeyEnabled > 0.5) {
        if (texColor.r < 0.01 && texColor.g < 0.01 && texColor.b < 0.01) discard;
    }

    // Unpack per-vertex combine state
    int colorFunc = getCombineFunc();
    int alphaFunc = getAlphaFunc();
    int colorFactorType = getColorFactor();
    int alphaFactorType = getAlphaFactor();
    int colorOther = getColorOther();
    int alphaLocalSrc = getAlphaLocal();
    int colorLocalSrc = getColorLocal();
    int alphaOtherSrc = getAlphaOther();

    // Resolve sources
    vec3 c_other = (colorOther == CO_TEXTURE) ? texColor.rgb
                 : (colorOther == CO_CONSTANT) ? fragConstantColor.rgb
                 : fragColor.rgb;

    vec3 c_local = (colorLocalSrc == CL_CONSTANT) ? fragConstantColor.rgb : fragColor.rgb;

    float a_local = (alphaLocalSrc == CL_CONSTANT) ? fragConstantColor.a : fragColor.a;

    float a_other = (alphaOtherSrc == CO_TEXTURE) ? texColor.a
                  : (alphaOtherSrc == CO_CONSTANT) ? fragConstantColor.a
                  : fragColor.a;

    vec3 c_factor = computeColorFactor(colorFactorType, c_local, a_local, a_local, texColor);

    // Color combine
    vec3 resultRGB;
    if      (colorFunc == CF_ZERO) resultRGB = vec3(0.0);
    else if (colorFunc == CF_LOCAL) resultRGB = c_local;
    else if (colorFunc == CF_LOCAL_ALPHA) resultRGB = vec3(a_local);
    else if (colorFunc == CF_SCALE_OTHER) resultRGB = c_factor * c_other;
    else if (colorFunc == CF_SCALE_OTHER_ADD_LOCAL) resultRGB = c_factor * c_other + c_local;
    else if (colorFunc == CF_SCALE_OTHER_ADD_LOCAL_ALPHA) resultRGB = c_factor * c_other + vec3(a_local);
    else if (colorFunc == CF_SCALE_OTHER_MINUS_LOCAL) resultRGB = c_factor * (c_other - c_local);
    else if (colorFunc == CF_SCALE_OTHER_MINUS_LOCAL_ADD_LOCAL) resultRGB = c_factor * (c_other - c_local) + c_local;
    else if (colorFunc == CF_SCALE_OTHER_MINUS_LOCAL_ADD_LOCAL_ALPHA) resultRGB = c_factor * (c_other - c_local) + vec3(a_local);
    else if (colorFunc == CF_SCALE_MINUS_LOCAL_ADD_LOCAL) resultRGB = (vec3(1.0) - c_factor) * c_local;
    else if (colorFunc == CF_SCALE_MINUS_LOCAL_ADD_LOCAL_ALPHA) resultRGB = vec3(a_local) - c_factor * c_local;
    else resultRGB = texColor.rgb * fragColor.rgb;

    // Alpha combine
    float a_factor = computeAlphaFactor(alphaFactorType, a_local, a_other, texColor);

    float resultA;
    if      (alphaFunc == CF_ZERO) resultA = fragColor.a;
    else if (alphaFunc == CF_LOCAL) resultA = a_local;
    else if (alphaFunc == CF_LOCAL_ALPHA) resultA = a_local;
    else if (alphaFunc == CF_SCALE_OTHER) resultA = a_factor * a_other;
    else if (alphaFunc == CF_SCALE_OTHER_ADD_LOCAL) resultA = a_factor * a_other + a_local;
    else if (alphaFunc == CF_SCALE_OTHER_ADD_LOCAL_ALPHA) resultA = a_factor * a_other + a_local;
    else if (alphaFunc == CF_SCALE_OTHER_MINUS_LOCAL) resultA = a_factor * (a_other - a_local);
    else if (alphaFunc == CF_SCALE_OTHER_MINUS_LOCAL_ADD_LOCAL) resultA = a_factor * (a_other - a_local) + a_local;
    else if (alphaFunc == CF_SCALE_OTHER_MINUS_LOCAL_ADD_LOCAL_ALPHA) resultA = a_factor * (a_other - a_local) + a_local;
    else if (alphaFunc == CF_SCALE_MINUS_LOCAL_ADD_LOCAL) resultA = (1.0 - a_factor) * a_local;
    else if (alphaFunc == CF_SCALE_MINUS_LOCAL_ADD_LOCAL_ALPHA) resultA = a_local - a_factor * a_local;
    else resultA = texColor.a * fragColor.a;

    resultRGB = clamp(resultRGB, 0.0, 1.0);
    resultA = clamp(resultA, 0.0, 1.0);

    if (pc.fogMode > 0) {
        float w = (fragOow > 0.0001) ? 1.0 / fragOow : 65536.0;
        float fogFactor = clamp(log2(max(w, 1.0)) * 4.0 / 63.0, 0.0, 1.0);
        resultRGB = mix(resultRGB, vec3(pc.fogColorR, pc.fogColorG, pc.fogColorB), fogFactor);
    }

    outColor = vec4(resultRGB, resultA);
}
