//! The native child surface mpv draws into. One facade, one file per platform.
//! Every method here is main-thread only: these are GTK and Win32 handles.

use crate::video::error::VideoError;

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod platform;

#[cfg(windows)]
#[path = "windows.rs"]
mod platform;

#[cfg(not(any(target_os = "linux", windows)))]
compile_error!("Sublore targets Windows and Linux; see CLAUDE.md section on platform policy.");

/// A rectangle in CSS pixels plus the window's scale factor. Each platform converts it as its own
/// window API expects: GDK geometry is logical, Win32 geometry is physical.
#[derive(Clone, Copy, Debug)]
pub struct SurfaceRegion {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub scale: f64,
}

/// X11 window coordinates are 16 bit, so clamp before casting rather than trusting the caller.
const COORD_LIMIT: f64 = i16::MAX as f64;

impl SurfaceRegion {
    fn scaled(&self, factor: f64) -> (i32, i32, i32, i32) {
        let clamp = |value: f64| (value * factor).round().clamp(-COORD_LIMIT, COORD_LIMIT) as i32;
        let size = |value: f64| (value * factor).round().clamp(1.0, COORD_LIMIT) as i32;
        (
            clamp(self.x),
            clamp(self.y),
            size(self.width),
            size(self.height),
        )
    }

    /// Logical pixels, for window systems that scale for us.
    #[cfg_attr(windows, allow(dead_code))]
    fn logical(&self) -> (i32, i32, i32, i32) {
        self.scaled(1.0)
    }

    /// Physical pixels, for window systems that do not.
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    fn physical(&self) -> (i32, i32, i32, i32) {
        self.scaled(self.scale)
    }

    /// A region with no area hides the surface instead of moving it.
    pub fn is_empty(&self) -> bool {
        !(self.width.is_finite() && self.height.is_finite())
            || self.width.round() <= 0.0
            || self.height.round() <= 0.0
    }
}

pub struct VideoSurface {
    inner: platform::Surface,
}

impl VideoSurface {
    /// Create the native child surface, hidden. Main thread only.
    pub fn create(window: &tauri::WebviewWindow) -> Result<Self, VideoError> {
        Ok(Self {
            inner: platform::create(window)?,
        })
    }

    /// Value for mpv's `wid` option. Stable for the surface's lifetime.
    pub fn wid(&self) -> i64 {
        self.inner.wid()
    }

    /// Move, resize and raise above the webview, without changing visibility. Main thread only.
    pub fn set_region(&self, region: SurfaceRegion) -> Result<(), VideoError> {
        self.inner.set_region(region)
    }

    /// Show at the last region set. Must happen before mpv builds its video output: mpv creates
    /// its own window inside this one and leaves it unmapped if this one is. Main thread only.
    pub fn show(&self) -> Result<(), VideoError> {
        self.inner.show()
    }

    /// Hide without destroying. Main thread only.
    pub fn hide(&self) -> Result<(), VideoError> {
        self.inner.hide()
    }

    /// Destroy. Main thread only, and only after mpv is gone.
    pub fn destroy(self) -> Result<(), VideoError> {
        self.inner.destroy()
    }
}
