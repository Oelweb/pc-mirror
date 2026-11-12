// SPDX-License-Identifier: GPL-3.0-or-later
//! Schlanke Anbindung an GStreamer zur Laufzeit (die DLLs liegen neben dem Programm).
//! Es werden nur stabile C-Funktionen und Strukturanfänge genutzt, deren Aufbau
//! seit GStreamer 1.0 unverändert ist.
#![allow(non_camel_case_types)]

use std::ffi::{c_char, c_int, c_uint, c_void, CStr, CString};
use std::sync::OnceLock;

pub type GstElement = c_void;
pub type GstBuffer = c_void;
pub type GstCaps = c_void;
pub type GstBus = c_void;
pub type GstMessage = c_void;
pub type GstPad = c_void;
pub type GstSample = c_void;
type gboolean = c_int;

pub const GST_STATE_NULL: c_int = 1;
pub const GST_STATE_READY: c_int = 2;
pub const GST_STATE_PLAYING: c_int = 4;
const GST_MESSAGE_ERROR: c_uint = 1 << 1;
const GST_MAP_READ: c_uint = 1;
const GST_PAD_PROBE_TYPE_BUFFER: c_uint = 1 << 4;
const GST_PAD_PROBE_OK: c_int = 1;
const GST_BUS_PASS: c_int = 1;
const GST_BUS_DROP: c_int = 0;
pub const GST_SECOND: u64 = 1_000_000_000;

#[repr(C)]
pub struct GstMapInfo {
    memory: *mut c_void,
    flags: c_uint,
    pub data: *mut u8,
    pub size: usize,
    maxsize: usize,
    user_data: [*mut c_void; 4],
    reserved: [*mut c_void; 4],
}

#[repr(C)]
pub struct GError {
    domain: u32,
    code: c_int,
    message: *const c_char,
}

/// Anfang von `GstPadProbeInfo`: `type` (enum), `id` (gulong, unter Windows 32 Bit), `data`.
#[repr(C)]
pub struct GstPadProbeInfo {
    kind: c_uint,
    id: std::ffi::c_ulong,
    data: *mut c_void,
}

type ProbeCallback = unsafe extern "C" fn(*mut GstPad, *mut GstPadProbeInfo, *mut c_void) -> c_int;
type BusSyncHandler = unsafe extern "C" fn(*mut GstBus, *mut GstMessage, *mut c_void) -> c_int;

