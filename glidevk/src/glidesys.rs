use std::fmt;
use std::os::raw::c_void;
use std::ptr::null_mut;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct FxBool(pub i32);
#[allow(non_upper_case_globals)]
impl FxBool {
    pub const False: Self = Self(0);
    pub const True: Self = Self(1);
    pub fn is_true(self) -> bool {
        self.0 != 0
    }
}

impl Default for FxBool {
    fn default() -> FxBool {
        FxBool::False
    }
}

impl fmt::Debug for FxBool {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        if *self != FxBool::False {
            write!(fmt, "FXTRUE")
        } else {
            write!(fmt, "FXFALSE")
        }
    }
}

impl fmt::Display for FxBool {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        if *self != FxBool::False {
            write!(fmt, "true")
        } else {
            write!(fmt, "false")
        }
    }
}

impl From<bool> for FxBool {
    fn from(b: bool) -> FxBool {
        if b {
            Self::True
        } else {
            Self::False
        }
    }
}

pub type FxU8 = u8;
pub type FxU16 = u16;
pub type FxU32 = u32;
pub type FxI32 = i32;

#[derive(Clone, Copy, Default)]
#[repr(transparent)]
pub struct Color(pub u32);

impl fmt::Debug for Color {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "#{:08x}", self.0)
    }
}

impl Color {
    pub fn to_rgba(&self, format: ColorFormat) -> (u8, u8, u8, u8) {
        let c = self.0;
        let (r, g, b, a) = match format {
            ColorFormat::Argb => (
                (c >> 16) & 0xff,
                (c >> 8) & 0xff,
                (c >> 0) & 0xff,
                (c >> 24) & 0xff,
            ),
            ColorFormat::Abgr => (
                (c >> 0) & 0xff,
                (c >> 8) & 0xff,
                (c >> 16) & 0xff,
                (c >> 24) & 0xff,
            ),
            ColorFormat::Rgba => (
                (c >> 24) & 0xff,
                (c >> 16) & 0xff,
                (c >> 8) & 0xff,
                (c >> 0) & 0xff,
            ),
            ColorFormat::Bgra => (
                (c >> 8) & 0xff,
                (c >> 16) & 0xff,
                (c >> 24) & 0xff,
                (c >> 0) & 0xff,
            ),
        };
        (r as u8, g as u8, b as u8, a as u8)
    }

    pub fn to_rgba_f(&self, format: ColorFormat) -> (f32, f32, f32, f32) {
        let (r, g, b, a) = self.to_rgba(format);
        (
            r as f32 / 255.,
            g as f32 / 255.,
            b as f32 / 255.,
            a as f32 / 255.,
        )
    }
}

#[derive(Clone, Copy, Default)]
#[repr(transparent)]
pub struct Alpha(pub u8);

impl fmt::Debug for Alpha {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "#{:02x}", self.0)
    }
}

impl Alpha {
    pub fn as_u8(&self) -> u8 {
        self.0
    }

    pub fn to_f32(&self) -> f32 {
        (self.0 as f32) / 255.
    }
}

#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct TmuVertex {
    pub sow: f32,
    pub tow: f32,
    pub oow: f32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Vertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub ooz: f32,
    pub a: f32,
    pub oow: f32,
    pub tmuvtx: [TmuVertex; crate::GLIDE_NUM_TMU],
}

impl fmt::Debug for Vertex {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let pos = (self.x, self.y);
        let color = (self.r, self.g, self.b, self.a);
        let st = (self.tmuvtx[0].sow, self.tmuvtx[0].tow);
        fmt.debug_struct("Vertex")
            .field("pos", &pos)
            .field("color", &color)
            .field("tex", &st)
            .finish()
    }
}

impl Vertex {
    /// Create a new vertex with the given screen position
    pub fn new(x: f32, y: f32) -> Self {
        Vertex {
            x,
            y,
            z: 0.0,
            r: 255.0,
            g: 255.0,
            b: 255.0,
            ooz: 0.0,
            a: 255.0,
            oow: 1.0,
            tmuvtx: [TmuVertex::default(); crate::GLIDE_NUM_TMU],
        }
    }

    pub fn s_and_t(&self) -> (f32, f32) {
        let tmuvtx = &self.tmuvtx[0];
        (tmuvtx.sow, tmuvtx.tow)
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(i32)]
pub enum LOD {
    // Glide LOD values: GR_LOD_LOG2_xxx where value = log2(size)
    L1 = 0,
    L2 = 1,
    L4 = 2,
    L8 = 3,
    L16 = 4,
    L32 = 5,
    L64 = 6,
    L128 = 7,
    L256 = 8,
}

impl LOD {
    pub fn to_size(self) -> usize {
        match self {
            LOD::L256 => 256,
            LOD::L128 => 128,
            LOD::L64 => 64,
            LOD::L32 => 32,
            LOD::L16 => 16,
            LOD::L8 => 8,
            LOD::L4 => 4,
            LOD::L2 => 2,
            LOD::L1 => 1,
        }
    }

