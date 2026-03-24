use ash::vk;
use std::mem::offset_of;

/// GPU vertex format for Glide rendering
/// This matches the layout expected by glide.vert shader
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct GpuVertex {
    pub position: [f32; 2],  // Screen-space x, y
    pub color: [f32; 4],     // RGBA color (normalized 0.0-1.0)
    pub tex_coord: [f32; 2], // Texture S, T coordinates
    pub oow: f32,            // Reciprocal W (1/w) for depth, 0.0-1.0 range
}

impl GpuVertex {
    pub fn new(x: f32, y: f32, r: f32, g: f32, b: f32, a: f32, s: f32, t: f32) -> Self {
        GpuVertex {
            position: [x, y],
            color: [r, g, b, a],
            tex_coord: [s, t],
            oow: 1.0, // Default depth (near)
        }
    }

    /// Create vertex from Glide vertex using per-vertex colors
    pub fn from_glide_vertex(v: &crate::glidesys::Vertex) -> Self {
        // Get perspective-divided texture coords (sow = s/w, tow = t/w)
        // and convert to actual texel space if needed
        let tmu = &v.tmuvtx[0];
        let (s, t) = if tmu.oow > 0.0 && tmu.oow != 1.0 {
            // Perspective-divided coords: s = sow / oow, t = tow / oow
            // (since oow = 1/w, dividing by oow gives us s*w and t*w = s and t)
            (tmu.sow / tmu.oow, tmu.tow / tmu.oow)
        } else {
            // oow is 0 (undefined) or 1 (no perspective): use coords as-is
            (tmu.sow, tmu.tow)
        };
        // Use ooz if set, otherwise fall back to oow
        // Normalize depth: Glide uses ooz = 65535/z, we need 0-1 range
        let depth = if v.ooz != 0.0 {
            (v.ooz / 65535.0).clamp(0.0, 1.0)
        } else {
            v.oow.clamp(0.0, 1.0)
        };
        GpuVertex {
            position: [v.x, v.y],
            color: [v.r / 255.0, v.g / 255.0, v.b / 255.0, v.a / 255.0],
            tex_coord: [s, t],
            oow: depth,
        }
    }

    /// Create vertex from Glide vertex with explicit RGBA color (normalized 0.0-1.0)
    pub fn from_glide_vertex_with_color(
        v: &crate::glidesys::Vertex,
        r: f32,
        g: f32,
        b: f32,
        a: f32,
    ) -> Self {
        // Get perspective-divided texture coords (sow = s/w, tow = t/w)
        // and convert to actual texel space if needed
        let tmu = &v.tmuvtx[0];
        let (s, t) = if tmu.oow > 0.0 && tmu.oow != 1.0 {
            // Perspective-divided coords: s = sow / oow, t = tow / oow
            (tmu.sow / tmu.oow, tmu.tow / tmu.oow)
        } else {
            // oow is 0 (undefined) or 1 (no perspective): use coords as-is
            (tmu.sow, tmu.tow)
        };
        // Use ooz if set, otherwise fall back to oow
        // Normalize depth: Glide uses ooz = 65535/z, we need 0-1 range
        let depth = if v.ooz != 0.0 {
            (v.ooz / 65535.0).clamp(0.0, 1.0)
        } else {
            v.oow.clamp(0.0, 1.0)
        };
        GpuVertex {
            position: [v.x, v.y],
            color: [r, g, b, a],
            tex_coord: [s, t],
            oow: depth,
        }
    }

    /// Create vertex from raw bytes using vertex layout
    /// Color components (r, g, b, a) are 0-255 range
    /// s, t are perspective-divided texture coords (s/w, t/w)
    /// oow is 1/w for perspective correction
    pub fn from_layout_data(
        x: f32,
        y: f32,
        r: f32,
        g: f32,
        b: f32,
        a: f32,
        s: f32,
        t: f32,
        oow: f32,
    ) -> Self {
        // Apply perspective correction if oow is not 0 or 1
        // s_actual = (s/w) / (1/w) = s
        let (s_corrected, t_corrected) = if oow > 0.0 && oow != 1.0 {
            (s / oow, t / oow)
        } else {
            (s, t)
        };
        GpuVertex {
            position: [x, y],
            color: [r / 255.0, g / 255.0, b / 255.0, a / 255.0],
            tex_coord: [s_corrected, t_corrected],
            oow: oow.clamp(0.0, 1.0),
        }
    }

