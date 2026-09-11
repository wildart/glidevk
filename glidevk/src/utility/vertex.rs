use ash::vk;
use std::mem::offset_of;

/// GPU vertex format for Glide rendering
/// This matches the layout expected by glide.vert shader
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct GpuVertex {
    pub position: [f32; 2],       // Screen-space x, y
    pub color: [f32; 4],          // RGBA color (iterated vertex color, normalized)
    pub tex_coord: [f32; 2],      // Texture S, T coordinates
    pub oow: f32,                 // Reciprocal W (1/w) for depth
    pub constant_color: [f32; 4], // Constant color from grConstantColorValue (per-vertex)
    pub combine_flags: u32,       // Packed combine state (per-vertex, avoids draw call breaks)
    pub texture_index: u32,
    pub depth: f32,
    pub tex_q: f32,
    pub tex_coord1: [f32; 2],
    pub tex_q1: f32,
    pub texture_index1: u32,
    pub fog: f32, // Index into descriptor array (per-vertex, avoids draw call breaks)
}

impl GpuVertex {
    pub fn new(x: f32, y: f32, r: f32, g: f32, b: f32, a: f32, s: f32, t: f32) -> Self {
        GpuVertex {
            position: [x, y],
            color: [r, g, b, a],
            tex_coord: [s, t],
            oow: 1.0,
            constant_color: [1.0, 1.0, 1.0, 1.0],
            combine_flags: 0,
            texture_index: 0,
            depth: 0.0,
            tex_q: 1.0,
            fog: 0.0,
            tex_coord1: [0.0; 2],
            tex_q1: 1.0,
            texture_index1: 0,
        }
    }

    /// Create vertex from Glide vertex using per-vertex colors
    pub fn from_glide_vertex(v: &crate::glidesys::Vertex) -> Self {
        let (s, t) = (v.tmuvtx[0].sow, v.tmuvtx[0].tow);
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
            oow: v.oow,
            constant_color: [1.0, 1.0, 1.0, 1.0],
            combine_flags: 0,
            texture_index: 0,
            depth: depth,
            tex_q: if v.tmuvtx[0].oow == 0.0 {
                1.0
            } else {
                v.tmuvtx[0].oow
            },
            fog: 0.0,
            tex_coord1: [v.tmuvtx[1].sow, v.tmuvtx[1].tow],
            tex_q1: if v.tmuvtx[1].oow == 0.0 {
                1.0
            } else {
                v.tmuvtx[1].oow
            },
            texture_index1: 0,
        }
    }

    /// Create a vertex with explicit normalized RGBA color.
    pub fn from_glide_vertex_with_color(
        v: &crate::glidesys::Vertex,
        r: f32,
        g: f32,
        b: f32,
        a: f32,
    ) -> Self {
        Self {
            color: [r, g, b, a],
            ..Self::from_glide_vertex(v)
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
        GpuVertex {
            position: [x, y],
            color: [r / 255.0, g / 255.0, b / 255.0, a / 255.0],
            tex_coord: [s, t],
            oow,
            constant_color: [1.0, 1.0, 1.0, 1.0],
            combine_flags: 0,
            texture_index: 0,
            depth: 0.0,
            tex_q: if oow == 0.0 { 1.0 } else { oow },
            fog: 0.0,
            tex_coord1: [0.0; 2],
            tex_q1: 1.0,
            texture_index1: 0,
        }
    }

    pub fn get_binding_description() -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription::default()
            .binding(0)
            .stride(std::mem::size_of::<GpuVertex>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX)
    }

    pub fn get_attribute_descriptions() -> [vk::VertexInputAttributeDescription; 13] {
        [
            // Position attribute
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(0)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(offset_of!(GpuVertex, position) as u32),
            // Iterated color attribute
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
            // Constant color attribute (from grConstantColorValue, per-vertex)
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(4)
                .format(vk::Format::R32G32B32A32_SFLOAT)
                .offset(offset_of!(GpuVertex, constant_color) as u32),
            // Packed combine flags (per-vertex)
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(5)
                .format(vk::Format::R32_UINT)
                .offset(offset_of!(GpuVertex, combine_flags) as u32),
            // Texture array index (per-vertex, for descriptor indexing)
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(6)
                .format(vk::Format::R32_UINT)
                .offset(offset_of!(GpuVertex, texture_index) as u32),
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(7)
                .format(vk::Format::R32_SFLOAT)
                .offset(offset_of!(GpuVertex, depth) as u32),
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(8)
                .format(vk::Format::R32_SFLOAT)
                .offset(offset_of!(GpuVertex, tex_q) as u32),
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(9)
                .format(vk::Format::R32_SFLOAT)
                .offset(offset_of!(GpuVertex, fog) as u32),
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(10)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(offset_of!(GpuVertex, tex_coord1) as u32),
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(11)
                .format(vk::Format::R32_SFLOAT)
                .offset(offset_of!(GpuVertex, tex_q1) as u32),
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(12)
                .format(vk::Format::R32_UINT)
                .offset(offset_of!(GpuVertex, texture_index1) as u32),
        ]
    }
}

