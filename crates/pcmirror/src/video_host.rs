// SPDX-License-Identifier: GPL-3.0-or-later
//! Natives Host-Fenster für das Video. UxPlay legt darin sein eigenes
//! D3D11-Kindfenster an und leitet Maus-/Tastaturereignisse hierher weiter.
use crate::events::{self, Event};
use crate::win::{hwnd, RawHwnd};
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetStockObject, BLACK_BRUSH, HBRUSH};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

pub struct VideoHost {
    handle: RawHwnd,
}

impl VideoHost {
    pub fn create(parent: RawHwnd) -> Option<Self> {
        unsafe {
            let instance = GetModuleHandleW(None).ok()?;
            let class = WNDCLASSW {
                style: CS_DBLCLKS,
                lpfnWndProc: Some(procedure),
                hInstance: instance.into(),
                hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                lpszClassName: w!("PCMirrorVideoHost"),
                ..Default::default()
            };
            RegisterClassW(&class); // mehrfach registrieren ist harmlos
            let window = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("PCMirrorVideoHost"),
                w!("PC-Mirror video host"),
                WS_CHILD | WS_CLIPSIBLINGS | WS_CLIPCHILDREN,
                0,
                0,
                1,
                1,
                Some(hwnd(parent)),
                None,
                Some(instance.into()),
                None,
            )
            .ok()?;
            Some(Self {
                handle: window.0 as RawHwnd,
            })
        }
    }

    pub fn raw(&self) -> RawHwnd {
        self.handle
    }

    /// Position in physischen Client-Koordinaten des Hauptfensters.
    pub fn place(&self, x: i32, y: i32, width: i32, height: i32, visible: bool) {
        unsafe {
            let flags = SWP_NOACTIVATE
                | SWP_NOZORDER
                | if visible {
                    SWP_SHOWWINDOW
                } else {
                    SWP_HIDEWINDOW
                };
            let _ = SetWindowPos(
                hwnd(self.handle),
                None,
                x,
                y,
                width.max(1),
                height.max(1),
                flags,
            );
        }
    }
}

impl Drop for VideoHost {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(hwnd(self.handle));
        }
    }
}

unsafe extern "system" fn procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_LBUTTONDBLCLK => events::post(Event::VideoDoubleClick),
        WM_MOUSEMOVE => events::post(Event::VideoPointer),
        WM_KEYDOWN => events::post(Event::VideoKey {
            key: wparam.0 as u32,
        }),
        _ => {}
    }
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}