    pub fn get_binding_description() -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription::default()
            .binding(0)
            .stride(std::mem::size_of::<GpuVertex>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX)
    }

    pub fn get_attribute_descriptions() -> [vk::VertexInputAttributeDescription; 4] {
        [
            // Position attribute
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(0)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(offset_of!(GpuVertex, position) as u32),
            // Color attribute
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(1)
                .format(vk::Format::R32G32B32A32_SFLOAT)
                .offset(offset_of!(GpuVertex, color) as u32),
            // Texture coordinate attribute
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(2)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(offset_of!(GpuVertex, tex_coord) as u32),
            // Depth (oow) attribute
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(3)
                .format(vk::Format::R32_SFLOAT)
                .offset(offset_of!(GpuVertex, oow) as u32),
        ]
    }
}

/// Push constants for the shaders.
/// New fields added after constant_color use only int/float (no vec3)
/// to avoid GLSL std430 vec3 alignment ambiguity.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PushConstants {
    pub screen_size: [f32; 2],
    pub texture_size: [f32; 2],
    pub chromakey_color: [f32; 3],
    pub chromakey_enabled: f32,
    // Color/alpha combine mode (integer enums passed as i32)
    pub color_combine_function: i32,
    pub color_combine_other: i32,
    pub alpha_combine_function: i32,
    pub alpha_combine_local: i32,
    // Constant color (RGBA, normalized 0.0-1.0)
    pub constant_color: [f32; 4],       // offset 48 (16-byte aligned)
    // Combine factors and sources
    pub color_combine_factor: i32,
    pub alpha_combine_factor: i32,
    pub color_combine_local: i32,
    pub alpha_combine_other: i32,
    // Fog state (separate floats, not vec3, to avoid std430 padding)
    pub fog_mode: i32,
    pub fog_color_r: f32,
    pub fog_color_g: f32,
    pub fog_color_b: f32,
}

/// Blend mode for a draw call — maps Glide's grAlphaBlendFunction to a pipeline variant
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlendMode {
    /// src*1 + dst*0 — opaque replacement
    Opaque,
    /// src*srcAlpha + dst*(1-srcAlpha) — standard transparency
    AlphaBlend,
    /// src*srcAlpha + dst*1 — additive glow (lightning, spell effects)
    Additive,
    /// src*0 + dst*srcColor — multiplicative darkening (shadows)
    Multiplicative,
}

/// Per-draw-call combine/color state (captured when state changes during a frame)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CombineState {
    pub color_combine_function: i32,
    pub color_combine_other: i32,
    pub alpha_combine_function: i32,
    pub alpha_combine_local: i32,
    pub color_combine_factor: i32,
    pub alpha_combine_factor: i32,
    pub color_combine_local: i32,
    pub alpha_combine_other: i32,
    pub constant_color: [f32; 4],
}

