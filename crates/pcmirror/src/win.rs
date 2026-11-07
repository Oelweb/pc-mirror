// SPDX-License-Identifier: GPL-3.0-or-later
//! Dünne, sichere Hüllen um die benötigten Win32-Aufrufe.
use std::ffi::c_void;
use std::path::Path;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND, POINT, RECT,
};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWINDOWATTRIBUTE};
use windows::Win32::Graphics::Gdi::{MonitorFromRect, MONITOR_DEFAULTTONULL};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
};
use windows::Win32::System::Threading::{
    CreateMutexW, OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
};
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::*;

/// Fensterhandle als Zahl: `HWND` selbst ist nicht `Send`.
pub type RawHwnd = isize;

pub fn hwnd(raw: RawHwnd) -> HWND {
    HWND(raw as *mut c_void)
}

/// Liefert das native Fenster eines (bereits sichtbaren) Slint-Fensters.
pub fn hwnd_of(window: &slint::Window) -> Option<RawHwnd> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = window.window_handle();
    let raw = handle.window_handle().ok()?.as_raw();
    match raw {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
        _ => None,
    }
}

/// Dunkle Titelleiste, abgerundete Ecken und Mica wie bei der früheren Qt-Version.
pub fn style_window(raw: RawHwnd) {
    let handle = hwnd(raw);
    let set = |attribute: i32, value: u32| unsafe {
        let _ = DwmSetWindowAttribute(
            handle,
            DWMWINDOWATTRIBUTE(attribute),
            &value as *const u32 as *const c_void,
            std::mem::size_of::<u32>() as u32,
        );
    };
    set(20, 1); // DWMWA_USE_IMMERSIVE_DARK_MODE
    set(35, rgb(28, 28, 28)); // DWMWA_CAPTION_COLOR
    set(36, rgb(255, 255, 255)); // DWMWA_TEXT_COLOR
    set(34, rgb(51, 51, 51)); // DWMWA_BORDER_COLOR
    set(33, 2); // DWMWA_WINDOW_CORNER_PREFERENCE = round
}

fn rgb(r: u8, g: u8, b: u8) -> u32 {
    u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16)
}

