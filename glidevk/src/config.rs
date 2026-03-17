//! Configuration module for GlideVK wrapper
//!
//! Loads settings from a TOML configuration file (glidevk.toml)

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Static size presets for window/display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StaticSize {
    /// No static size - use game's native resolution
    #[default]
    None,
    /// 640x480
    Vga,
    /// 800x600
    Svga,
    /// 1024x768
    Xga,
    /// 1280x960
    Sxga,
    /// 1600x1200
    Uxga,
    /// 1920x1440
    Qxga,
    /// Use desktop resolution
    Desktop,
    /// Custom resolution
    Custom { width: u32, height: u32 },
}

impl StaticSize {
    /// Get the dimensions for this size preset (Desktop returns None, must be resolved at runtime)
    pub fn dimensions(&self) -> Option<(u32, u32)> {
        match self {
            StaticSize::None => None,
            StaticSize::Vga => Some((640, 480)),
            StaticSize::Svga => Some((800, 600)),
            StaticSize::Xga => Some((1024, 768)),
            StaticSize::Sxga => Some((1280, 960)),
            StaticSize::Uxga => Some((1600, 1200)),
            StaticSize::Qxga => Some((1920, 1440)),
            StaticSize::Desktop => None, // Resolved at runtime
            StaticSize::Custom { width, height } => Some((*width, *height)),
        }
    }

    /// Resolve dimensions with desktop size fallback
    /// `desktop_size` should be the current monitor resolution
    pub fn resolve(&self, desktop_size: (u32, u32)) -> Option<(u32, u32)> {
        match self {
            StaticSize::Desktop => Some(desktop_size),
            other => other.dimensions(),
        }
    }

    /// Check if this is a "use desktop resolution" setting
    pub fn is_desktop(&self) -> bool {
        matches!(self, StaticSize::Desktop)
    }

    /// Check if any static size is configured (not None)
    pub fn is_set(&self) -> bool {
        !matches!(self, StaticSize::None)
    }
}

/// Texture memory presets (in MB)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TextureMemory {
    /// 2 MB
    Mb2,
    /// 4 MB
    Mb4,
    /// 8 MB
    #[default]
    Mb8,
    /// 16 MB
    Mb16,
    /// 32 MB
    Mb32,
    /// 64 MB
    Mb64,
    /// 128 MB
    Mb128,
    /// 256 MB
    Mb256,
    /// Custom value in MB
    Custom(u32),
}

impl TextureMemory {
    /// Get the texture memory size in bytes
    pub fn bytes(&self) -> u32 {
        let mb = match self {
            TextureMemory::Mb2 => 2,
            TextureMemory::Mb4 => 4,
            TextureMemory::Mb8 => 8,
            TextureMemory::Mb16 => 16,
            TextureMemory::Mb32 => 32,
            TextureMemory::Mb64 => 64,
            TextureMemory::Mb128 => 128,
            TextureMemory::Mb256 => 256,
            TextureMemory::Custom(mb) => *mb,
        };
        mb * 1024 * 1024
    }
}

/// Buffer texture size presets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BufferTextureSize {
    /// 256x256
    Size256,
    /// 512x512
    Size512,
    /// 1024x1024 (recommended)
    #[default]
    Size1024,
    /// 2048x2048
    Size2048,
    /// 4096x4096
    Size4096,
}

impl BufferTextureSize {
    /// Get the buffer texture size in pixels
    pub fn pixels(&self) -> u32 {
        match self {
            BufferTextureSize::Size256 => 256,
            BufferTextureSize::Size512 => 512,
            BufferTextureSize::Size1024 => 1024,
            BufferTextureSize::Size2048 => 2048,
            BufferTextureSize::Size4096 => 4096,
        }
    }
}

/// Window configuration settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowConfig {
    /// Run in windowed mode instead of fullscreen
    /// Overrides the "-w" command-line parameter
    pub windowed: bool,

    /// Keep 4:3 aspect ratio when running in windowed mode or at desktop resolution
    /// Additional space is filled with black bars
    pub keep_aspect_ratio: bool,

    /// Static size preset for the window/display
    /// In fullscreen mode, surrounding area is filled black
    pub static_size: StaticSize,

    /// Allow resizing the window in windowed mode
    /// Window content will be scaled to fit
    pub resizable: bool,

    /// Center the window on the desktop (only in windowed mode)
    pub centered: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            windowed: false,
            keep_aspect_ratio: true,
            static_size: StaticSize::None,
            resizable: false,
            centered: true,
        }
    }
}

/// Texture and rendering configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TextureConfig {
    /// Amount of texture memory to report to the game
    /// Too high values may cause texture glitches or driver issues
    pub memory: TextureMemory,

    /// Size of buffer textures used internally
    /// Smaller = more game textures fit on GPU
    /// Larger = more game textures fit in single buffer
    /// 1024x1024 is recommended for most hardware
    pub buffer_size: BufferTextureSize,
}

impl Default for TextureConfig {
    fn default() -> Self {
        Self {
            memory: TextureMemory::Mb8,
            buffer_size: BufferTextureSize::Size1024,
        }
    }
}