impl Default for CombineState {
    fn default() -> Self {
        Self {
            color_combine_function: 3, // ScaleOther
            color_combine_other: 1,    // Texture
            alpha_combine_function: 3, // ScaleOther
            alpha_combine_local: 0,    // Iterated
            color_combine_factor: 1,   // Local
            alpha_combine_factor: 1,   // Local
            color_combine_local: 0,    // Iterated
            alpha_combine_other: 0,    // Iterated
            constant_color: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

/// A single draw call with its associated texture and render state
#[derive(Clone, Copy, Debug)]
pub struct DrawCall {
    pub texture_addr: u32,
    pub start_vertex: u32,
    pub vertex_count: u32,
    pub chromakey_enabled: bool,
    pub blend_mode: BlendMode,
    pub combine_state: CombineState,
}

/// Vertex batch for collecting draw calls
pub struct VertexBatch {
    pub vertices: Vec<GpuVertex>,
    pub draw_calls: Vec<DrawCall>,
    max_vertices: usize,
    current_texture: u32,
    current_start: u32,
    current_chromakey: bool,
    current_blend_mode: BlendMode,
    current_combine_state: CombineState,
}

impl VertexBatch {
    pub fn new(max_vertices: usize) -> Self {
        VertexBatch {
            vertices: Vec::with_capacity(max_vertices),
            draw_calls: Vec::with_capacity(256),
            max_vertices,
            current_texture: 0,
            current_start: 0,
            current_chromakey: false,
            current_blend_mode: BlendMode::Opaque,
            current_combine_state: CombineState::default(),
        }
    }

    /// Get the current texture address
    pub fn current_texture(&self) -> u32 {
        self.current_texture
    }

    /// Set the current texture for subsequent vertices
    pub fn set_texture(&mut self, texture_addr: u32) {
        if texture_addr != self.current_texture && !self.vertices.is_empty() {
            // Texture changed - finalize current draw call if we have vertices
            let current_count = self.vertices.len() as u32 - self.current_start;
            if current_count > 0 {
                self.draw_calls.push(DrawCall {
                    texture_addr: self.current_texture,
                    start_vertex: self.current_start,
                    vertex_count: current_count,
                    chromakey_enabled: self.current_chromakey,
                    blend_mode: self.current_blend_mode,
                    combine_state: self.current_combine_state,
                });
                self.current_start = self.vertices.len() as u32;
            }
        }
        self.current_texture = texture_addr;
    }

    /// Set the chromakey mode for subsequent vertices
    pub fn set_chromakey(&mut self, enabled: bool) {
        if enabled != self.current_chromakey && !self.vertices.is_empty() {
            // Chromakey changed - finalize current draw call if we have vertices
            let current_count = self.vertices.len() as u32 - self.current_start;
            if current_count > 0 {
                self.draw_calls.push(DrawCall {
                    texture_addr: self.current_texture,
                    start_vertex: self.current_start,
                    vertex_count: current_count,
                    chromakey_enabled: self.current_chromakey,
                    blend_mode: self.current_blend_mode,
                    combine_state: self.current_combine_state,
                });
                self.current_start = self.vertices.len() as u32;
            }
        }
        self.current_chromakey = enabled;
    }

    /// Set the blend mode for subsequent vertices
    pub fn set_blend_mode(&mut self, mode: BlendMode) {
        if mode != self.current_blend_mode && !self.vertices.is_empty() {
            let current_count = self.vertices.len() as u32 - self.current_start;
            if current_count > 0 {
                self.draw_calls.push(DrawCall {
                    texture_addr: self.current_texture,
                    start_vertex: self.current_start,
                    vertex_count: current_count,
                    chromakey_enabled: self.current_chromakey,
                    blend_mode: self.current_blend_mode,
                    combine_state: self.current_combine_state,
                });
                self.current_start = self.vertices.len() as u32;
            }
        }
        self.current_blend_mode = mode;
    }

    /// Set the combine/constant color state for subsequent vertices
    pub fn set_combine_state(&mut self, state: CombineState) {
        if state != self.current_combine_state && !self.vertices.is_empty() {
            let current_count = self.vertices.len() as u32 - self.current_start;
            if current_count > 0 {
                self.draw_calls.push(DrawCall {
                    texture_addr: self.current_texture,
                    start_vertex: self.current_start,
                    vertex_count: current_count,
                    chromakey_enabled: self.current_chromakey,
                    blend_mode: self.current_blend_mode,
                    combine_state: self.current_combine_state,
                });
                self.current_start = self.vertices.len() as u32;
            }
        }
        self.current_combine_state = state;
    }

    pub fn push_triangle(&mut self, v0: GpuVertex, v1: GpuVertex, v2: GpuVertex) -> bool {
        if self.vertices.len() + 3 > self.max_vertices {
            return false;
        }
        self.vertices.push(v0);
        self.vertices.push(v1);
        self.vertices.push(v2);
        true
    }

    /// Finalize any pending draw call and return all draw calls
    pub fn finalize(&mut self) -> &[DrawCall] {
        let current_count = self.vertices.len() as u32 - self.current_start;
        if current_count > 0 {
            self.draw_calls.push(DrawCall {
                texture_addr: self.current_texture,
                start_vertex: self.current_start,
                vertex_count: current_count,
                chromakey_enabled: self.current_chromakey,
                blend_mode: self.current_blend_mode,
                combine_state: self.current_combine_state,
            });
            self.current_start = self.vertices.len() as u32;
        }
        &self.draw_calls
    }

    pub fn clear(&mut self) {
        self.vertices.clear();
        self.draw_calls.clear();
        self.current_start = 0;
        // Don't reset chromakey - it persists across frames
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                self.vertices.as_ptr() as *const u8,
                self.vertices.len() * std::mem::size_of::<GpuVertex>(),
            )
        }
    }
}

/// Manages a Vulkan vertex buffer with staging
pub struct VertexBuffer {
    pub buffer: vk::Buffer,
    pub memory: vk::DeviceMemory,
    pub size: vk::DeviceSize,
}

impl VertexBuffer {
    pub fn new(
        device: &ash::Device,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
        properties: vk::MemoryPropertyFlags,
    ) -> Self {
        let buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe {
            device
                .create_buffer(&buffer_info, None)
                .expect("Failed to create buffer")
        };

        let mem_requirements = unsafe { device.get_buffer_memory_requirements(buffer) };

        let memory_type_index = find_memory_type(
            instance,
            physical_device,
            mem_requirements.memory_type_bits,
            properties,
        );

        let alloc_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_requirements.size)
            .memory_type_index(memory_type_index);

