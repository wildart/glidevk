#version 450
#extension GL_EXT_nonuniform_qualifier : require

layout(location = 0) in vec4 fragColor;
layout(location = 1) in vec2 fragTexCoord;
layout(location = 2) in float fragOow;
layout(location = 3) in vec4 fragConstantColor;
layout(location = 4) flat in uint fragCombineFlags;
layout(location = 5) flat in uint fragTexIndex;
layout(location = 6) in float fragTexQ;
layout(location = 7) in float fragFog;
layout(location = 8) in float fragDepth;
layout(location = 9) in vec2 fragTexCoord1;
layout(location = 10) in float fragTexQ1;
layout(location = 11) flat in uint fragTexIndex1;

layout(binding = 0) uniform texture2D textures[2048];
layout(binding = 1) uniform sampler samplers[72];

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
    uint filterMode;
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
bool getChromakeyEnabled() { return (fragCombineFlags & (1u << 26)) != 0u; }

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

vec4 combine(int fn, vec4 factor, vec4 local, vec4 other) {
    if (fn == CF_ZERO) return vec4(0.0);
    if (fn == CF_LOCAL) return local;
    if (fn == CF_LOCAL_ALPHA) return vec4(local.a);
    if (fn == CF_SCALE_OTHER) return factor * other;
    if (fn == CF_SCALE_OTHER_ADD_LOCAL) return factor * other + local;
    if (fn == CF_SCALE_OTHER_ADD_LOCAL_ALPHA) return factor * other + vec4(local.a);
    if (fn == CF_SCALE_OTHER_MINUS_LOCAL) return factor * (other - local);
    if (fn == CF_SCALE_OTHER_MINUS_LOCAL_ADD_LOCAL) return factor * (other - local) + local;
    if (fn == CF_SCALE_OTHER_MINUS_LOCAL_ADD_LOCAL_ALPHA) return factor * (other - local) + vec4(local.a);
    if (fn == CF_SCALE_MINUS_LOCAL_ADD_LOCAL) return (vec4(1.0) - factor) * local;
    if (fn == CF_SCALE_MINUS_LOCAL_ADD_LOCAL_ALPHA) return vec4(local.a) - factor * local;
    return vec4(0.0);
}

bool alphaPass(float alpha) {
    int fn = int(pc.alphaTest & 7u);
    float a = floor(alpha * 255.0 + 0.5);
    float ref = float((pc.alphaTest >> 8) & 255u);
    if (fn == 0) return false;
    if (fn == 1) return a < ref;
    if (fn == 2) return a == ref;
    if (fn == 3) return a <= ref;
    if (fn == 4) return a > ref;
    if (fn == 5) return a != ref;
    if (fn == 6) return a >= ref;
    return true;
}

float fogEntry(int i) { return float((pc.fogTable[i / 4] >> (8 * (i % 4))) & 255u) / 255.0; }
float fogW(int i) { return exp2(float(3 + i / 4)) / float(8 - i % 4); }
float tableFog(float q) {
    float w = q > 0.0 ? 1.0 / q : 65536.0;
    for (int i = 0; i < 63; ++i) {
        if (w < fogW(i + 1)) return mix(fogEntry(i), fogEntry(i + 1), clamp((w - fogW(i)) / (fogW(i + 1) - fogW(i)), 0.0, 1.0));
    }
    return fogEntry(63);
}

vec4 textureCombine(uint flags, vec4 local, vec4 other) {
    vec4 factor = vec4(computeColorFactor(int((flags >> 10) & 15u), local.rgb, local.a, other.a, local),
        computeAlphaFactor(int((flags >> 14) & 15u), local.a, other.a, local));
    vec4 value;
    value.rgb = clamp(combine(int(flags & 31u), factor, local, other).rgb, 0.0, 1.0);
    value.a = clamp(combine(int((flags >> 5) & 31u), vec4(factor.a), vec4(local.a), vec4(other.a)).a, 0.0, 1.0);
    if ((flags & (1u << 18)) != 0u) value.rgb = vec3(1.0) - value.rgb;
    if ((flags & (1u << 19)) != 0u) value.a = 1.0 - value.a;
    return value;
}
vec4 sampleTmu(uint index, vec2 st, float q, uint flags) {
    ivec2 size = textureSize(sampler2D(textures[nonuniformEXT(index)], samplers[(flags >> 20) & 127u]), 0);
    vec2 baseSize = 256.0 * vec2(size) / float(max(size.x, size.y));
    return texture(sampler2D(textures[nonuniformEXT(index)], samplers[(flags >> 20) & 127u]), st / (abs(q) > 1e-20 ? q : 1.0) / baseSize);
}

// Presented game image: samplers[31] = linear/linear, clamp-to-edge (see TextureCache sampler keys).
#define SCENE sampler2D(textures[nonuniformEXT(fragTexIndex)], samplers[31])
vec4 sceneTap(vec2 uv) { return textureLod(SCENE, uv, 0.0); }

