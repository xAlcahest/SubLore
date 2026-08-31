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
compile_error!(
    "Sublore targets Windows and Linux; see CONTRIBUTING.md section on platform policy."
);

/// A rectangle in native device pixels, resolved by the page before it crosses the IPC boundary.
///
/// The ratio does not cross the IPC boundary: the page is the only party that knows the full one,
/// since `window.scale_factor()` is an integer in `tao` and reports 1 on a fractionally scaled
/// display, where the 1.5 arrives as page zoom instead. Each side still reads its own half
/// locally, and the geometry is right only while the two agree: the page re-reports on every ratio
/// change (`VideoStage.tsx`) so that they do.
///
/// What each platform does with these numbers is its own business and stays behind this type.
/// Win32 geometry is physical, so Windows takes them as they are. GDK multiplies child geometry by
/// the window's integer scale factor on the way to X, so the Linux backend divides by that factor
/// first, or an integer scale lands twice — measured at 4x instead of 2x under `GDK_SCALE=2`.
#[derive(Clone, Copy, Debug)]
pub struct SurfaceRegion {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// X11 window coordinates are 16 bit, so clamp before casting rather than trusting the caller.
const COORD_LIMIT: f64 = i16::MAX as f64;

impl SurfaceRegion {
    /// The rectangle as integers a window API can take, clamped to what X11 coordinates can hold.
    /// A size of zero would be rejected by both toolkits, so it floors at one; an empty region
    /// hides the surface and never reaches here (`is_empty`).
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    fn pixels(&self) -> (i32, i32, i32, i32) {
        self.pixels_over(1.0)
    }

    /// The same, divided by whatever the window system will multiply back. See the type's note.
    /// The division happens before rounding, so the result is the nearest whole pixel to the
    /// rectangle the page asked for rather than the nearest to an already rounded one.
    #[cfg_attr(windows, allow(dead_code))]
    fn pixels_over(&self, divisor: f64) -> (i32, i32, i32, i32) {
        let divisor = if divisor.is_finite() && divisor >= 1.0 {
            divisor
        } else {
            1.0
        };
        let edge = |value: f64| (value / divisor).round();
        // Edges first, then the size from them: the same rule the page uses, so a rectangle never
        // gains or loses a pixel to rounding each side on its own (`VideoStage.tsx`).
        let span = |start: f64, length: f64| {
            (edge(start + length) - edge(start)).clamp(1.0, COORD_LIMIT) as i32
        };
        (
            edge(self.x).clamp(-COORD_LIMIT, COORD_LIMIT) as i32,
            edge(self.y).clamp(-COORD_LIMIT, COORD_LIMIT) as i32,
            span(self.x, self.width),
            span(self.y, self.height),
        )
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

#[cfg(test)]
mod tests {
    use super::{SurfaceRegion, COORD_LIMIT};

    fn region(x: f64, y: f64, width: f64, height: f64) -> SurfaceRegion {
        SurfaceRegion {
            x,
            y,
            width,
            height,
        }
    }

    /// The numbers a page produces at a fractional ratio reach the window API unchanged. 682 x 1.5
    /// is 1023, which is what the owner's display actually measured; nothing here may quietly
    /// scale it a second time.
    #[test]
    fn a_fractionally_scaled_rectangle_passes_through() {
        assert_eq!(
            region(432.0, 444.0, 1023.0, 699.0).pixels(),
            (432, 444, 1023, 699)
        );
    }

    /// The two ratios that matter, against the two things a window system does with them. A
    /// fractional ratio comes from page zoom while GDK's own factor stays 1, so nothing is divided
    /// out and the native rectangle is what X gets. An integer ratio comes from GDK's factor, which
    /// GDK re-applies, so it has to be divided back out — measured at 4x instead of 2x when it was
    /// not (`e2e/scripts/scaled-surface-check.js`).
    #[test]
    fn the_divisor_undoes_only_what_the_window_system_re_applies() {
        let css = (288.0, 296.0, 512.0, 120.0);

        // 1.5 from page zoom, GDK's factor 1: nothing to undo.
        let native = region(css.0 * 1.5, css.1 * 1.5, css.2 * 1.5, css.3 * 1.5);
        assert_eq!(native.pixels_over(1.0), (432, 444, 768, 180));

        // 2 from GDK, which multiplies by 2 again: the page's own numbers come back.
        let native = region(css.0 * 2.0, css.1 * 2.0, css.2 * 2.0, css.3 * 2.0);
        assert_eq!(native.pixels_over(2.0), (288, 296, 512, 120));

        // Windows re-applies nothing, so the native rectangle passes through.
        assert_eq!(native.pixels(), (576, 592, 1024, 240));
    }

    /// A divisor below one, or not a number at all, would grow the rectangle instead of shrinking
    /// it. `scale_factor()` cannot return those today; this is here so it stays that way.
    #[test]
    fn a_nonsense_divisor_is_ignored_rather_than_applied() {
        let native = region(100.0, 100.0, 200.0, 200.0);
        for divisor in [0.0, -2.0, 0.5, f64::NAN] {
            assert_eq!(native.pixels_over(divisor), native.pixels(), "{divisor}");
        }
    }
    /// Positions round halves away from zero, as Rust's `f64::round` does. The width is 2 and not
    /// 3 because the size is the distance between the rounded edges, which is the next test.
    #[test]
    fn halves_round_away_from_zero() {
        assert_eq!(region(0.5, -0.5, 2.5, 3.5).pixels(), (1, -1, 2, 4));
    }

    /// The size comes from the rounded edges, never from rounding the length on its own: at
    /// `GDK_SCALE=2` a stage the page reports as x 577 width 1025 has edges at 288.5 and 801, so
    /// the surface is 512 wide. Rounding the length by itself gives 513 and hangs a pixel past the
    /// stage's right edge — the invariant `VideoStage.tsx` states and this side has to keep too.
    #[test]
    fn a_size_never_overshoots_the_edges_it_came_from() {
        assert_eq!(
            region(577.0, 333.0, 1025.0, 181.0).pixels_over(2.0),
            (289, 167, 512, 90)
        );
    }

    /// X11 window coordinates are 16 bit. A rectangle past that limit is clamped rather than cast,
    /// because casting wraps and a wrapped coordinate puts the video somewhere nobody asked for.
    #[test]
    fn coordinates_clamp_to_the_x11_limit() {
        let huge = COORD_LIMIT * 4.0;
        let (x, y, width, height) = region(-huge, huge, huge, huge).pixels();
        assert_eq!(
            (x, y, width, height),
            (
                -(COORD_LIMIT as i32),
                COORD_LIMIT as i32,
                COORD_LIMIT as i32,
                COORD_LIMIT as i32
            )
        );
    }

    /// Zero is not a size any window API accepts, and an empty region hides the surface instead of
    /// reaching here, so the floor is one rather than zero.
    #[test]
    fn sizes_never_reach_the_window_api_as_zero() {
        let (_, _, width, height) = region(0.0, 0.0, 0.4, 0.0).pixels();
        assert_eq!((width, height), (1, 1));
    }

    #[test]
    fn an_empty_region_is_recognised_before_it_is_converted() {
        assert!(region(0.0, 0.0, 0.0, 100.0).is_empty());
        assert!(region(0.0, 0.0, 100.0, 0.0).is_empty());
        assert!(region(0.0, 0.0, f64::NAN, 100.0).is_empty());
        assert!(!region(0.0, 0.0, 1.0, 1.0).is_empty());
    }
}