/// Alle benötigten GStreamer-/GLib-Funktionen.
pub struct Api {
    _libs: Vec<libloading::Library>,
    pub gst_init: unsafe extern "C" fn(*mut c_int, *mut c_void),
    pub gst_parse_launch: unsafe extern "C" fn(*const c_char, *mut *mut GError) -> *mut GstElement,
    pub gst_bin_get_by_name: unsafe extern "C" fn(*mut GstElement, *const c_char) -> *mut GstElement,
    pub gst_element_set_state: unsafe extern "C" fn(*mut GstElement, c_int) -> c_int,
    pub gst_object_unref: unsafe extern "C" fn(*mut c_void),
    pub gst_caps_from_string: unsafe extern "C" fn(*const c_char) -> *mut GstCaps,
    pub gst_mini_object_unref: unsafe extern "C" fn(*mut c_void),
    pub gst_mini_object_ref: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    pub gst_buffer_new_allocate: unsafe extern "C" fn(*mut c_void, usize, *mut c_void) -> *mut GstBuffer,
    pub gst_buffer_fill: unsafe extern "C" fn(*mut GstBuffer, usize, *const c_void, usize) -> usize,
    pub gst_buffer_map: unsafe extern "C" fn(*mut GstBuffer, *mut GstMapInfo, c_uint) -> gboolean,
    pub gst_buffer_unmap: unsafe extern "C" fn(*mut GstBuffer, *mut GstMapInfo),
    pub gst_element_get_bus: unsafe extern "C" fn(*mut GstElement) -> *mut GstBus,
    pub gst_bus_set_sync_handler: unsafe extern "C" fn(*mut GstBus, Option<BusSyncHandler>, *mut c_void, *mut c_void),
    pub gst_bus_timed_pop_filtered: unsafe extern "C" fn(*mut GstBus, u64, c_uint) -> *mut GstMessage,
    pub gst_message_parse_error: unsafe extern "C" fn(*mut GstMessage, *mut *mut GError, *mut *mut c_char),
    pub gst_element_get_static_pad: unsafe extern "C" fn(*mut GstElement, *const c_char) -> *mut GstPad,
    pub gst_pad_add_probe: unsafe extern "C" fn(*mut GstPad, c_uint, Option<ProbeCallback>, *mut c_void, *mut c_void) -> std::ffi::c_ulong,
    pub gst_pad_get_current_caps: unsafe extern "C" fn(*mut GstPad) -> *mut GstCaps,
    pub gst_caps_get_structure: unsafe extern "C" fn(*mut GstCaps, c_uint) -> *mut c_void,
    pub gst_structure_get_int: unsafe extern "C" fn(*mut c_void, *const c_char, *mut c_int) -> gboolean,
    pub gst_sample_new: unsafe extern "C" fn(*mut GstBuffer, *mut GstCaps, *mut c_void, *mut c_void) -> *mut GstSample,
    pub gst_sample_get_buffer: unsafe extern "C" fn(*mut GstSample) -> *mut GstBuffer,
    pub gst_sample_get_caps: unsafe extern "C" fn(*mut GstSample) -> *mut GstCaps,
    pub gst_element_factory_find: unsafe extern "C" fn(*const c_char) -> *mut c_void,
    pub gst_app_src_push_buffer: unsafe extern "C" fn(*mut GstElement, *mut GstBuffer) -> c_int,
    pub gst_app_src_end_of_stream: unsafe extern "C" fn(*mut GstElement) -> c_int,
    pub gst_is_video_overlay_prepare_window_handle_message: unsafe extern "C" fn(*mut GstMessage) -> gboolean,
    pub gst_video_overlay_set_window_handle: unsafe extern "C" fn(*mut GstElement, usize),
    pub gst_video_overlay_handle_events: unsafe extern "C" fn(*mut GstElement, gboolean),
    pub gst_video_convert_sample: unsafe extern "C" fn(*mut GstSample, *mut GstCaps, u64, *mut *mut GError) -> *mut GstSample,
    pub g_error_free: unsafe extern "C" fn(*mut GError),
    pub g_free: unsafe extern "C" fn(*mut c_void),
    pub g_object_set: unsafe extern "C" fn(*mut c_void, *const c_char, ...),
    pub gst_element_get_clock: unsafe extern "C" fn(*mut GstElement) -> *mut c_void,
    pub gst_clock_get_time: unsafe extern "C" fn(*mut c_void) -> u64,
    pub gst_element_get_base_time: unsafe extern "C" fn(*mut GstElement) -> u64,
}

/// Sucht ein Symbol in allen geladenen Bibliotheken.
unsafe fn sym<T: Copy>(libs: &[libloading::Library], name: &str) -> Result<T, String> {
    let symbol = format!("{name}\0");
    for lib in libs {
        if let Ok(found) = lib.get::<T>(symbol.as_bytes()) {
            return Ok(*found);
        }
    }
    Err(format!("GStreamer-Funktion {name} fehlt"))
}

