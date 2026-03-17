//! Window handling abstraction for GlideVK
//!
//! Provides a reusable window and event loop management layer.

use winit::event_loop::EventLoop;
use winit::window::{Window, WindowBuilder};

/// Rendering mode for the application
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    /// Windowed mode with visible window
    Windowed,
    /// Headless mode - hidden window for offscreen rendering
    Headless,
}

impl Default for RenderMode {
    fn default() -> Self {
        RenderMode::Windowed
    }
}

/// Configuration for window creation
#[derive(Debug, Clone)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub mode: RenderMode,
}

impl WindowConfig {
    /// Create a new windowed configuration
    pub fn new(title: &str, width: u32, height: u32) -> Self {
        Self {
            title: title.to_string(),
            width,
            height,
            mode: RenderMode::Windowed,
        }
    }

    /// Create a headless configuration (hidden window)
    pub fn headless(width: u32, height: u32) -> Self {
        Self {
            title: String::new(),
            width,
            height,
            mode: RenderMode::Headless,
        }
    }

    /// Set the render mode
    pub fn with_mode(mut self, mode: RenderMode) -> Self {
        self.mode = mode;
        self
    }
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "GlideVK".to_string(),
            width: 640,
            height: 480,
            mode: RenderMode::Windowed,
        }
    }
}

/// Main application runner that manages window and event loop
pub struct GlideApp {
    config: WindowConfig,
    event_loop: Option<EventLoop<()>>,
    window: Option<Window>,
}

impl GlideApp {
    /// Create a new GlideApp with the given configuration
    pub fn new(config: WindowConfig) -> Self {
        let event_loop = EventLoop::new().expect("Failed to create event loop");

        let window = WindowBuilder::new()
            .with_title(&config.title)
            .with_inner_size(winit::dpi::LogicalSize::new(config.width, config.height))
            .with_visible(config.mode == RenderMode::Windowed)
            .build(&event_loop)
            .expect("Failed to create window");

        GlideApp {
            config,
            event_loop: Some(event_loop),
            window: Some(window),
        }
    }

    /// Get the window configuration
    pub fn config(&self) -> &WindowConfig {
        &self.config
    }

    /// Take ownership of the window (for passing to Gr::init)
    pub fn take_window(&mut self) -> Option<Window> {
        self.window.take()
    }

    /// Take ownership of the event loop (for running the main loop)
    pub fn take_event_loop(&mut self) -> Option<EventLoop<()>> {
        self.event_loop.take()
    }

    /// Check if in headless mode
    pub fn is_headless(&self) -> bool {
        self.config.mode == RenderMode::Headless
    }
}

/// Trait for window interaction callbacks
pub trait WindowInteraction {
    fn draw_frame(&mut self, delta_time: f32);
    fn recreate_swapchain(&mut self);
    fn cleanup_swapchain(&self);
    fn wait_device_idle(&self);
    fn resize_framebuffer(&mut self);
    fn window_ref(&self) -> Option<&Window>;
}

/// Simple window creation helper (legacy API)
pub fn init_window(event_loop: &EventLoop<()>, title: &str, width: u32, height: u32) -> Window {
    WindowBuilder::new()
        .with_title(title)
        .with_inner_size(winit::dpi::LogicalSize::new(width, height))
        .build(event_loop)
        .expect("Failed to create window.")
}