/// Push constants for the shaders.
/// Constant color moved to vertex attribute to avoid draw call breaks.
/// texture_size removed: computed per-vertex in the shader via textureSize().
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PushConstants {
    pub screen_size: [f32; 2],
    pub chromakey_r: f32,
    pub chromakey_g: f32,
    pub chromakey_b: f32,
    pub fog_mode: i32,
    pub fog_color_r: f32,
    pub fog_color_g: f32,
    pub fog_color_b: f32,
    pub gamma_r: f32,
    pub gamma_g: f32,
    pub gamma_b: f32,
    pub fog_table: [u32; 16],
    pub tex_combine: [u32; 2],
    pub alpha_test: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PipelineState {
    pub blend: [i32; 4],
    pub depth_test: bool,
    pub depth_write: bool,
    pub depth_compare: i32,
    pub color_mask: u32,
}
impl Default for PipelineState {
    fn default() -> Self {
        Self {
            blend: [1, 0, 1, 0],
            depth_test: false,
            depth_write: false,
            depth_compare: vk::CompareOp::LESS_OR_EQUAL.as_raw(),
            color_mask: vk::ColorComponentFlags::RGBA.as_raw(),
        }
    }
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawState {
    pub pipeline: PipelineState,
    pub push: PushConstants,
    pub clip: vk::Rect2D,
}

/// A single draw call with render state that requires a pipeline/descriptor change.
/// Texture selection is per-vertex (via texture_index), not per-draw-call.
/// Combine state is per-vertex (in combine_flags), not per-draw-call.
#[derive(Clone, Copy, Debug)]
pub struct DrawCall {
    pub start_vertex: u32,
    pub vertex_count: u32,
    pub blend_mode: BlendMode,
    pub state: Option<DrawState>,
}

/// Vertex batch for collecting draw calls.
/// Draw calls break when captured pipeline, scissor, or push-constant state changes.
/// Texture selection is per-vertex via texture_index.
pub struct VertexBatch {
    pub vertices: Vec<GpuVertex>,
    pub draw_calls: Vec<DrawCall>,
    current_start: u32,
    current_blend_mode: BlendMode,
    current_state: Option<DrawState>,
}

impl VertexBatch {
    pub fn new(max_vertices: usize) -> Self {
        VertexBatch {
            vertices: Vec::with_capacity(max_vertices),
            draw_calls: Vec::with_capacity(256),
            current_start: 0,
            current_blend_mode: BlendMode::Opaque,
            current_state: None,
        }
    }

    /// Set the blend mode for subsequent vertices
    pub fn set_blend_mode(&mut self, mode: BlendMode) {
        if mode != self.current_blend_mode && !self.vertices.is_empty() {
            let current_count = self.vertices.len() as u32 - self.current_start;
            if current_count > 0 {
                self.draw_calls.push(DrawCall {
                    start_vertex: self.current_start,
                    vertex_count: current_count,
                    blend_mode: self.current_blend_mode,
                    state: self.current_state,
                });
                self.current_start = self.vertices.len() as u32;
            }
        }
        self.current_blend_mode = mode;
    }

    pub fn set_state(&mut self, state: DrawState) {
        if self.current_state != Some(state) {
            self.finalize();
            self.current_state = Some(state);
        }
    }

    pub fn push_triangle(&mut self, v0: GpuVertex, v1: GpuVertex, v2: GpuVertex) -> bool {
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
                start_vertex: self.current_start,
                vertex_count: current_count,
                blend_mode: self.current_blend_mode,
                state: self.current_state,
            });
            self.current_start = self.vertices.len() as u32;
        }
        &self.draw_calls
    }

    pub fn clear(&mut self) {
        self.vertices.clear();
        self.draw_calls.clear();
        self.current_start = 0;
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
        // 2+4+2+1 (position,color,texcoord,oow) + 4 (constant_color) + 1 (combine_flags) + 1 (texture_index) = 60 bytes
        assert_eq!(std::mem::size_of::<GpuVertex>(), 88);
    }

    #[test]
    fn test_push_constants_size() {
        // screen_size(8) + chromakey_rgb(12) + fog(16) + gamma_rgb(12) = 48
        assert_eq!(std::mem::size_of::<PushConstants>(), 124);
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
    fn batch_grows_past_old_frame_limit_and_preserves_perspective() {
        let mut batch = VertexBatch::new(6);
        let v = GpuVertex::default();
        for _ in 0..21846 {
            assert!(batch.push_triangle(v, v, v));
        }
        assert_eq!(batch.vertex_count(), 65538);
        let near = GpuVertex::from_layout_data(0.0, 0.0, 255.0, 255.0, 255.0, 255.0, 0.0, 0.0, 1.0);
        let far =
            GpuVertex::from_layout_data(1.0, 0.0, 255.0, 255.0, 255.0, 255.0, 64.0, 0.0, 0.25);
        let midpoint = (near.tex_coord[0] + far.tex_coord[0]) / (near.tex_q + far.tex_q);
        assert!((midpoint - 51.2).abs() < 0.001);
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
        // 3 vertices * 60 bytes each = 180 bytes
        assert_eq!(bytes.len(), 3 * std::mem::size_of::<GpuVertex>());
    }

    #[test]
    fn test_binding_description() {
        let binding = GpuVertex::get_binding_description();
        assert_eq!(binding.binding, 0);
        assert_eq!(binding.stride, std::mem::size_of::<GpuVertex>() as u32); // size of GpuVertex
    }

    #[test]
    fn test_attribute_descriptions() {
        let attrs = GpuVertex::get_attribute_descriptions();
        assert_eq!(attrs.len(), 13);

        assert_eq!(attrs[0].location, 0); // position at 0
        assert_eq!(attrs[0].offset, 0);
        assert_eq!(attrs[1].location, 1); // color at 8
        assert_eq!(attrs[1].offset, 8);
        assert_eq!(attrs[2].location, 2); // texcoord at 24
        assert_eq!(attrs[2].offset, 24);
        assert_eq!(attrs[3].location, 3); // oow at 32
        assert_eq!(attrs[3].offset, 32);
        assert_eq!(attrs[4].location, 4); // constant_color at 36
        assert_eq!(attrs[4].offset, 36);
        assert_eq!(attrs[5].location, 5); // combine_flags at 52
        assert_eq!(attrs[5].offset, 52);
        assert_eq!(attrs[6].location, 6); // texture_index at 56
        assert_eq!(attrs[6].offset, 56);
    }
}