// Catmull-Rom bicubic in 9 bilinear taps (http://vec3.ca/bicubic-filtering-in-fewer-taps/).
vec4 catmullRom(vec2 uv, vec2 size) {
    vec2 pos = uv * size;
    vec2 p1 = floor(pos - 0.5) + 0.5;
    vec2 f = pos - p1;
    vec2 w0 = f * (-0.5 + f * (1.0 - 0.5 * f));
    vec2 w1 = 1.0 + f * f * (-2.5 + 1.5 * f);
    vec2 w2 = f * (0.5 + f * (2.0 - 1.5 * f));
    vec2 w3 = f * f * (-0.5 + 0.5 * f);
    vec2 w12 = w1 + w2;
    vec2 p0 = (p1 - 1.0) / size;
    vec2 p3 = (p1 + 2.0) / size;
    vec2 p12 = (p1 + w2 / w12) / size;
    vec3 result = ((sceneTap(vec2(p0.x, p0.y)) * w0.x + sceneTap(vec2(p12.x, p0.y)) * w12.x + sceneTap(vec2(p3.x, p0.y)) * w3.x) * w0.y
        + (sceneTap(vec2(p0.x, p12.y)) * w0.x + sceneTap(vec2(p12.x, p12.y)) * w12.x + sceneTap(vec2(p3.x, p12.y)) * w3.x) * w12.y
        + (sceneTap(vec2(p0.x, p3.y)) * w0.x + sceneTap(vec2(p12.x, p3.y)) * w12.x + sceneTap(vec2(p3.x, p3.y)) * w3.x) * w3.y).rgb;
    return vec4(result, 1.0);
}

// Presentation upscale of the game image: 0 = sharp anti-aliased nearest,
// 1 = bilinear, 2 = Catmull-Rom bicubic. All build on linear taps; screenSize is the source image size.
vec4 presentSample(vec2 uv) {
    if (pc.filterMode == 1u) return texture(SCENE, uv);
    if (pc.filterMode == 2u) return catmullRom(uv, pc.screenSize);
    // Nearest texel, with the linear tap only within ~half a screen pixel of a texel edge.
    vec2 texel = uv * pc.screenSize;
    vec2 dxdy = abs(vec2(dFdx(texel.x), dFdy(texel.y)));
    vec2 delta = fract(texel) - 0.5;
    vec2 aa = clamp((0.5 - abs(delta)) * 2.0 / dxdy, 0.0, 1.0);
    return texture(SCENE, uv - delta * aa / pc.screenSize);
}

void main() {
    if ((fragCombineFlags & (1u << 31)) != 0u) {
        vec4 scene = presentSample(fragTexCoord);
        outColor = vec4(pow(clamp(scene.rgb, 0.0, 1.0), vec3(1.0) / max(vec3(pc.gammaR, pc.gammaG, pc.gammaB), vec3(0.001))), scene.a);
        // During display, alphaTest marks automatic sRGB target encoding.
        if (pc.alphaTest != 0u) {
            vec3 c = outColor.rgb;
            outColor.rgb = mix(pow((c + 0.055) / 1.055, vec3(2.4)), c / 12.92, lessThanEqual(c, vec3(0.04045)));
        }
        return;
    }
    vec4 texColor = sampleTmu(fragTexIndex, fragTexCoord, fragTexQ, pc.texCombine[0]);

    if (getChromakeyEnabled()) {
        if (all(lessThan(abs(texColor.rgb - vec3(pc.chromakeyR, pc.chromakeyG, pc.chromakeyB)), vec3(0.5 / 255.0)))) discard;
    }

    vec4 other = textureCombine(pc.texCombine[1], sampleTmu(fragTexIndex1, fragTexCoord1, fragTexQ1, pc.texCombine[1]), vec4(0.0));
    texColor = textureCombine(pc.texCombine[0], texColor, other);

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

    float a_local = (alphaLocalSrc == CL_CONSTANT) ? fragConstantColor.a : (alphaLocalSrc == 2) ? fragDepth : fragColor.a;

    float a_other = (alphaOtherSrc == CO_TEXTURE) ? texColor.a
                  : (alphaOtherSrc == CO_CONSTANT) ? fragConstantColor.a
                  : fragColor.a;

    vec3 c_factor = computeColorFactor(colorFactorType, c_local, a_local, a_other, texColor);

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
    // Pass iterated vertex alpha through CF_ZERO for legacy Glide UI compatibility.
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

    if ((fragCombineFlags & (1u << 27)) != 0u) resultRGB = vec3(1.0) - resultRGB;
    if ((fragCombineFlags & (1u << 28)) != 0u) resultA = 1.0 - resultA;
    if (!alphaPass(resultA)) discard;

    int fogMode = pc.fogMode & 255;
    if (fogMode > 0) {
        float f = fogMode == 1 ? tableFog(fragFog) : fogMode == 2 ? tableFog(fragOow)
            : fogMode == 3 ? fragFog : fragColor.a;
        f = clamp(f, 0.0, 1.0);
        vec3 fogColor = vec3(pc.fogColorR, pc.fogColorG, pc.fogColorB);
        // MULT2 omits the fog-color term; ADD2 omits attenuation of incoming RGB.
        resultRGB = resultRGB * (((pc.fogMode & 0x200) != 0) ? 1.0 : 1.0 - f)
            + (((pc.fogMode & 0x100) != 0) ? vec3(0.0) : fogColor * f);
    }
    outColor = vec4(clamp(resultRGB, 0.0, 1.0), resultA);
}