impl Api {
    unsafe fn load() -> Result<Self, String> {
        let mut libs = Vec::new();
        for name in [
            "libgstreamer-1.0-0.dll",
            "libgobject-2.0-0.dll",
            "libglib-2.0-0.dll",
            "libgstapp-1.0-0.dll",
            "libgstvideo-1.0-0.dll",
        ] {
            libs.push(libloading::Library::new(name).map_err(|e| format!("{name}: {e}"))?);
        }
        Ok(Self {
            gst_init: sym(&libs, "gst_init")?,
            gst_parse_launch: sym(&libs, "gst_parse_launch")?,
            gst_bin_get_by_name: sym(&libs, "gst_bin_get_by_name")?,
            gst_element_set_state: sym(&libs, "gst_element_set_state")?,
            gst_object_unref: sym(&libs, "gst_object_unref")?,
            gst_caps_from_string: sym(&libs, "gst_caps_from_string")?,
            gst_mini_object_unref: sym(&libs, "gst_mini_object_unref")?,
            gst_mini_object_ref: sym(&libs, "gst_mini_object_ref")?,
            gst_buffer_new_allocate: sym(&libs, "gst_buffer_new_allocate")?,
            gst_buffer_fill: sym(&libs, "gst_buffer_fill")?,
            gst_buffer_map: sym(&libs, "gst_buffer_map")?,
            gst_buffer_unmap: sym(&libs, "gst_buffer_unmap")?,
            gst_element_get_bus: sym(&libs, "gst_element_get_bus")?,
            gst_bus_set_sync_handler: sym(&libs, "gst_bus_set_sync_handler")?,
            gst_bus_timed_pop_filtered: sym(&libs, "gst_bus_timed_pop_filtered")?,
            gst_message_parse_error: sym(&libs, "gst_message_parse_error")?,
            gst_element_get_static_pad: sym(&libs, "gst_element_get_static_pad")?,
            gst_pad_add_probe: sym(&libs, "gst_pad_add_probe")?,
            gst_pad_get_current_caps: sym(&libs, "gst_pad_get_current_caps")?,
            gst_caps_get_structure: sym(&libs, "gst_caps_get_structure")?,
            gst_structure_get_int: sym(&libs, "gst_structure_get_int")?,
            gst_sample_new: sym(&libs, "gst_sample_new")?,
            gst_sample_get_buffer: sym(&libs, "gst_sample_get_buffer")?,
            gst_sample_get_caps: sym(&libs, "gst_sample_get_caps")?,
            gst_element_factory_find: sym(&libs, "gst_element_factory_find")?,
            gst_app_src_push_buffer: sym(&libs, "gst_app_src_push_buffer")?,
            gst_app_src_end_of_stream: sym(&libs, "gst_app_src_end_of_stream")?,
            gst_is_video_overlay_prepare_window_handle_message: sym(&libs, "gst_is_video_overlay_prepare_window_handle_message")?,
            gst_video_overlay_set_window_handle: sym(&libs, "gst_video_overlay_set_window_handle")?,
            gst_video_overlay_handle_events: sym(&libs, "gst_video_overlay_handle_events")?,
            gst_video_convert_sample: sym(&libs, "gst_video_convert_sample")?,
            g_error_free: sym(&libs, "g_error_free")?,
            g_free: sym(&libs, "g_free")?,
            g_object_set: sym(&libs, "g_object_set")?,
            gst_element_get_clock: sym(&libs, "gst_element_get_clock")?,
            gst_clock_get_time: sym(&libs, "gst_clock_get_time")?,
            gst_element_get_base_time: sym(&libs, "gst_element_get_base_time")?,
            _libs: libs,
        })
    }
}

static API: OnceLock<Result<Api, String>> = OnceLock::new();

/// Lädt GStreamer einmalig und ruft `gst_init` auf.
pub fn api() -> Result<&'static Api, String> {
    API.get_or_init(|| unsafe {
        let api = Api::load()?;
        (api.gst_init)(std::ptr::null_mut(), std::ptr::null_mut());
        Ok(api)
    })
    .as_ref()
    .map_err(Clone::clone)
}

fn cstr(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap()
}

unsafe fn take_error(api: &Api, error: *mut GError) -> String {
    if error.is_null() {
        return "unbekannt".into();
    }
    let text = if (*error).message.is_null() {
        "unbekannt".into()
    } else {
        CStr::from_ptr((*error).message).to_string_lossy().into_owned()
    };
    (api.g_error_free)(error);
    text
}

/// Ist ein GStreamer-Element (Plugin) vorhanden?
pub fn has_element(name: &str) -> bool {
    let Ok(api) = api() else { return false };
    unsafe {
        let factory = (api.gst_element_factory_find)(cstr(name).as_ptr());
        if factory.is_null() {
            false
        } else {
            (api.gst_object_unref)(factory);
            true
        }
    }
}

/// Fenster, in das die Videosenke zeichnet (wird beim Bus-Sync-Handler gesetzt).
static WINDOW: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub fn set_window_handle(handle: usize) {
    WINDOW.store(handle, std::sync::atomic::Ordering::SeqCst);
}