/// Rendering quality settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderConfig {
    /// Enable bilinear filtering
    /// Renders scene to texture first, then to screen
    /// Reduces framerate but increases image quality at desktop resolution
    pub bilinear_filtering: bool,

    /// Enable VSync
    pub vsync: bool,

    /// Supersampling factor (1 = disabled, 2 = 2x, 4 = 4x)
    pub supersampling: u32,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            bilinear_filtering: true,
            vsync: true,
            supersampling: 1,
        }
    }
}

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    /// Window settings
    pub window: WindowConfig,

    /// Texture settings
    pub texture: TextureConfig,

    /// Rendering settings
    pub render: RenderConfig,
}

impl Config {
    /// Default configuration file name
    pub const DEFAULT_FILENAME: &'static str = "glidevk.toml";

    /// Load configuration from the default file (glidevk.toml)
    /// Returns default config if file doesn't exist
    pub fn load() -> Self {
        Self::load_from(Self::DEFAULT_FILENAME)
    }

    /// Load configuration from a specific file path
    /// Returns default config if file doesn't exist or is invalid
    pub fn load_from<P: AsRef<Path>>(path: P) -> Self {
        let path = path.as_ref();

        match fs::read_to_string(path) {
            Ok(contents) => match toml::from_str(&contents) {
                Ok(config) => {
                    log::info!("Loaded configuration from {}", path.display());
                    config
                }
                Err(e) => {
                    log::warn!(
                        "Failed to parse configuration file {}: {}. Using defaults.",
                        path.display(),
                        e
                    );
                    Self::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                log::info!(
                    "Configuration file {} not found. Using defaults.",
                    path.display()
                );
                Self::default()
            }
            Err(e) => {
                log::warn!(
                    "Failed to read configuration file {}: {}. Using defaults.",
                    path.display(),
                    e
                );
                Self::default()
            }
        }
    }

    /// Save configuration to the default file
    pub fn save(&self) -> Result<(), std::io::Error> {
        self.save_to(Self::DEFAULT_FILENAME)
    }

    /// Save configuration to a specific file path
    pub fn save_to<P: AsRef<Path>>(&self, path: P) -> Result<(), std::io::Error> {
        let contents = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(path, contents)
    }

    /// Generate a default configuration file with comments
    pub fn generate_default_file<P: AsRef<Path>>(path: P) -> Result<(), std::io::Error> {
        let default_config = r#"# GlideVK Wrapper Configuration
# Place this file in the game directory as 'glidevk.toml'

[window]
# Run in windowed mode instead of fullscreen
# Overrides the "-w" command-line parameter
windowed = false

# Keep 4:3 aspect ratio when running in windowed mode or at desktop resolution
# Additional space is filled with black bars
keep_aspect_ratio = true

# Static size preset for the window/display
# Options: "none", "vga", "svga", "xga", "sxga", "uxga", "qxga", "desktop"
# Or use custom: { width = 1920, height = 1080 }
# In fullscreen mode, surrounding area is filled black
static_size = "none"

# Allow resizing the window in windowed mode
# Window content will be scaled to fit
resizable = false

# Center the window on the desktop (only in windowed mode)
centered = true

[texture]
# Amount of texture memory to report to the game
# Options: "mb2", "mb4", "mb8", "mb16", "mb32", "mb64", "mb128", "mb256"
# Or use custom: { Custom = 24 } for 24 MB
# WARNING: Too high values may cause texture glitches or driver issues
memory = "mb8"

# Size of buffer textures used internally
# Options: "Size256", "Size512", "Size1024", "Size2048", "Size4096"
# Smaller = more game textures fit on GPU
# Larger = more game textures fit in single buffer
# 1024x1024 is recommended for most hardware
buffer_size = "Size1024"

[render]
# Enable bilinear filtering
# Renders scene to texture first, then to screen
# Reduces framerate but increases image quality at desktop resolution
bilinear_filtering = true

# Enable VSync
vsync = true

# Supersampling factor (1 = disabled, 2 = 2x, 4 = 4x)
supersampling = 1
"#;
        fs::write(path, default_config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(!config.window.windowed);
        assert!(config.window.keep_aspect_ratio);
        assert_eq!(config.texture.memory.bytes(), 8 * 1024 * 1024);
        assert_eq!(config.texture.buffer_size.pixels(), 1024);
    }

    #[test]
    fn test_texture_memory_bytes() {
        assert_eq!(TextureMemory::Mb2.bytes(), 2 * 1024 * 1024);
        assert_eq!(TextureMemory::Mb8.bytes(), 8 * 1024 * 1024);
        assert_eq!(TextureMemory::Custom(24).bytes(), 24 * 1024 * 1024);
    }

    #[test]
    fn test_static_size_dimensions() {
        assert_eq!(StaticSize::Vga.dimensions(), Some((640, 480)));
        assert_eq!(StaticSize::Xga.dimensions(), Some((1024, 768)));
        assert_eq!(StaticSize::None.dimensions(), None);
        assert_eq!(
            StaticSize::Custom {
                width: 1920,
                height: 1080
            }
            .dimensions(),
            Some((1920, 1080))
        );
    }

    #[test]
    fn test_serialize_deserialize() {
        let config = Config::default();
        let toml_str = toml::to_string(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(config.window.windowed, parsed.window.windowed);
        assert_eq!(
            config.texture.buffer_size.pixels(),
            parsed.texture.buffer_size.pixels()
        );
    }
}
