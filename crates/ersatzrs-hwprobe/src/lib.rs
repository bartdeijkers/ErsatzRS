//! Native hardware capability probes behind safe value objects.
//!
//! The command-generation crate must stay focused on `FFmpeg` argv and filter
//! graph construction. Direct OS/GPU APIs live here so any required FFI remains
//! small, platform-guarded, and testable at the boundary.

use std::collections::BTreeSet;

/// `VideoToolbox` hardware capabilities reported by macOS framework APIs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VideoToolboxProbe {
    /// Whether this runtime attempted the native framework probe.
    pub attempted: bool,
    /// Supported hardware decoder `FourCC` values such as `avc1` or `hvc1`.
    pub decoders: BTreeSet<String>,
    /// Hardware encoder names such as `H.264 (HW)` or `HEVC (HW)`.
    pub encoders: BTreeSet<String>,
}

/// Probe macOS `VideoToolbox` hardware decode and encode support.
///
/// Non-macOS targets return an unattempted empty probe.
#[must_use]
pub fn probe_videotoolbox() -> VideoToolboxProbe {
    probe_videotoolbox_impl()
}

#[cfg(not(target_os = "macos"))]
fn probe_videotoolbox_impl() -> VideoToolboxProbe {
    VideoToolboxProbe::default()
}

#[cfg(target_os = "macos")]
fn probe_videotoolbox_impl() -> VideoToolboxProbe {
    videotoolbox::probe()
}