unsafe extern "C" fn bus_sync_handler(_bus: *mut GstBus, message: *mut GstMessage, _data: *mut c_void) -> c_int {
    let Ok(api) = api() else { return GST_BUS_PASS };
    if (api.gst_is_video_overlay_prepare_window_handle_message)(message) != 0 {
        let handle = WINDOW.load(std::sync::atomic::Ordering::SeqCst);
        if handle != 0 {
            // GST_MESSAGE_SRC: GstMessage { GstMiniObject (64 Byte), type (4), Padding, timestamp (8), src }
            let src = *(message as *const u8).add(80).cast::<*mut GstElement>();
            (api.gst_video_overlay_set_window_handle)(src, handle);
            (api.gst_video_overlay_handle_events)(src, 0);
        }
        (api.gst_mini_object_unref)(message);
        return GST_BUS_DROP;
    }
    GST_BUS_PASS
}

/// Eine laufende Pipeline mit optionaler `appsrc`.
pub struct Pipeline {
    pipeline: *mut GstElement,
    appsrc: *mut GstElement,
    bus: *mut GstBus,
}

unsafe impl Send for Pipeline {}

impl Pipeline {
    pub fn launch(description: &str, appsrc_name: Option<&str>, caps: Option<&str>) -> Result<Self, String> {
        let api = api()?;
        unsafe {
            let mut error: *mut GError = std::ptr::null_mut();
            let pipeline = (api.gst_parse_launch)(cstr(description).as_ptr(), &mut error);
            if !error.is_null() || pipeline.is_null() {
                let message = take_error(api, error);
                if !pipeline.is_null() {
                    (api.gst_object_unref)(pipeline);
                }
                return Err(format!("Pipeline „{description}“: {message}"));
            }
            let bus = (api.gst_element_get_bus)(pipeline);
            if !bus.is_null() {
                (api.gst_bus_set_sync_handler)(bus, Some(bus_sync_handler), std::ptr::null_mut(), std::ptr::null_mut());
            }
            let mut appsrc = std::ptr::null_mut();
            if let Some(name) = appsrc_name {
                appsrc = (api.gst_bin_get_by_name)(pipeline, cstr(name).as_ptr());
                if appsrc.is_null() {
                    return Err(format!("appsrc {name} fehlt"));
                }
                if let Some(caps) = caps {
                    let caps_ptr = (api.gst_caps_from_string)(cstr(caps).as_ptr());
                    (api.g_object_set)(
                        appsrc,
                        c"caps".as_ptr(),
                        caps_ptr,
                        c"stream-type".as_ptr(),
                        0 as c_int,
                        c"is-live".as_ptr(),
                        1 as c_int,
                        c"format".as_ptr(),
                        3 as c_int, // GST_FORMAT_TIME
                        std::ptr::null::<c_char>(),
                    );
                    (api.gst_mini_object_unref)(caps_ptr);
                }
            }
            Ok(Self { pipeline, appsrc, bus })
        }
    }

    pub fn element(&self, name: &str) -> Option<Element> {
        let api = api().ok()?;
        let element = unsafe { (api.gst_bin_get_by_name)(self.pipeline, cstr(name).as_ptr()) };
        (!element.is_null()).then_some(Element(element))
    }

    pub fn set_state(&self, state: c_int) -> bool {
        let Ok(api) = api() else { return false };
        unsafe { (api.gst_element_set_state)(self.pipeline, state) != 0 }
    }

    pub fn push(&self, data: &[u8]) {
        self.push_at(data, None);
    }

    /// Aktuelle Laufzeit der Pipeline (Uhrzeit minus Basiszeit), sobald sie läuft.
    pub fn running_time(&self) -> Option<u64> {
        let api = api().ok()?;
        unsafe {
            let clock = (api.gst_element_get_clock)(self.pipeline);
            if clock.is_null() {
                return None;
            }
            let now = (api.gst_clock_get_time)(clock);
            (api.gst_object_unref)(clock);
            let base = (api.gst_element_get_base_time)(self.pipeline);
            (now != u64::MAX && base != u64::MAX).then(|| now.saturating_sub(base))
        }
    }