        let memory = unsafe {
            device
                .allocate_memory(&alloc_info, None)
                .expect("Failed to allocate buffer memory")
        };

        unsafe {
            device
                .bind_buffer_memory(buffer, memory, 0)
                .expect("Failed to bind buffer memory");
        }

        VertexBuffer {
            buffer,
            memory,
            size,
        }
    }

    pub fn destroy(&self, device: &ash::Device) {
        unsafe {
            device.destroy_buffer(self.buffer, None);
            device.free_memory(self.memory, None);
        }
    }
}

fn find_memory_type(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    type_filter: u32,
    properties: vk::MemoryPropertyFlags,
) -> u32 {
    let mem_properties = unsafe { instance.get_physical_device_memory_properties(physical_device) };

    for i in 0..mem_properties.memory_type_count {
        if (type_filter & (1 << i)) != 0
            && mem_properties.memory_types[i as usize]
                .property_flags
                .contains(properties)
        {
            return i;
        }
    }

    panic!("Failed to find suitable memory type!");
}

/// Copy data from CPU to a host-visible buffer
pub fn upload_to_buffer(device: &ash::Device, buffer: &VertexBuffer, data: &[u8]) {
    unsafe {
        let data_ptr = device
            .map_memory(
                buffer.memory,
                0,
                data.len() as vk::DeviceSize,
                vk::MemoryMapFlags::empty(),
            )
            .expect("Failed to map memory");

        std::ptr::copy_nonoverlapping(data.as_ptr(), data_ptr as *mut u8, data.len());

        device.unmap_memory(buffer.memory);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_vertex_new() {
        let v = GpuVertex::new(100.0, 200.0, 1.0, 0.5, 0.25, 1.0, 0.5, 0.75);
        assert_eq!(v.position, [100.0, 200.0]);
        assert_eq!(v.color, [1.0, 0.5, 0.25, 1.0]);
        assert_eq!(v.tex_coord, [0.5, 0.75]);
    }

    #[test]
    fn test_gpu_vertex_default() {
        let v = GpuVertex::default();
        assert_eq!(v.position, [0.0, 0.0]);
        assert_eq!(v.color, [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(v.tex_coord, [0.0, 0.0]);
    }

    #[test]
    fn test_gpu_vertex_size() {
        // Ensure vertex size is as expected for buffer calculations
        // 2 floats (position) + 4 floats (color) + 2 floats (texcoord) + 1 float (oow) = 9 floats = 36 bytes
        assert_eq!(std::mem::size_of::<GpuVertex>(), 36);
    }

    #[test]
    fn test_push_constants_size() {
        // screen_size(8) + texture_size(8) + chromakey_color(12) + chromakey_enabled(4)
        // + 4 combine ints(16) + constant_color(16) + 4 ints(16) + fog(16) = 96 bytes
        assert_eq!(std::mem::size_of::<PushConstants>(), 96);
    }

    #[test]
    fn test_vertex_batch_new() {
        let batch = VertexBatch::new(100);
        assert!(batch.is_empty());
        assert_eq!(batch.vertex_count(), 0);
    }

    #[test]
    fn test_vertex_batch_push_triangle() {
        let mut batch = VertexBatch::new(100);
        let v0 = GpuVertex::new(0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        let v1 = GpuVertex::new(100.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0);
        let v2 = GpuVertex::new(50.0, 100.0, 0.0, 0.0, 1.0, 1.0, 0.5, 1.0);

        assert!(batch.push_triangle(v0, v1, v2));
        assert!(!batch.is_empty());
        assert_eq!(batch.vertex_count(), 3);
    }

    #[test]
    fn test_vertex_batch_multiple_triangles() {
        let mut batch = VertexBatch::new(100);
        let v = GpuVertex::default();

        // Add 3 triangles (9 vertices)
        assert!(batch.push_triangle(v, v, v));
        assert!(batch.push_triangle(v, v, v));
        assert!(batch.push_triangle(v, v, v));

        assert_eq!(batch.vertex_count(), 9);
    }

    #[test]
    fn test_vertex_batch_overflow() {
        let mut batch = VertexBatch::new(6); // Only room for 2 triangles
        let v = GpuVertex::default();

        assert!(batch.push_triangle(v, v, v)); // 3 vertices
        assert!(batch.push_triangle(v, v, v)); // 6 vertices
        assert!(!batch.push_triangle(v, v, v)); // Would be 9, should fail

        assert_eq!(batch.vertex_count(), 6);
    }

    #[test]
    fn test_vertex_batch_clear() {
        let mut batch = VertexBatch::new(100);
        let v = GpuVertex::default();

        batch.push_triangle(v, v, v);
        assert_eq!(batch.vertex_count(), 3);

        batch.clear();
        assert!(batch.is_empty());
        assert_eq!(batch.vertex_count(), 0);
    }

    #[test]
    fn test_vertex_batch_as_bytes() {
        let mut batch = VertexBatch::new(100);
        let v = GpuVertex::default();

        batch.push_triangle(v, v, v);

        let bytes = batch.as_bytes();
        // 3 vertices * 36 bytes each = 108 bytes
        assert_eq!(bytes.len(), 108);
    }

    #[test]
    fn test_binding_description() {
        let binding = GpuVertex::get_binding_description();
        assert_eq!(binding.binding, 0);
        assert_eq!(binding.stride, 36); // size of GpuVertex
    }

    #[test]
    fn test_attribute_descriptions() {
        let attrs = GpuVertex::get_attribute_descriptions();
        assert_eq!(attrs.len(), 4);

        // Position at location 0, offset 0
        assert_eq!(attrs[0].location, 0);
        assert_eq!(attrs[0].offset, 0);

        // Color at location 1, offset 8 (after 2 floats)
        assert_eq!(attrs[1].location, 1);
        assert_eq!(attrs[1].offset, 8);

        // TexCoord at location 2, offset 24 (after 2 + 4 floats)
        assert_eq!(attrs[2].location, 2);
        assert_eq!(attrs[2].offset, 24);

        // Oow at location 3, offset 32 (after 2 + 4 + 2 floats)
        assert_eq!(attrs[3].location, 3);
        assert_eq!(attrs[3].offset, 32);
    }
}