#[cfg(target_os = "macos")]
mod videotoolbox {
    use std::collections::BTreeSet;
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_int, c_void};
    use std::ptr;

    use crate::VideoToolboxProbe;

    const VIDEO_TOOLBOX_PATH: &str =
        "/System/Library/Frameworks/VideoToolbox.framework/VideoToolbox";
    const RTLD_NOW: c_int = 2;
    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFArrayGetCount(array: *const c_void) -> isize;
        fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
        fn CFDictionaryGetValue(dict: *const c_void, key: *const c_void) -> *const c_void;
        fn CFStringGetLength(the_string: *const c_void) -> isize;
        fn CFStringGetCStringPtr(the_string: *const c_void, encoding: u32) -> *const c_char;
        fn CFStringGetCString(
            the_string: *const c_void,
            buffer: *mut c_char,
            buffer_size: isize,
            encoding: u32,
        ) -> u8;
        fn CFRelease(cf: *const c_void);
    }

    #[link(name = "VideoToolbox", kind = "framework")]
    unsafe extern "C" {
        fn VTCopyVideoEncoderList(
            options: *const c_void,
            list_of_encoders: *mut *const c_void,
        ) -> i32;
        fn VTIsHardwareDecodeSupported(codec_type: u32) -> u8;
    }

    #[link(name = "System")]
    unsafe extern "C" {
        fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        fn dlclose(handle: *mut c_void) -> c_int;
    }

    pub(super) fn probe() -> VideoToolboxProbe {
        let mut probe = VideoToolboxProbe {
            attempted: true,
            decoders: BTreeSet::new(),
            encoders: BTreeSet::new(),
        };

        for fourcc in ["av01", "avc1", "hvc1", "mp2v", "mp4v", "vp09"] {
            if is_hardware_decoder_supported(fourcc) {
                probe.decoders.insert(fourcc.to_owned());
            }
        }

        for encoder in available_encoder_names() {
            if encoder.contains("HEVC (HW)") {
                probe.encoders.insert("HEVC (HW)".to_owned());
            }
            if encoder.contains("H.264 (HW)") {
                probe.encoders.insert("H.264 (HW)".to_owned());
            }
        }

        probe
    }

    fn is_hardware_decoder_supported(fourcc: &str) -> bool {
        let Some(codec) = fourcc_to_u32(fourcc) else {
            return false;
        };
        unsafe { VTIsHardwareDecodeSupported(codec) != 0 }
    }

    fn available_encoder_names() -> Vec<String> {
        let Some(encoder_name_key) =
            get_cf_string_symbol(VIDEO_TOOLBOX_PATH, "kVTVideoEncoderList_EncoderName")
        else {
            return Vec::new();
        };

        let mut encoder_list = ptr::null();
        let status = unsafe { VTCopyVideoEncoderList(ptr::null(), &raw mut encoder_list) };
        if status != 0 || encoder_list.is_null() {
            return Vec::new();
        }

        let mut names = Vec::new();
        let count = unsafe { CFArrayGetCount(encoder_list) };
        for index in 0..count {
            let encoder_dict = unsafe { CFArrayGetValueAtIndex(encoder_list, index) };
            if encoder_dict.is_null() {
                continue;
            }
            let cf_name = unsafe { CFDictionaryGetValue(encoder_dict, encoder_name_key) };
            if let Some(name) = cf_string_to_string(cf_name) {
                names.push(name);
            }
        }
        unsafe { CFRelease(encoder_list) };
        names
    }

    fn get_cf_string_symbol(framework_path: &str, symbol_name: &str) -> Option<*const c_void> {
        let framework = CString::new(framework_path).ok()?;
        let symbol = CString::new(symbol_name).ok()?;
        let handle = unsafe { dlopen(framework.as_ptr(), RTLD_NOW) };
        if handle.is_null() {
            return None;
        }

        let raw_symbol = unsafe { dlsym(handle, symbol.as_ptr()) };
        let result = if raw_symbol.is_null() {
            None
        } else {
            Some(unsafe { *(raw_symbol.cast::<*const c_void>()) })
        };
        unsafe {
            let _ = dlclose(handle);
        }
        result
    }

    fn cf_string_to_string(cf_string: *const c_void) -> Option<String> {
        if cf_string.is_null() {
            return None;
        }

        let c_string_ptr = unsafe { CFStringGetCStringPtr(cf_string, K_CF_STRING_ENCODING_UTF8) };
        if !c_string_ptr.is_null() {
            return unsafe { CStr::from_ptr(c_string_ptr) }
                .to_str()
                .ok()
                .map(str::to_owned);
        }

        let length = unsafe { CFStringGetLength(cf_string) };
        if length == 0 {
            return Some(String::new());
        }
        let max_size = length.saturating_mul(4).saturating_add(1);
        let buffer_size = usize::try_from(max_size).ok()?;
        let mut buffer = vec![0_i8; buffer_size];
        let copied = unsafe {
            CFStringGetCString(
                cf_string,
                buffer.as_mut_ptr(),
                max_size,
                K_CF_STRING_ENCODING_UTF8,
            )
        };
        if copied == 0 {
            return None;
        }
        unsafe { CStr::from_ptr(buffer.as_ptr()) }
            .to_str()
            .ok()
            .map(str::to_owned)
    }

    fn fourcc_to_u32(fourcc: &str) -> Option<u32> {
        let bytes = fourcc.as_bytes();
        if bytes.len() != 4 {
            return None;
        }
        Some(
            (u32::from(bytes[0]) << 24)
                | (u32::from(bytes[1]) << 16)
                | (u32::from(bytes[2]) << 8)
                | u32::from(bytes[3]),
        )
    }

    #[cfg(test)]
    mod tests {
        use super::fourcc_to_u32;

        #[test]
        fn fourcc_to_u32_uses_big_endian_codes() {
            assert_eq!(fourcc_to_u32("avc1"), Some(0x6176_6331));
            assert_eq!(fourcc_to_u32("hvc1"), Some(0x6876_6331));
            assert_eq!(fourcc_to_u32("bad"), None);
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn videotoolbox_probe_is_unattempted_off_macos() {
        let probe = super::probe_videotoolbox();

        assert!(!probe.attempted);
        assert!(probe.decoders.is_empty());
        assert!(probe.encoders.is_empty());
    }
}