    /// Puffer mit optionalem Darstellungszeitpunkt (PTS, Laufzeit in ns) einspeisen.
    pub fn push_at(&self, data: &[u8], pts: Option<u64>) {
        let Ok(api) = api() else { return };
        if self.appsrc.is_null() || data.is_empty() {
            return;
        }
        unsafe {
            let buffer = (api.gst_buffer_new_allocate)(std::ptr::null_mut(), data.len(), std::ptr::null_mut());
            if buffer.is_null() {
                return;
            }
            (api.gst_buffer_fill)(buffer, 0, data.as_ptr().cast(), data.len());
            if let Some(pts) = pts {
                // GstBuffer { GstMiniObject (64 Byte), pool (8), pts, dts, duration, ... }
                *(buffer as *mut u8).add(72).cast::<u64>() = pts;
            }
            (api.gst_app_src_push_buffer)(self.appsrc, buffer); // übernimmt den Puffer
        }
    }

    pub fn end_of_stream(&self) {
        if let Ok(api) = api() {
            if !self.appsrc.is_null() {
                unsafe { (api.gst_app_src_end_of_stream)(self.appsrc) };
            }
        }
    }

    /// Nächste Fehlermeldung des Busses (ohne zu warten).
    pub fn poll_error(&self) -> Option<String> {
        let api = api().ok()?;
        if self.bus.is_null() {
            return None;
        }
        unsafe {
            let message = (api.gst_bus_timed_pop_filtered)(self.bus, 0, GST_MESSAGE_ERROR);
            if message.is_null() {
                return None;
            }
            let mut error: *mut GError = std::ptr::null_mut();
            let mut debug: *mut c_char = std::ptr::null_mut();
            (api.gst_message_parse_error)(message, &mut error, &mut debug);
            let mut text = take_error(api, error);
            if !debug.is_null() {
                text.push_str(&format!(" ({})", CStr::from_ptr(debug).to_string_lossy()));
                (api.g_free)(debug.cast());
            }
            (api.gst_mini_object_unref)(message);
            Some(text)
        }
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        let Ok(api) = api() else { return };
        unsafe {
            (api.gst_element_set_state)(self.pipeline, GST_STATE_NULL);
            if !self.appsrc.is_null() {
                (api.gst_object_unref)(self.appsrc);
            }
            if !self.bus.is_null() {
                (api.gst_bus_set_sync_handler)(self.bus, None, std::ptr::null_mut(), std::ptr::null_mut());
                (api.gst_object_unref)(self.bus);
            }
            (api.gst_object_unref)(self.pipeline);
        }
    }
}

/// Ein benanntes Element mit eigener Referenz.
pub struct Element(*mut GstElement);

unsafe impl Send for Element {}

impl Element {
    pub fn set_f64(&self, property: &str, value: f64) {
        if let Ok(api) = api() {
            unsafe { (api.g_object_set)(self.0, cstr(property).as_ptr(), value, std::ptr::null::<c_char>()) };
        }
    }

    pub fn set_bool(&self, property: &str, value: bool) {
        if let Ok(api) = api() {
            unsafe { (api.g_object_set)(self.0, cstr(property).as_ptr(), c_int::from(value), std::ptr::null::<c_char>()) };
        }
    }

    /// Hängt eine Sonde an das Sink-Pad, die jedes Bild als letztes Bild merkt.
    pub fn watch_frames(&self) {
        let Ok(api) = api() else { return };
        unsafe {
            let pad = (api.gst_element_get_static_pad)(self.0, c"sink".as_ptr());
            if !pad.is_null() {
                (api.gst_pad_add_probe)(pad, GST_PAD_PROBE_TYPE_BUFFER, Some(frame_probe), std::ptr::null_mut(), std::ptr::null_mut());
                (api.gst_object_unref)(pad);
            }
        }
    }
}

impl Drop for Element {
    fn drop(&mut self) {
        if let Ok(api) = api() {
            unsafe { (api.gst_object_unref)(self.0) };
        }
    }
}

// ------------------------------------------------------------ letztes Bild

struct LastFrame {
    sample: *mut GstSample,
    width: i32,
    height: i32,
}
unsafe impl Send for LastFrame {}

static LAST_FRAME: std::sync::Mutex<LastFrame> = std::sync::Mutex::new(LastFrame {
    sample: std::ptr::null_mut(),
    width: 0,
    height: 0,
});

/// Wird bei neuer Bildgröße mit (Breite, Höhe) aufgerufen.
static FRAME_SIZE_CALLBACK: OnceLock<Box<dyn Fn(i32, i32) + Send + Sync>> = OnceLock::new();

pub fn on_frame_size(callback: impl Fn(i32, i32) + Send + Sync + 'static) {
    let _ = FRAME_SIZE_CALLBACK.set(Box::new(callback));
}

