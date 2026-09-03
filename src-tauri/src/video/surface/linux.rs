//! An X11 child window of the Tauri toplevel. X11 subwindows stack above the parent's own
//! drawing, which is what puts the video over the WebKitGTK webview.

use gtk::gdk;
use gtk::glib::prelude::*;
use gtk::prelude::*;

use super::SurfaceRegion;
use crate::video::error::VideoError;

pub struct Surface {
    window: gdk::Window,
    xid: i64,
}

pub fn create(window: &tauri::WebviewWindow) -> Result<Surface, VideoError> {
    let gtk_window = window
        .gtk_window()
        .map_err(|error| VideoError::player_unavailable(format!("no GTK window: {error}")))?;
    if !gtk_window.is_realized() {
        gtk_window.realize();
    }
    let parent = gtk_window
        .window()
        .ok_or_else(|| VideoError::player_unavailable("the GTK window has no GDK window"))?;

    let attributes = gdk::WindowAttr {
        window_type: gdk::WindowType::Child,
        wclass: gdk::WindowWindowClass::InputOutput,
        x: Some(0),
        y: Some(0),
        width: 1,
        height: 1,
        event_mask: gdk::EventMask::EXPOSURE_MASK,
        ..Default::default()
    };
    let child = gdk::Window::new(Some(&parent), &attributes);
    // GTK3 child windows are client side by default; mpv needs a real X11 window id.
    if !child.ensure_native() {
        return Err(VideoError::player_unavailable(
            "GDK refused a native child window; is the app running on X11?",
        ));
    }
    child.set_pass_through(false);

    let x11 = child
        .clone()
        .downcast::<gdkx11::X11Window>()
        .map_err(|_| VideoError::player_unavailable("the video surface is not an X11 window"))?;

    Ok(Surface {
        window: child,
        xid: x11.xid() as i64,
    })
}

impl Surface {
    pub fn wid(&self) -> i64 {
        self.xid
    }

    pub fn set_region(&self, region: SurfaceRegion) -> Result<(), VideoError> {
        // GDK multiplies child geometry by this factor on the way to X, so it comes out here
        // first: without that an integer scale lands twice. See BACKLOG N2c.
        let (x, y, width, height) = region.pixels_over(f64::from(self.window.scale_factor()));
        self.window.move_resize(x, y, width, height);
        self.window.raise();
        Ok(())
    }

    pub fn show(&self) -> Result<(), VideoError> {
        self.window.show();
        self.window.raise();
        Ok(())
    }

    pub fn hide(&self) -> Result<(), VideoError> {
        self.window.hide();
        Ok(())
    }

    pub fn destroy(self) -> Result<(), VideoError> {
        self.window.destroy();
        // The X window goes; the GObject wrapping it deliberately does not. GDK walks its own
        // window list while the app is quitting and type-checks what it finds, and a freed one
        // makes that read a class pointer that is no longer mapped. See BACKLOG.md N11.
        std::mem::forget(self);
        Ok(())
    }
}
