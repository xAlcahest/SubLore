//! A `STATIC` child HWND of the Tauri window. WebView2's host window is a sibling created earlier,
//! so this one sits above it, and every region update re-asserts that with SetWindowPos.

use windows::core::w;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, SetWindowPos, ShowWindow, HWND_TOP, SWP_NOACTIVATE, SW_HIDE,
    SW_SHOWNA, WINDOW_EX_STYLE, WS_CHILD, WS_CLIPSIBLINGS,
};

use super::SurfaceRegion;
use crate::video::error::VideoError;

pub struct Surface {
    hwnd: HWND,
}

pub fn create(window: &tauri::WebviewWindow) -> Result<Surface, VideoError> {
    let parent = window
        .hwnd()
        .map_err(|error| VideoError::player_unavailable(format!("no window handle: {error}")))?;

    // SAFETY: STATIC is a predefined class, so there is no class to register and no window
    // procedure to write. Called on the main thread during setup.
    let hwnd = unsafe {
        let instance = GetModuleHandleW(None).map_err(|error| {
            VideoError::player_unavailable(format!("GetModuleHandleW: {error}"))
        })?;
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("STATIC"),
            w!(""),
            WS_CHILD | WS_CLIPSIBLINGS,
            0,
            0,
            1,
            1,
            Some(parent),
            None,
            Some(instance.into()),
            None,
        )
        .map_err(|error| {
            VideoError::player_unavailable(format!("could not create the video surface: {error}"))
        })?
    };

    Ok(Surface { hwnd })
}

impl Surface {
    pub fn wid(&self) -> i64 {
        self.hwnd.0 as i64
    }

    pub fn set_region(&self, region: SurfaceRegion) -> Result<(), VideoError> {
        // Already native pixels, resolved by the page. See SurfaceRegion.
        let (x, y, width, height) = region.pixels();
        // SAFETY: our own child window, main thread only.
        unsafe {
            SetWindowPos(
                self.hwnd,
                Some(HWND_TOP),
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE,
            )
            .map_err(|error| VideoError::command_failed(format!("SetWindowPos: {error}")))
        }
    }

    pub fn show(&self) -> Result<(), VideoError> {
        // SAFETY: our own child window, main thread only. SHOWNA keeps focus in the webview.
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOWNA);
        }
        Ok(())
    }

    pub fn hide(&self) -> Result<(), VideoError> {
        // SAFETY: our own child window, main thread only.
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
        Ok(())
    }

    pub fn destroy(self) -> Result<(), VideoError> {
        // SAFETY: our own child window, destroyed on the main thread after mpv is gone.
        unsafe {
            DestroyWindow(self.hwnd)
                .map_err(|error| VideoError::command_failed(format!("DestroyWindow: {error}")))
        }
    }
}