    pub fn to_texture_size(self, aspect: AspectRatio) -> (usize, usize) {
        let size = self.to_size();
        // Use max(1, ...) to prevent 0-dimension textures from integer division
        match aspect {
            AspectRatio::A8x1 => (size, (size / 8).max(1)),
            AspectRatio::A4x1 => (size, (size / 4).max(1)),
            AspectRatio::A2x1 => (size, (size / 2).max(1)),
            AspectRatio::A1x1 => (size, size),
            AspectRatio::A1x2 => ((size / 2).max(1), size),
            AspectRatio::A1x4 => ((size / 4).max(1), size),
            AspectRatio::A1x8 => ((size / 8).max(1), size),
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(i32)]
pub enum AspectRatio {
    A8x1 = 3,  // 8W x 1H (GR_ASPECT_LOG2_8x1)
    A4x1 = 2,  // 4W x 1H (GR_ASPECT_LOG2_4x1)
    A2x1 = 1,  // 2W x 1H (GR_ASPECT_LOG2_2x1)
    A1x1 = 0,  // 1W x 1H (GR_ASPECT_LOG2_1x1)
    A1x2 = -1, // 1W x 2H (GR_ASPECT_LOG2_1x2)
    A1x4 = -2, // 1W x 4H (GR_ASPECT_LOG2_1x4)
    A1x8 = -3, // 1W x 8H (GR_ASPECT_LOG2_1x8)
}

#[derive(Clone, Copy, Debug)]
#[repr(i32)]
pub enum TextureFormat {
    Rgb332 = 0,
    Yiq422 = 1,
    Alpha8 = 2,
    Intensity8 = 3,
    AlphaIntensity44 = 4,
    P8 = 5,
    Rsvd0 = 6,
    Rsvd1 = 7,
    Argb8332 = 8,
    Ayiq8422 = 9,
    Rgb565 = 10,
    Argb1555 = 11,
    Argb4444 = 12,
    AlphaIntensity88 = 13,
    Ap88 = 14,
    Rsvd2 = 15,
}

impl TextureFormat {
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            TextureFormat::Rgb332
            | TextureFormat::Yiq422
            | TextureFormat::Alpha8
            | TextureFormat::Intensity8
            | TextureFormat::AlphaIntensity44
            | TextureFormat::P8 => 1,
            TextureFormat::Argb8332
            | TextureFormat::Ayiq8422
            | TextureFormat::Rgb565
            | TextureFormat::Argb1555
            | TextureFormat::Argb4444
            | TextureFormat::AlphaIntensity88
            | TextureFormat::Ap88 => 2,
            TextureFormat::Rsvd0 | TextureFormat::Rsvd1 | TextureFormat::Rsvd2 => {
                panic!("Reserved texture format: {:?}", self)
            }
        }
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct TexInfo {
    smallLod: i32,
    largeLod: i32,
    aspectRatio: i32,
    format: i32,
    data: *mut c_void,
}

impl TexInfo {
    pub fn empty(
        smallLod: LOD,
        largeLod: LOD,
        aspectRatio: AspectRatio,
        format: TextureFormat,
    ) -> TexInfo {
        TexInfo {
            smallLod: smallLod as i32,
            largeLod: largeLod as i32,
            aspectRatio: aspectRatio as i32,
            format: format as i32,
            data: null_mut(),
        }
    }

    /// Debug method to dump raw field values without interpreting as enums
    pub fn debug_raw_values(&self) -> String {
        unsafe {
            let ptr = self as *const TexInfo as *const i32;
            let small_lod_raw = *ptr;
            let large_lod_raw = *ptr.add(1);
            let aspect_raw = *ptr.add(2);
            let format_raw = *ptr.add(3);
            let data_ptr = *(ptr.add(4) as *const *mut c_void);
            format!(
                "smallLod={}, largeLod={}, aspect={}, format={}, data={:p}",
                small_lod_raw, large_lod_raw, aspect_raw, format_raw, data_ptr
            )
        }
    }

    pub fn get_dimensions(&self) -> (usize, usize) {
        self.levels()
            .first()
            .map(|(_, w, h)| (*w, *h))
            .unwrap_or((0, 0))
    }

    pub fn levels(&self) -> Vec<(LOD, usize, usize)> {
        if !self.is_valid() {
            return Vec::new();
        }
        let lods = [
            LOD::L1,
            LOD::L2,
            LOD::L4,
            LOD::L8,
            LOD::L16,
            LOD::L32,
            LOD::L64,
            LOD::L128,
            LOD::L256,
        ];
        (self.smallLod as usize..=self.largeLod as usize)
            .rev()
            .map(|i| {
                let (w, h) = lods[i].to_texture_size(self.aspect_ratio());
                (lods[i], w, h)
            })
            .collect()
    }

    pub fn get_size(&self) -> usize {
        self.levels()
            .iter()
            .map(|(_, w, h)| w * h * self.format().bytes_per_pixel())
            .sum()
    }

    pub fn get_size_mask(&self, even_odd: u32) -> usize {
        let size: usize = self
            .levels()
            .iter()
            .filter(|(lod, _, _)| even_odd & (1 << (*lod as u32 & 1)) != 0)
            .map(|(_, w, h)| w * h * self.format().bytes_per_pixel())
            .sum();
        (size + 7) & !7
    }

    pub fn is_valid(&self) -> bool {
        (0..=8).contains(&self.smallLod)
            && (self.smallLod..=8).contains(&self.largeLod)
            && (-3..=3).contains(&self.aspectRatio)
            && (0..=15).contains(&self.format)
    }
    pub fn aspect_ratio(&self) -> AspectRatio {
        match self.aspectRatio {
            -3 => AspectRatio::A1x8,
            -2 => AspectRatio::A1x4,
            -1 => AspectRatio::A1x2,
            1 => AspectRatio::A2x1,
            2 => AspectRatio::A4x1,
            3 => AspectRatio::A8x1,
            _ => AspectRatio::A1x1,
        }
    }

    pub fn format(&self) -> TextureFormat {
        const FORMATS: [TextureFormat; 16] = [
            TextureFormat::Rgb332,
            TextureFormat::Yiq422,
            TextureFormat::Alpha8,
            TextureFormat::Intensity8,
            TextureFormat::AlphaIntensity44,
            TextureFormat::P8,
            TextureFormat::Rsvd0,
            TextureFormat::Rsvd1,
            TextureFormat::Argb8332,
            TextureFormat::Ayiq8422,
            TextureFormat::Rgb565,
            TextureFormat::Argb1555,
            TextureFormat::Argb4444,
            TextureFormat::AlphaIntensity88,
            TextureFormat::Ap88,
            TextureFormat::Rsvd2,
        ];
        FORMATS
            .get(self.format as usize)
            .copied()
            .unwrap_or(TextureFormat::Rsvd0)
    }

    /// # Safety
    /// `self.data` must point to a valid buffer of `self.get_size()` bytes.
    pub unsafe fn get_data(&self) -> &[u8] {
        let size = self.get_size();
        if self.data.is_null() {
            return &[];
        }
        unsafe { std::slice::from_raw_parts(self.data as *const u8, size) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(i32)]
pub enum Buffer {
    Front = 0,
    Back = 1,
    Aux = 2,
    Depth = 3,
    Alpha = 4,
    Triple = 5,
}

/// Lock type for LFB access
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct LfbLock(pub u32);
#[allow(non_upper_case_globals)]
impl LfbLock {
    pub const ReadOnly: Self = Self(0);
    pub const WriteOnly: Self = Self(1);
    pub const NoIdle: Self = Self(0x10);
}
impl std::ops::BitOr for LfbLock {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// LFB write mode (pixel format)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct LfbWriteMode(pub i32);
#[allow(non_upper_case_globals)]
impl LfbWriteMode {
    pub const Rgb565: Self = Self(0x0);
    pub const Rgb555: Self = Self(0x1);
    pub const Argb1555: Self = Self(0x2);
    pub const Reserved1: Self = Self(0x3);
    pub const Rgb888: Self = Self(0x4);
    pub const Argb8888: Self = Self(0x5);
    pub const Reserved2: Self = Self(0x6);
    pub const Reserved3: Self = Self(0x7);
    pub const Reserved4: Self = Self(0x8);
    pub const Reserved5: Self = Self(0x9);
    pub const Reserved6: Self = Self(0xa);
    pub const Reserved7: Self = Self(0xb);
    pub const Rgb565Depth: Self = Self(0xc);
    pub const Rgb555Depth: Self = Self(0xd);
    pub const Argb1555Depth: Self = Self(0xe);
    pub const Za16: Self = Self(0xf);
    pub const Any: Self = Self(0xff);
    pub const Rle16: Self = Self(0x80);
}

/// LFB info structure returned by grLfbLock
#[repr(C)]
pub struct LfbInfo {
    pub size: i32,
    pub lfb_ptr: *mut std::ffi::c_void,
    pub stride_in_bytes: u32,
    pub write_mode: LfbWriteMode,
    pub origin: OriginLocation,
}

impl Default for LfbInfo {
    fn default() -> Self {
        LfbInfo {
            size: std::mem::size_of::<LfbInfo>() as i32,
            lfb_ptr: std::ptr::null_mut(),
            stride_in_bytes: 0,
            write_mode: LfbWriteMode::Rgb565,
            origin: OriginLocation::UpperLeft,
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(i32)]
pub enum MipMapMode {
    Disable = 0,
    Nearest = 1,
    NearestDither = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct AlphaBlendFnc(pub i32);
#[allow(non_upper_case_globals)]
impl AlphaBlendFnc {
    pub const Zero: Self = Self(0);
    pub const SrcAlpha: Self = Self(1);
    pub const SrcColor: Self = Self(2);
    pub const DstAlpha: Self = Self(3);
    pub const One: Self = Self(4);
    pub const OneMinusSrcAlpha: Self = Self(5);
    pub const OneMinusSrcColor: Self = Self(6);
    pub const OneMinusDstAlpha: Self = Self(7);
    pub const AlphaSaturate: Self = Self(15);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct ChipID(pub i32);
#[allow(non_upper_case_globals)]
impl ChipID {
    pub const Tmu0: Self = Self(0);
    pub const Tmu1: Self = Self(1);
    pub const Tmu2: Self = Self(2);
    pub const Fbi: Self = Self(0);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct CombineFunction(pub i32);
#[allow(non_upper_case_globals)]
impl CombineFunction {
    pub const Zero: Self = Self(0);
    pub const Local: Self = Self(1);
    pub const LocalAlpha: Self = Self(2);
    pub const ScaleOther: Self = Self(3);
    pub const ScaleOtherAddLocal: Self = Self(4);
    pub const ScaleOtherAddLocalAlpha: Self = Self(5);
    pub const ScaleOtherMinusLocal: Self = Self(6);
    pub const ScaleOtherMinusLocalAddLocal: Self = Self(7);
    pub const ScaleOtherMinusLocalAddLocalAlpha: Self = Self(8);
    pub const ScaleMinusLocalAddLocal: Self = Self(9);
    pub const ScaleMinusLocalAddLocalAlpha: Self = Self(16);
}

#[derive(Clone, Copy, Debug)]
#[repr(i32)]
pub enum CombineFactor {
    Zero = 0,
    Local = 1,
    OtherAlpha = 2,
    LocalAlpha = 3,
    TextureAlpha = 4,
    TextureRgb = 5,
    // LodFraction = 5
    One = 8,
    OneMinusLocal = 9,
    OneMinusOtherAlpha = 10,
    OneMinusLocalAlpha = 11,
    OneMinusTextureAlpha = 12,
    OneMinusLodFraction = 13,
}

#[derive(Clone, Copy, Debug)]
#[repr(i32)]
pub enum CombineLocal {
    Iterated = 0,
    Constant = 1,
    Depth = 2,
}

#[derive(Clone, Copy, Debug)]
#[repr(i32)]
pub enum CombineOther {
    Iterated = 0,
    Texture = 1,
    Constant = 2,
}

#[derive(Clone, Copy, Debug)]
#[repr(u32)]
pub enum CoordinateSpaceMode {
    Window = 0,
    Clip = 1,
}

#[derive(Clone, Copy, Debug)]
#[repr(i32)]
pub enum DitherMode {
    Disable = 0,
    Dither2x2 = 1,
    Dither4x4 = 2,
}

/// Primitive types for grDrawVertexArray
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct PrimitiveType(pub u32);
#[allow(non_upper_case_globals)]
impl PrimitiveType {
    pub const Points: Self = Self(0);
    pub const LineStrip: Self = Self(1);
    pub const Lines: Self = Self(2);
    pub const Polygon: Self = Self(3);
    pub const TriangleStrip: Self = Self(4);
    pub const TriangleFan: Self = Self(5);
    pub const Triangles: Self = Self(6);
    pub const TriangleStripContinue: Self = Self(7);
    pub const TriangleFanContinue: Self = Self(8);
}

/// Texture table types for grTexDownloadTable
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum TextureTableType {
    /// NCC table 0 (narrow channel compression)
    Ncc0 = 0,
    /// NCC table 1 (narrow channel compression)
    Ncc1 = 1,
    /// 256-color palette (ARGB 8888)
    Palette = 2,
    /// 256-color palette (ARGB 6666 extension)
    Palette6666Ext = 3,
}

#[derive(Clone, Copy, Debug)]
#[repr(i32)]
pub enum TextureFilterMode {
    PointSampled = 0,
    Bilinear = 1,
}

#[derive(Clone, Copy, Debug)]
#[repr(i32)]
pub enum CmpFnc {
    Never = 0,
    Less = 1,
    Equal = 2,
    LEqual = 3,
    Greater = 4,
    NotEqual = 5,
    GEqual = 6,
    Always = 7,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(i32)]
pub enum ScreenRefresh {
    R60Hz = 0,
    R70Hz = 1,
    R72Hz = 2,
    R75Hz = 3,
    R80Hz = 4,
    R90Hz = 5,
    R100Hz = 6,
    R85Hz = 7,
    R120Hz = 8,
    None = 0xff,
}

impl From<ScreenRefresh> for usize {
    fn from(refresh: ScreenRefresh) -> usize {
        use self::ScreenRefresh::*;
        match refresh {
            R60Hz => 60,
            R70Hz => 70,
            R72Hz => 72,
            R75Hz => 75,
            R80Hz => 80,
            R90Hz => 90,
            R100Hz => 100,
            R85Hz => 85,
            R120Hz => 120,
            None => unimplemented!("Refresh rate must be set currently"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(i32)]
pub enum ScreenResolution {
    R320x200 = 0,
    R320x240 = 1,
    R400x256 = 2,
    R512x384 = 3,
    R640x200 = 4,
    R640x350 = 5,
    R640x400 = 6,
    R640x480 = 7,
    R800x600 = 8,
    R960x720 = 9,
    R856x480 = 10,
    R512x256 = 11,
    R1024x768 = 12,
    R1280x1024 = 13,
    R1600x1200 = 14,
    R400x300 = 15,
    None = 0xff,
}

impl From<ScreenResolution> for (usize, usize) {
    fn from(res: ScreenResolution) -> (usize, usize) {
        use self::ScreenResolution::*;
        match res {
            R320x200 => (320, 200),
            R320x240 => (320, 240),
            R400x256 => (400, 256),
            R512x384 => (512, 384),
            R640x200 => (640, 200),
            R640x350 => (640, 350),
            R640x400 => (640, 400),
            R640x480 => (640, 480),
            R800x600 => (800, 600),
            R960x720 => (960, 720),
            R856x480 => (856, 480),
            R512x256 => (512, 256),
            R1024x768 => (1024, 768),
            R1280x1024 => (1280, 1024),
            R1600x1200 => (1600, 1200),
            R400x300 => (400, 300),
            None => unimplemented!("Using the window’s size requires winsys integration."),
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(i32)]
pub enum ColorFormat {
    Argb = 0,
    Abgr = 1,
    Rgba = 2,
    Bgra = 3,
}

impl Default for ColorFormat {
    fn default() -> ColorFormat {
        ColorFormat::Argb
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(i32)]
pub enum CullMode {
    Disable = 0,
    Negative = 1,
    Positive = 2,
}

impl Default for CullMode {
    fn default() -> CullMode {
        CullMode::Disable
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct OriginLocation(pub i32);
#[allow(non_upper_case_globals)]
impl OriginLocation {
    pub const UpperLeft: Self = Self(0);
    pub const LowerLeft: Self = Self(1);
    pub const Any: Self = Self(0xff);
}

#[derive(Clone, Copy, Debug)]
#[repr(i32)]
pub enum DepthBufferMode {
    Disable = 0,
    ZBuffer = 1,
    WBuffer = 2,
    ZBufferCompareToBias = 3,
    WBufferCompareToBias = 4,
}

// Compatibility enums for glide.dll

#[derive(Clone, Copy, Debug)]
#[repr(i32)]
pub enum LfbBypassMode {
    Disable = 0,
    Enable = 1,
}

#[derive(Clone, Copy, Debug)]
#[repr(i32)]
pub enum ChromakeyMode {
    Disable = 0,
    Enable = 1,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(i32)]
pub enum PassthruMode {
    Vga = 0,
    Sst1 = 1,
}

#[derive(Clone, Copy, Debug)]
#[repr(i32)]
pub enum AlphaSource {
    CcAlpha = 0,
    IteratedAlpha = 1,
    TextureAlpha = 2,
    TextureAlphaTimesIteratedAlpha = 3,
}

#[derive(Clone, Copy, Debug)]
#[repr(i32)]
pub enum ColorCombineFnc {
    Zero = 0,
    Ccrgb = 1,
    Itrgb = 2,
    ItrgbDelta0 = 3,
    DecalTexture = 4,
    TextureTimesCcrgb = 5,
    TextureTimesItrgb = 6,
    TextureTimesItrgbDelta0 = 7,
    TextureTimesItrgbAddAlpha = 8,
    TextureTimesAlpha = 9,
    TextureTimesAlphaAddItrgb = 10,
    TextureAddItrgb = 11,
    TextureSubItrgb = 12,
    CcrgbBlendItrgbOnTexalpha = 13,
    DiffSpecA = 14,
    DiffSpecB = 15,
    One = 16,
}

#[derive(Clone, Copy, Debug)]
#[repr(i32)]
pub enum TextureCombineFnc {
    Zero = 0,
    Decal = 1,
    Other = 2,
    Add = 3,
    Multiply = 4,
    Subtract = 5,
    Detail = 6,
    DetailOther = 7,
    TrilinearOdd = 8,
    TrilinearEven = 9,
    One = 10,
}

/*
** grGet/grReset types
*/
#[derive(Clone, Copy, Debug)]
#[repr(i32)]
pub enum EnvironmentParameters {
    BitsDepth = 0x01,
    BitsRgba = 0x02,
    FifoFullness = 0x03,
    FogTableEntries = 0x04,
    GammaTableEntries = 0x05,
    GlideStateSize = 0x06,
    GlideVertexlayoutSize = 0x07,
    IsBusy = 0x08,
    LfbPixelPipe = 0x09,
    MaxTextureSize = 0x0a,
    MaxTextureAspectRatio = 0x0b,
    MemoryFb = 0x0c,
    MemoryTmu = 0x0d,
    MemoryUma = 0x0e,
    NumBoards = 0x0f,
    NonPowerOfTwoTextures = 0x10,
    NumFb = 0x11,
    NumSwapHistoryBuffer = 0x12,
    NumTmu = 0x13,
}
// #define GR_PENDING_BUFFERSWAPS          0x14
// #define GR_REVISION_FB                  0x15
// #define GR_REVISION_TMU                 0x16
// #define GR_STATS_LINES                  0x17  /* grGet/grReset */
// #define GR_STATS_PIXELS_AFUNC_FAIL      0x18
// #define GR_STATS_PIXELS_CHROMA_FAIL     0x19
// #define GR_STATS_PIXELS_DEPTHFUNC_FAIL  0x1a
// #define GR_STATS_PIXELS_IN              0x1b
// #define GR_STATS_PIXELS_OUT             0x1c
// #define GR_STATS_PIXELS                 0x1d  /* grReset */
// #define GR_STATS_POINTS                 0x1e  /* grGet/grReset */
// #define GR_STATS_TRIANGLES_IN           0x1f
// #define GR_STATS_TRIANGLES_OUT          0x20
// #define GR_STATS_TRIANGLES              0x21  /* grReset */
// #define GR_SWAP_HISTORY                 0x22
// #define GR_SUPPORTS_PASSTHRU            0x23
// #define GR_TEXTURE_ALIGN                0x24
// #define GR_VIDEO_POSITION               0x25
// #define GR_VIEWPORT                     0x26
// #define GR_WDEPTH_MIN_MAX               0x27
// #define GR_ZDEPTH_MIN_MAX               0x28
// #define GR_VERTEX_PARAMETER             0x29
// #define GR_BITS_GAMMA                   0x2a

/// grGet parameter types
#[derive(Clone, Copy, Debug)]
#[repr(u32)]
pub enum GrGetParam {
    BitsDepth = 0x01,
    BitsRgba = 0x02,
    FifoFullness = 0x03,
    FogTableEntries = 0x04,
    GammaTableEntries = 0x05,
    GlideStateSize = 0x06,
    GlideVertexLayoutSize = 0x07,
    IsBusy = 0x08,
    LfbPixelPipe = 0x09,
    MaxTextureSize = 0x0a,
    MaxTextureAspectRatio = 0x0b,
    MemoryFb = 0x0c,
    MemoryTmu = 0x0d,
    MemoryUma = 0x0e,
    NumBoards = 0x0f,
    NonPowerOfTwoTextures = 0x10,
    NumFb = 0x11,
    NumSwapHistoryBuffer = 0x12,
    NumTmu = 0x13,
    PendingBufferswaps = 0x14,
    RevisionFb = 0x15,
    RevisionTmu = 0x16,
    StatsLines = 0x17,
    StatsPixelsAfuncFail = 0x18,
    StatsPixelsChromaFail = 0x19,
    StatsPixelsDepthfuncFail = 0x1a,
    StatsPixelsIn = 0x1b,
    StatsPixelsOut = 0x1c,
    StatsPixels = 0x1d,
    StatsPoints = 0x1e,
    StatsTrianglesIn = 0x1f,
    StatsTrianglesOut = 0x20,
    StatsTriangles = 0x21,
    SwapHistory = 0x22,
    SupportsPassthru = 0x23,
    TextureAlign = 0x24,
    VideoPosition = 0x25,
    Viewport = 0x26,
    WdepthMinMax = 0x27,
    ZdepthMinMax = 0x28,
    VertexParameter = 0x29,
    BitsGamma = 0x2a,
}

/*
** grGetString types
*/
#[derive(Clone, Copy, Debug)]
#[repr(i32)]
pub enum EnvironmentInfo {
    Extension = 0xa0,
    Hardware = 0xa1,
    Renderer = 0xa2,
    Vendor = 0xa3,
    Version = 0xa4,
}

/*
** Fog modes
*/
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct FogMode(pub i32);
#[allow(non_upper_case_globals)]
impl FogMode {
    pub const Disable: Self = Self(0);
    pub const WithTableOnFogCoordExt: Self = Self(1);
    pub const WithTable: Self = Self(2);
    pub const WithTableOnQ: Self = Self(2);
    pub const WithTableOnW: Self = Self(2);
    pub const WithIteratedZ: Self = Self(3);
    pub const WithIteratedAlphaExt: Self = Self(4);
    pub const Mult2: Self = Self(0x100);
    pub const Add2: Self = Self(0x200);
}

impl std::ops::BitOr for FogMode {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/*
** Texture clamp modes
*/
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(i32)]
pub enum TextureClampMode {
    Wrap = 0,
    Clamp = 1,
    Mirror = 2,
}

impl Default for TextureClampMode {
    fn default() -> TextureClampMode {
        TextureClampMode::Wrap
    }
}

/*
** grEnable/grDisable modes
*/
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(i32)]
pub enum EnableMode {
    /// Anti-aliased rendering (ordered)
    AaOrdered = 0x01,
    /// Allow mipmap dithering
    AllowMipmapDither = 0x02,
    /// Pass-through mode
    Passthru = 0x03,
    /// Shameless plug (easter egg)
    ShamelessPlug = 0x04,
    /// Video smoothing
    VideoSmoothing = 0x05,
}

/*
** grVertexLayout parameters
*/
/// Vertex layout parameter types for grVertexLayout
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum VertexLayoutParam {
    /// XY position (must be at offset 0)
    Xy = 0x01,
    /// Z coordinate
    Z = 0x02,
    /// W coordinate (perspective)
    W = 0x03,
    /// Q coordinate (1/W for texture mapping)
    Q = 0x04,
    /// Fog coordinate (extension)
    FogExt = 0x05,
    /// Alpha component (float)
    A = 0x10,
    /// RGB color components (floats)
    Rgb = 0x20,
    /// Packed ARGB color (u32)
    Pargb = 0x30,
    /// Texture coordinates for TMU0
    St0 = 0x40,
    /// Texture coordinates for TMU1
    St1 = 0x41,
    /// Texture coordinates for TMU2
    St2 = 0x42,
    /// Q for TMU0
    Q0 = 0x50,
    /// Q for TMU1
    Q1 = 0x51,
    /// Q for TMU2
    Q2 = 0x52,
}

/// Vertex layout mode (enable/disable)
#[derive(Clone, Copy, Debug, PartialEq, Default)]
#[repr(u32)]
pub enum VertexLayoutMode {
    #[default]
    Disable = 0x00,
    Enable = 0x01,
}

/// Single vertex component info
#[derive(Clone, Copy, Debug, Default)]
pub struct VertexComponentInfo {
    pub offset: i32,
    pub mode: VertexLayoutMode,
}

/// Color type for vertex colors
#[derive(Clone, Copy, Debug, PartialEq, Default)]
#[repr(u32)]
pub enum VertexColorType {
    /// Floating point RGB/A components (0.0-255.0)
    #[default]
    Float = 0,
    /// Packed unsigned byte ARGB
    U8 = 1,
}

/// Complete vertex layout state
#[derive(Clone, Debug, Default)]
pub struct VertexLayout {
    /// XY position info (must be at offset 0)
    pub xy: VertexComponentInfo,
    /// Z coordinate info
    pub z: VertexComponentInfo,
    /// W coordinate info
    pub w: VertexComponentInfo,
    /// Q coordinate info
    pub q: VertexComponentInfo,
    /// Fog coordinate info
    pub fog: VertexComponentInfo,
    /// Alpha component info
    pub a: VertexComponentInfo,
    /// RGB color info
    pub rgb: VertexComponentInfo,
    /// Packed ARGB info
    pub pargb: VertexComponentInfo,
    /// ST0 texture coordinates
    pub st0: VertexComponentInfo,
    /// ST1 texture coordinates
    pub st1: VertexComponentInfo,
    /// Q0 for TMU0
    pub q0: VertexComponentInfo,
    /// Q1 for TMU1
    pub q1: VertexComponentInfo,
    /// Color type (float or packed)
    pub color_type: VertexColorType,
}

impl VertexLayout {
    /// Read a float from raw bytes at the given offset
    fn read_f32(data: &[u8], offset: i32) -> f32 {
        if offset < 0 || (offset as usize + 4) > data.len() {
            return 0.0;
        }
        let bytes: [u8; 4] = data[offset as usize..offset as usize + 4]
            .try_into()
            .unwrap_or([0; 4]);
        f32::from_le_bytes(bytes)
    }

    /// Read a u32 from raw bytes at the given offset
    fn read_u32(data: &[u8], offset: i32) -> u32 {
        if offset < 0 || (offset as usize + 4) > data.len() {
            return 0;
        }
        let bytes: [u8; 4] = data[offset as usize..offset as usize + 4]
            .try_into()
            .unwrap_or([0; 4]);
        u32::from_le_bytes(bytes)
    }

    /// Extract vertex components from raw vertex data based on configured layout
    /// Returns (x, y, r, g, b, a, s, t, oow)
    pub fn extract_vertex(&self, data: &[u8]) -> (f32, f32, f32, f32, f32, f32, f32, f32, f32) {
        // Position (XY) - always required, at offset 0
        let x = Self::read_f32(data, 0);
        let y = Self::read_f32(data, 4);

        // Color - either float RGB + A or packed ARGB
        let (r, g, b, a) = if self.color_type == VertexColorType::U8
            && self.pargb.mode == VertexLayoutMode::Enable
        {
            // Packed ARGB (u32 with bytes: ARGB or BGRA depending on format)
            let packed = Self::read_u32(data, self.pargb.offset);
            // Standard ARGB: A in high byte, R, G, B in lower bytes
            let a = ((packed >> 24) & 0xFF) as f32;
            let r = ((packed >> 16) & 0xFF) as f32;
            let g = ((packed >> 8) & 0xFF) as f32;
            let b = (packed & 0xFF) as f32;
            (r, g, b, a)
        } else {
            // Float RGB and separate A
            let (r, g, b) = if self.rgb.mode == VertexLayoutMode::Enable {
                (
                    Self::read_f32(data, self.rgb.offset),
                    Self::read_f32(data, self.rgb.offset + 4),
                    Self::read_f32(data, self.rgb.offset + 8),
                )
            } else {
                (255.0, 255.0, 255.0)
            };
            let a = if self.a.mode == VertexLayoutMode::Enable {
                Self::read_f32(data, self.a.offset)
            } else {
                255.0
            };
            (r, g, b, a)
        };

        // Texture coordinates (ST0)
        let (s, t) = if self.st0.mode == VertexLayoutMode::Enable {
            (
                Self::read_f32(data, self.st0.offset),
                Self::read_f32(data, self.st0.offset + 4),
            )
        } else {
            (0.0, 0.0)
        };

        // Q0 (1/w for perspective correct texturing)
        let oow = if self.q0.mode == VertexLayoutMode::Enable {
            Self::read_f32(data, self.q0.offset)
        } else if self.q.mode == VertexLayoutMode::Enable {
            Self::read_f32(data, self.q.offset)
        } else if self.w.mode == VertexLayoutMode::Enable {
            let w = Self::read_f32(data, self.w.offset);
            if w != 0.0 {
                1.0 / w
            } else {
                1.0
            }
        } else {
            1.0
        };

        (x, y, r, g, b, a, s, t, oow)
    }

    pub fn decode(&self, data: &[u8]) -> (Vertex, f32) {
        let (x, y, r, g, b, a, s, t, tq) = self.extract_vertex(data);
        let read = |field: &VertexComponentInfo, default| {
            if field.mode == VertexLayoutMode::Enable {
                Self::read_f32(data, field.offset)
            } else {
                default
            }
        };
        let w = read(&self.w, 1.0);
        let q = read(&self.q, if w == 0.0 { 1.0 } else { 1.0 / w });
        let mut vertex = Vertex::new(x, y);
        vertex.r = r;
        vertex.g = g;
        vertex.b = b;
        vertex.a = a;
        vertex.ooz = read(&self.z, 0.0);
        vertex.oow = q;
        vertex.tmuvtx[0] = TmuVertex {
            sow: s,
            tow: t,
            oow: tq,
        };
        vertex.tmuvtx[1] = TmuVertex {
            sow: read(&self.st1, 0.0),
            tow: if self.st1.mode == VertexLayoutMode::Enable {
                Self::read_f32(data, self.st1.offset + 4)
            } else {
                0.0
            },
            oow: read(&self.q1, q),
        };
        (vertex, read(&self.fog, q))
    }

    /// Calculate the minimum vertex size needed based on enabled components
    pub fn vertex_size(&self) -> usize {
        let mut max_end = 8; // Minimum: XY (2 floats at offset 0)

        // Check each component and track the maximum end position
        let check = |info: &VertexComponentInfo, size: i32, max_end: &mut usize| {
            if info.mode == VertexLayoutMode::Enable && info.offset >= 0 {
                let end = info.offset as usize + size as usize;
                if end > *max_end {
                    *max_end = end;
                }
            }
        };

        check(&self.z, 4, &mut max_end);
        check(&self.w, 4, &mut max_end);
        check(&self.q, 4, &mut max_end);
        check(&self.fog, 4, &mut max_end);
        check(&self.a, 4, &mut max_end);
        check(&self.rgb, 12, &mut max_end); // 3 floats
        check(&self.pargb, 4, &mut max_end);
        check(&self.st0, 8, &mut max_end); // 2 floats
        check(&self.st1, 8, &mut max_end);
        check(&self.q0, 4, &mut max_end);
        check(&self.q1, 4, &mut max_end);

        max_end
    }
}

// TYPE DEFINITIONS
pub type GrMipMapId = FxU32;
pub type GrStipplePattern = FxU32;
pub type GrFog = FxU8;
pub type GrContext = FxU32;
pub type GrTmuMemRange = FxU32;

#[cfg(test)]
mod layout_regressions {
    use super::*;
    #[test]
    fn texinfo_and_noncanonical_abi_values() {
        let mut info = TexInfo::empty(LOD::L1, LOD::L4, AspectRatio::A1x1, TextureFormat::Rgb565);
        assert_eq!(info.get_size(), 42);
        assert_eq!(info.get_size_mask(1), 40);
        assert_eq!(info.get_size_mask(2), 8);
        info.format = 0x80;
        assert!(!info.is_valid());
        assert_eq!(info.get_size(), 0);
        info.smallLod = -1;
        assert!(info.levels().is_empty());
        assert!(FxBool(255).is_true());
        assert!(!FxBool(0).is_true());
        assert_eq!(ChipID::Fbi, ChipID::Tmu0);
    }

    #[test]
    fn short_layout_color_switch_and_separate_z() {
        let enabled = |offset| VertexComponentInfo {
            offset,
            mode: VertexLayoutMode::Enable,
        };
        let mut layout = VertexLayout {
            pargb: enabled(8),
            color_type: VertexColorType::U8,
            ..Default::default()
        };
        let bytes: Vec<u8> = [1.0f32.to_bits(), 2.0f32.to_bits(), 0xff123456]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        assert_eq!(layout.vertex_size(), 12);
        let (vertex, _) = layout.decode(&bytes);
        assert_eq!(
            (vertex.x, vertex.y, vertex.r, vertex.g, vertex.b),
            (1.0, 2.0, 18.0, 52.0, 86.0)
        );
        layout.rgb = enabled(12);
        layout.a = enabled(24);
        layout.z = enabled(28);
        layout.q = enabled(32);
        layout.color_type = VertexColorType::Float;
        let mut bytes = bytes;
        for value in [64.0f32, 128.0, 192.0, 99.0, 12345.0, 0.5] {
            bytes.extend(value.to_le_bytes());
        }
        let (vertex, _) = layout.decode(&bytes);
        assert_eq!(
            (vertex.r, vertex.g, vertex.b, vertex.a, vertex.ooz, vertex.oow),
            (64.0, 128.0, 192.0, 99.0, 12345.0, 0.5)
        );
        assert_eq!((FogMode::WithTable | FogMode::Mult2).0, 0x102);
        assert_eq!((LfbLock::WriteOnly | LfbLock::NoIdle).0, 0x11);
    }
}
