//! Funciones de ventana que GPUI no expone de forma portable.

use gpui::Window;

/// Pone o quita la ventana por encima de todas las demás.
#[cfg(windows)]
pub fn set_always_on_top(window: &Window, on_top: bool) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
    };

    // `Window` tiene su propio `window_handle()`; aquí se quiere el de raw-window-handle.
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    let hwnd = handle.hwnd.get() as *mut core::ffi::c_void;
    let after = if on_top { HWND_TOPMOST } else { HWND_NOTOPMOST };
    // SAFETY: `hwnd` es la ventana viva de GPUI; SetWindowPos solo cambia su orden Z.
    unsafe {
        SetWindowPos(
            hwnd,
            after,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

#[cfg(not(windows))]
pub fn set_always_on_top(_window: &Window, _on_top: bool) {}