pub fn set_topmost(raw: RawHwnd, topmost: bool) {
    let after = if topmost {
        HWND_TOPMOST
    } else {
        HWND_NOTOPMOST
    };
    unsafe {
        let _ = SetWindowPos(
            hwnd(raw),
            Some(after),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

/// Gehört `child` zu `owner`, bleibt es über dem Besitzer und minimiert mit ihm.
pub fn set_owner(child: RawHwnd, owner: RawHwnd) {
    unsafe {
        SetWindowLongPtrW(hwnd(child), GWLP_HWNDPARENT, owner);
    }
}

/// Schwebende Werkzeugleiste: kein Taskleisteneintrag, nimmt keinen Fokus.
pub fn make_tool_window(raw: RawHwnd) {
    unsafe {
        let style = GetWindowLongPtrW(hwnd(raw), GWL_EXSTYLE);
        let tool = (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) as isize;
        let app = WS_EX_APPWINDOW.0 as isize;
        let layered = WS_EX_LAYERED.0 as isize;
        SetWindowLongPtrW(hwnd(raw), GWL_EXSTYLE, (style | tool) & !app & !layered);
        // Rahmenlose Fenster bekommen sonst DWM-Transparenz; der Software-Renderer
        // liefert keinen Alphakanal, der Inhalt wäre unsichtbar.
        let margins = windows::Win32::UI::Controls::MARGINS::default();
        let _ = windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea(hwnd(raw), &margins);
        let _ = SetWindowPos(
            hwnd(raw),
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

/// Zeigt ein Fenster an der Position, ohne es zu aktivieren, und hält es über dem Besitzer.
pub fn show_at(raw: RawHwnd, x: i32, y: i32) {
    unsafe {
        let _ = SetWindowPos(
            hwnd(raw),
            Some(HWND_TOP),
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

/// Parkt ein Fenster außerhalb des Bildschirms. Für Slint bleibt es sichtbar und
/// wird weiter gezeichnet; ein natives Verstecken würde das Rendern anhalten.
pub fn park(raw: RawHwnd) {
    unsafe {
        let _ = SetWindowPos(
            hwnd(raw),
            None,
            -32000,
            -32000,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
        );
    }
}

pub fn hide(raw: RawHwnd) {
    unsafe {
        let _ = ShowWindow(hwnd(raw), SW_HIDE);
    }
}

pub fn enable_window(raw: RawHwnd, enabled: bool) {
    unsafe {
        let _ = EnableWindow(hwnd(raw), enabled);
    }
}

pub fn bring_to_front(raw: RawHwnd) {
    unsafe {
        let _ = SetForegroundWindow(hwnd(raw));
    }
}

pub fn is_minimized(raw: RawHwnd) -> bool {
    unsafe { IsIconic(hwnd(raw)).as_bool() }
}

pub fn is_visible(raw: RawHwnd) -> bool {
    unsafe { IsWindowVisible(hwnd(raw)).as_bool() }
}

/// Fensterrechteck in physischen Bildschirmkoordinaten.
pub fn window_rect(raw: RawHwnd) -> Option<RECT> {
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd(raw), &mut rect).ok()? };
    Some(rect)
}

/// Client-Ursprung in Bildschirmkoordinaten.
pub fn client_origin(raw: RawHwnd) -> (i32, i32) {
    let mut point = POINT::default();
    unsafe {
        let _ = windows::Win32::Graphics::Gdi::ClientToScreen(hwnd(raw), &mut point);
    }
    (point.x, point.y)
}

pub fn cursor_pos() -> (i32, i32) {
    let mut point = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut point);
    }
    (point.x, point.y)
}

/// Liegt das Rechteck zumindest teilweise auf einem angeschlossenen Monitor?
pub fn on_any_monitor(x: i32, y: i32, width: u32, height: u32) -> bool {
    let rect = RECT {
        left: x,
        top: y,
        right: x + width as i32,
        bottom: y + height as i32,
    };
    unsafe { !MonitorFromRect(&rect, MONITOR_DEFAULTTONULL).is_invalid() }
}

pub fn message_box(text: &str, error: bool) {
    let style = if error {
        MB_ICONERROR
    } else {
        MB_ICONINFORMATION
    };
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(text),
            &HSTRING::from("PC-Mirror"),
            style | MB_OK,
        );
    }
}

/// Öffnet eine Windows-Adresse wie `ms-settings:network-status`.
pub fn open_uri(uri: &str) -> bool {
    let result = unsafe {
        ShellExecuteW(
            None,
            &HSTRING::from("open"),
            &HSTRING::from(uri),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    result.0 as isize > 32
}

pub fn open_folder(path: &Path) -> bool {
    let result = unsafe {
        ShellExecuteW(
            None,
            &HSTRING::from("open"),
            &HSTRING::from(path.as_os_str()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    result.0 as isize > 32
}

/// Benanntes Mutex statt Lockdatei: verschwindet zuverlässig mit dem Prozess.
pub struct SingleInstance(HANDLE);

impl SingleInstance {
    pub fn acquire(name: &str) -> Option<Self> {
        let handle = unsafe { CreateMutexW(None, true, &HSTRING::from(name)).ok()? };
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            unsafe {
                let _ = CloseHandle(handle);
            }
            return None;
        }
        Some(Self(handle))
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Windows-Job: beendet den Empfänger, sobald die App endet – auch bei Absturz.
pub struct Job(HANDLE);
unsafe impl Send for Job {}

impl Job {
    pub fn new() -> Option<Self> {
        unsafe {
            let job = CreateJobObjectW(None, PCWSTR::null()).ok()?;
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if ok.is_err() {
                let _ = CloseHandle(job);
                return None;
            }
            Some(Self(job))
        }
    }

    pub fn assign(&self, pid: u32) -> Result<(), String> {
        unsafe {
            let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false, pid)
                .map_err(|e| e.to_string())?;
            let result = AssignProcessToJobObject(self.0, process).map_err(|e| e.to_string());
            let _ = CloseHandle(process);
            result
        }
    }

    pub fn terminate(&self) {
        unsafe {
            let _ = TerminateJobObject(self.0, 1);
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

// --- Registry (HKCU) -------------------------------------------------------

pub fn registry_get_string(key: &str, name: &str) -> Option<String> {
    let mut buffer = vec![0u16; 2048];
    let mut size = (buffer.len() * 2) as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(key),
            &HSTRING::from(name),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr() as *mut c_void),
            Some(&mut size),
        )
    };
    if result.is_err() {
        return None;
    }
    let length = (size as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buffer[..length]))
}

fn open_writable(key: &str) -> Result<HKEY, String> {
    let mut handle = HKEY::default();
    unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(key),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut handle,
            None,
        )
        .ok()
        .map_err(|e| e.to_string())?;
    }
    Ok(handle)
}

pub fn registry_set_string(key: &str, name: &str, value: &str) -> Result<(), String> {
    let handle = open_writable(key)?;
    let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = unsafe { std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2) };
    let result = unsafe { RegSetValueExW(handle, &HSTRING::from(name), None, REG_SZ, Some(bytes)) };
    unsafe {
        let _ = RegCloseKey(handle);
    }
    result.ok().map_err(|e| e.to_string())
}

pub fn registry_delete_value(key: &str, name: &str) -> Result<(), String> {
    let handle = open_writable(key)?;
    let result = unsafe { RegDeleteValueW(handle, &HSTRING::from(name)) };
    unsafe {
        let _ = RegCloseKey(handle);
    }
    // Nicht vorhanden ist ebenfalls „aus“.
    if result.is_ok() || result == windows::Win32::Foundation::ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        Err(format!("{result:?}"))
    }
}