unsafe extern "C" fn frame_probe(pad: *mut GstPad, info: *mut GstPadProbeInfo, _data: *mut c_void) -> c_int {
    let Ok(api) = api() else { return GST_PAD_PROBE_OK };
    let buffer = (*info).data;
    if buffer.is_null() {
        return GST_PAD_PROBE_OK;
    }
    let caps = (api.gst_pad_get_current_caps)(pad);
    if caps.is_null() {
        return GST_PAD_PROBE_OK;
    }
    let structure = (api.gst_caps_get_structure)(caps, 0);
    let (mut width, mut height) = (0, 0);
    (api.gst_structure_get_int)(structure, c"width".as_ptr(), &mut width);
    (api.gst_structure_get_int)(structure, c"height".as_ptr(), &mut height);
    let sample = (api.gst_sample_new)(buffer, caps, std::ptr::null_mut(), std::ptr::null_mut());
    (api.gst_mini_object_unref)(caps);
    let changed = {
        let mut last = LAST_FRAME.lock().unwrap();
        if !last.sample.is_null() {
            (api.gst_mini_object_unref)(last.sample);
        }
        last.sample = sample;
        let changed = last.width != width || last.height != height;
        last.width = width;
        last.height = height;
        changed
    };
    if changed {
        if let Some(callback) = FRAME_SIZE_CALLBACK.get() {
            callback(width, height);
        }
    }
    GST_PAD_PROBE_OK
}

/// Vergisst das letzte Bild (bei Verbindungsende).
pub fn clear_last_frame() {
    let Ok(api) = api() else { return };
    let mut last = LAST_FRAME.lock().unwrap();
    if !last.sample.is_null() {
        unsafe { (api.gst_mini_object_unref)(last.sample) };
    }
    last.sample = std::ptr::null_mut();
    last.width = 0;
    last.height = 0;
}

pub fn last_frame_size() -> Option<(i32, i32)> {
    let last = LAST_FRAME.lock().unwrap();
    (!last.sample.is_null() && last.width > 0 && last.height > 0).then_some((last.width, last.height))
}

/// Wandelt das letzte Bild in RGB um: (Breite, Höhe, Zeilenlänge, Pixel).
pub fn snapshot_rgb() -> Option<(usize, usize, usize, Vec<u8>)> {
    let api = api().ok()?;
    let (sample, width, height) = {
        let last = LAST_FRAME.lock().unwrap();
        if last.sample.is_null() || last.width <= 0 || last.height <= 0 {
            return None;
        }
        (unsafe { (api.gst_mini_object_ref)(last.sample) }, last.width, last.height)
    };
    unsafe {
        let caps = (api.gst_caps_from_string)(c"video/x-raw,format=RGB".as_ptr());
        let mut error: *mut GError = std::ptr::null_mut();
        let rgb = (api.gst_video_convert_sample)(sample, caps, 2 * GST_SECOND, &mut error);
        (api.gst_mini_object_unref)(caps);
        (api.gst_mini_object_unref)(sample);
        if rgb.is_null() {
            let _ = take_error(api, error);
            return None;
        }
        let result = (|| {
            let buffer = (api.gst_sample_get_buffer)(rgb);
            let out_caps = (api.gst_sample_get_caps)(rgb);
            let structure = (api.gst_caps_get_structure)(out_caps, 0);
            let (mut w, mut h) = (width, height);
            (api.gst_structure_get_int)(structure, c"width".as_ptr(), &mut w);
            (api.gst_structure_get_int)(structure, c"height".as_ptr(), &mut h);
            let mut map: GstMapInfo = std::mem::zeroed();
            if (api.gst_buffer_map)(buffer, &mut map, GST_MAP_READ) == 0 {
                return None;
            }
            let (w, h) = (w as usize, h as usize);
            let stride = (w * 3 + 3) & !3; // GStreamer-Standard für RGB
            let ok = map.size >= stride * h;
            let pixels = ok.then(|| std::slice::from_raw_parts(map.data, stride * h).to_vec());
            (api.gst_buffer_unmap)(buffer, &mut map);
            pixels.map(|p| (w, h, stride, p))
        })();
        (api.gst_mini_object_unref)(rgb);
        result
    }
}

