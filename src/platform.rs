//! Funciones de Windows que GPUI no expone de forma portable.

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

/// Manda un archivo a la papelera de reciclaje, para poder recuperarlo.
#[cfg(windows)]
pub fn move_to_trash(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::{
        FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, SHFILEOPSTRUCTW,
        SHFileOperationW,
    };

    let path = std::path::absolute(path)?;
    // `pFrom` es una lista de rutas terminada en dos ceros.
    let from: Vec<u16> = path.as_os_str().encode_wide().chain([0, 0]).collect();
    let mut op = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: from.as_ptr(),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT) as u16,
        ..Default::default()
    };
    // SAFETY: `from` vive hasta el final de la llamada y termina en dos ceros.
    let code = unsafe { SHFileOperationW(&mut op) };
    if code != 0 || op.fAnyOperationsAborted != 0 {
        return Err(std::io::Error::other(format!(
            "no se pudo mover a la papelera (código {code})"
        )));
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn move_to_trash(path: &std::path::Path) -> std::io::Result<()> {
    std::fs::remove_file(path)
}
