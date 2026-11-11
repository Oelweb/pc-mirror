// SPDX-License-Identifier: GPL-3.0-or-later
//! Natives Video-Kindfenster im Fenster der App (`PCMIRROR_HWND`).
//! Maus- und Tastaturereignisse gehen an das Elternfenster; die Größe folgt dem
//! Elternfenster (beim iPad optional „Fenster ausfüllen“).
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{MapWindowPoints, HBRUSH};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::WindowsAndMessaging::*;

static PARENT: AtomicIsize = AtomicIsize::new(0);
static CHILD: AtomicIsize = AtomicIsize::new(0);
static FILL: AtomicBool = AtomicBool::new(false);
static IPAD: AtomicBool = AtomicBool::new(false);
const TIMER_LAYOUT: usize = 1;

pub fn set_fill(fill: bool) {
    FILL.store(fill, Ordering::SeqCst);
}

pub fn set_ipad(ipad: bool) {
    IPAD.store(ipad, Ordering::SeqCst);
}

pub fn is_ipad_model(model: &str) -> bool {
    model.starts_with("iPad")
}

/// Bildausschnitt wie `pcmirror_layout.h`: beim iPad mit „ausfüllen“ wird das
/// Bild so vergrößert, dass es das Fenster bedeckt (Ränder werden abgeschnitten).
pub fn viewport(width: i32, height: i32, fill: bool, ipad: bool, source_w: i64, source_h: i64) -> (i32, i32, i32, i32) {
    let fit = (0, 0, width, height);
    if !fill || !ipad || width <= 0 || height <= 0 || source_w <= 0 || source_h <= 0
        || source_w > 1_000_000_000 || source_h > 1_000_000_000
    {
        return fit;
    }
    let w = (height as i64 * source_w + source_h - 1) / source_h;
    let h = (width as i64 * source_h + source_w - 1) / source_w;
    if w > 32767 || h > 32767 {
        return fit;
    }
    let covered_w = (w as i32).max(width);
    let covered_h = (h as i32).max(height);
    ((width - covered_w) / 2, (height - covered_h) / 2, covered_w, covered_h)
}

unsafe extern "system" fn window_proc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let parent = HWND(PARENT.load(Ordering::SeqCst) as *mut _);
    match message {
        WM_TIMER if wparam.0 == TIMER_LAYOUT => {
            layout(window, parent);
            LRESULT(0)
        }
        WM_MOUSEMOVE | WM_LBUTTONDBLCLK | WM_KEYDOWN | WM_KEYUP => {
            let mut lparam = lparam;
            if message == WM_MOUSEMOVE || message == WM_LBUTTONDBLCLK {
                let mut point = [POINT {
                    x: (lparam.0 & 0xffff) as i16 as i32,
                    y: ((lparam.0 >> 16) & 0xffff) as i16 as i32,
                }];
                MapWindowPoints(Some(window), Some(parent), &mut point);
                lparam = LPARAM(((point[0].y as u16 as isize) << 16) | point[0].x as u16 as isize);
            }
            let _ = PostMessageW(Some(parent), message, wparam, lparam);
            DefWindowProcW(window, message, wparam, lparam)
        }
        _ => DefWindowProcW(window, message, wparam, lparam),
    }
}

unsafe fn layout(child: HWND, parent: HWND) {
    let mut parent_rect = RECT::default();
    if GetClientRect(parent, &mut parent_rect).is_err() {
        return;
    }
    let mut child_rect = RECT::default();
    let _ = GetWindowRect(child, &mut child_rect);
    let mut points = [
        POINT { x: child_rect.left, y: child_rect.top },
        POINT { x: child_rect.right, y: child_rect.bottom },
    ];
    MapWindowPoints(None, Some(parent), &mut points);
    let (source_w, source_h) = if FILL.load(Ordering::SeqCst) && IPAD.load(Ordering::SeqCst) {
        crate::gst::last_frame_size().map(|(w, h)| (w as i64, h as i64)).unwrap_or((0, 0))
    } else {
        (0, 0)
    };
    let (x, y, w, h) = viewport(
        parent_rect.right,
        parent_rect.bottom,
        FILL.load(Ordering::SeqCst),
        IPAD.load(Ordering::SeqCst),
        source_w,
        source_h,
    );
    if parent_rect.right > 0
        && parent_rect.bottom > 0
        && (points[0].x != x || points[0].y != y || points[1].x - points[0].x != w || points[1].y - points[0].y != h)
    {
        let _ = SetWindowPos(child, None, x, y, w, h, SWP_NOACTIVATE | SWP_NOZORDER);
    }
}

/// Erstellt das Kindfenster in einem eigenen Thread mit Nachrichtenschleife und
/// gibt dessen Handle zurück (0, wenn kein Elternfenster angegeben ist).
pub fn create_from_env() -> usize {
    let Some(parent) = std::env::var("PCMIRROR_HWND").ok().and_then(|v| v.trim().parse::<isize>().ok()) else {
        return 0;
    };
    if parent == 0 {
        return 0;
    }
    PARENT.store(parent, Ordering::SeqCst);
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let instance = GetModuleHandleW(None).unwrap_or_default();
        let class = WNDCLASSW {
            style: CS_DBLCLKS,
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: w!("PCMirrorVideoChild"),
            hbrBackground: HBRUSH(stock_black()),
            ..Default::default()
        };
        RegisterClassW(&class);
        let parent_hwnd = HWND(parent as *mut _);
        let mut rect = RECT::default();
        let _ = GetClientRect(parent_hwnd, &mut rect);
        let child = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("PCMirrorVideoChild"),
            w!("PC-Mirror video"),
            WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | WS_CLIPCHILDREN,
            0,
            0,
            rect.right,
            rect.bottom,
            Some(parent_hwnd),
            None,
            Some(instance.into()),
            None,
        );
        let Ok(child) = child else {
            eprintln!("PC-Mirror could not create native video child");
            let _ = sender.send(0usize);
            return;
        };
        CHILD.store(child.0 as isize, Ordering::SeqCst);
        SetTimer(Some(child), TIMER_LAYOUT, 50, None);
        let _ = sender.send(child.0 as usize);
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    });
    receiver.recv().unwrap_or(0)
}

/// Schwarzer Hintergrund für Bereiche ohne Bild.
unsafe fn stock_black() -> *mut core::ffi::c_void {
    windows::Win32::Graphics::Gdi::GetStockObject(windows::Win32::Graphics::Gdi::BLACK_BRUSH).0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_matches_c_layout() {
        assert_eq!(viewport(800, 600, false, true, 1668, 2388), (0, 0, 800, 600));
        assert_eq!(viewport(800, 600, true, false, 1668, 2388), (0, 0, 800, 600));
        // iPad hochkant in Querfenster: Breite füllt, Höhe wird abgeschnitten
        let (x, y, w, h) = viewport(800, 600, true, true, 1668, 2388);
        assert_eq!((x, w), (0, 800));
        assert!(h > 600 && y < 0);
        assert_eq!(viewport(800, 600, true, true, 0, 0), (0, 0, 800, 600));
    }
}
