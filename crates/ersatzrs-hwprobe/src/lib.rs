//! Native hardware capability probes behind safe value objects.
//!
//! The command-generation crate must stay focused on `FFmpeg` argv and filter
//! graph construction. Direct OS/GPU APIs live here so any required FFI remains
//! small, platform-guarded, and testable at the boundary.

use std::collections::BTreeSet;

/// NVIDIA CUDA/NVDEC capabilities reported by native driver APIs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NvidiaProbe {
    /// Whether this runtime attempted the native CUDA/NVDEC probe.
    pub attempted: bool,
    /// Supported hardware decoder rows.
    pub decoders: Vec<NvidiaDecoderProbe>,
    /// Human-readable native device detail text, matching ErsatzTV's
    /// troubleshooting shape where practical.
    pub device_details: Option<String>,
}

/// One NVIDIA native decoder capability row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvidiaDecoderProbe {
    /// Codec name such as `h264`, `hevc`, or `av1`.
    pub codec: String,
    /// Supported bit depth.
    pub bit_depth: u8,
}

impl NvidiaDecoderProbe {
    /// Create a native NVIDIA decoder row.
    #[must_use]
    pub fn new(codec: impl Into<String>, bit_depth: u8) -> Self {
        Self {
            codec: codec.into(),
            bit_depth,
        }
    }
}

/// Probe native NVIDIA CUDA/NVDEC hardware decoder support.
///
/// macOS returns an unattempted empty probe, matching ErsatzTV's CUDA skip on
/// that platform. Other targets return an unattempted empty probe when the
/// NVIDIA native libraries are unavailable.
#[must_use]
pub fn probe_nvidia() -> NvidiaProbe {
    probe_nvidia_impl()
}

#[cfg(target_os = "macos")]
fn probe_nvidia_impl() -> NvidiaProbe {
    NvidiaProbe::default()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn probe_nvidia_impl() -> NvidiaProbe {
    nvidia::probe()
}

#[cfg(windows)]
fn probe_nvidia_impl() -> NvidiaProbe {
    nvidia::probe()
}

#[cfg(not(any(unix, windows)))]
fn probe_nvidia_impl() -> NvidiaProbe {
    NvidiaProbe::default()
}

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

#[cfg(any(all(unix, not(target_os = "macos")), windows))]
mod nvidia {
    use std::ffi::{c_char, c_int, c_uint, c_void, CStr, CString};
    use std::fmt::Write;
    use std::ptr;

    use crate::{NvidiaDecoderProbe, NvidiaProbe};

    type CuDevice = c_int;
    type CuContext = *mut c_void;
    type CuResult = c_int;

    const CUDA_SUCCESS: CuResult = 0;
    const CU_DEVICE_ATTRIBUTE_COMPUTE_CAPABILITY_MAJOR: c_int = 75;
    const CU_DEVICE_ATTRIBUTE_COMPUTE_CAPABILITY_MINOR: c_int = 76;
    const CUDA_VIDEO_CHROMA_FORMAT_YUV420: c_uint = 1;

    #[derive(Debug, Clone, Copy)]
    struct DecoderSpec {
        codec: &'static str,
        label: &'static str,
        cuda_codec: c_uint,
        bit_depth: u8,
    }

    const DECODER_SPECS: &[DecoderSpec] = &[
        DecoderSpec {
            codec: "mpeg2video",
            label: "mpeg2video 8-bit",
            cuda_codec: 1,
            bit_depth: 8,
        },
        DecoderSpec {
            codec: "mpeg2video",
            label: "mpeg2video 10-bit",
            cuda_codec: 1,
            bit_depth: 10,
        },
        DecoderSpec {
            codec: "mpeg4",
            label: "mpeg4 8-bit",
            cuda_codec: 2,
            bit_depth: 8,
        },
        DecoderSpec {
            codec: "mpeg4",
            label: "mpeg4 10-bit",
            cuda_codec: 2,
            bit_depth: 10,
        },
        DecoderSpec {
            codec: "vc1",
            label: "vc1 8-bit",
            cuda_codec: 3,
            bit_depth: 8,
        },
        DecoderSpec {
            codec: "vc1",
            label: "vc1 10-bit",
            cuda_codec: 3,
            bit_depth: 10,
        },
        DecoderSpec {
            codec: "h264",
            label: "h264 8-bit",
            cuda_codec: 4,
            bit_depth: 8,
        },
        DecoderSpec {
            codec: "h264",
            label: "h264 10-bit",
            cuda_codec: 4,
            bit_depth: 10,
        },
        DecoderSpec {
            codec: "hevc",
            label: "hevc 8-bit",
            cuda_codec: 8,
            bit_depth: 8,
        },
        DecoderSpec {
            codec: "hevc",
            label: "hevc 10-bit",
            cuda_codec: 8,
            bit_depth: 10,
        },
        DecoderSpec {
            codec: "vp8",
            label: "vp8 8-bit",
            cuda_codec: 9,
            bit_depth: 8,
        },
        DecoderSpec {
            codec: "vp8",
            label: "vp8 10-bit",
            cuda_codec: 9,
            bit_depth: 10,
        },
        DecoderSpec {
            codec: "vp9",
            label: "vp9 8-bit",
            cuda_codec: 10,
            bit_depth: 8,
        },
        DecoderSpec {
            codec: "vp9",
            label: "vp9 10-bit",
            cuda_codec: 10,
            bit_depth: 10,
        },
        DecoderSpec {
            codec: "av1",
            label: "av1 8-bit",
            cuda_codec: 11,
            bit_depth: 8,
        },
        DecoderSpec {
            codec: "av1",
            label: "av1 10-bit",
            cuda_codec: 11,
            bit_depth: 10,
        },
    ];

    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    struct CuvidDecodeCaps {
        codec_type: c_uint,
        chroma_format: c_uint,
        bit_depth_minus_8: c_uint,
        reserved1: [c_uint; 3],
        is_supported: CuCharBool,
        num_nvdecs: c_uint,
        output_format_mask: u16,
        max_width: u16,
        max_height: u16,
        max_mb_count: u16,
        min_width: u16,
        min_height: u16,
        reserved2: u16,
        reserved3: [c_uint; 11],
    }

    type CuCharBool = u8;

    impl CuvidDecodeCaps {
        fn for_spec(spec: DecoderSpec) -> Self {
            Self {
                codec_type: spec.cuda_codec,
                chroma_format: CUDA_VIDEO_CHROMA_FORMAT_YUV420,
                bit_depth_minus_8: c_uint::from(spec.bit_depth.saturating_sub(8)),
                reserved1: [0; 3],
                is_supported: 0,
                num_nvdecs: 0,
                output_format_mask: 0,
                max_width: 0,
                max_height: 0,
                max_mb_count: 0,
                min_width: 0,
                min_height: 0,
                reserved2: 0,
                reserved3: [0; 11],
            }
        }
    }

    struct CudaApi {
        _cuda: Library,
        _cuvid: Library,
        cu_init: unsafe extern "C" fn(c_uint) -> CuResult,
        cu_device_get_count: unsafe extern "C" fn(*mut c_int) -> CuResult,
        cu_device_get: unsafe extern "C" fn(*mut CuDevice, c_int) -> CuResult,
        cu_device_get_name: unsafe extern "C" fn(*mut c_char, c_int, CuDevice) -> CuResult,
        cu_device_get_attribute: unsafe extern "C" fn(*mut c_int, c_int, CuDevice) -> CuResult,
        cu_ctx_create: unsafe extern "C" fn(*mut CuContext, c_uint, CuDevice) -> CuResult,
        cu_ctx_destroy: unsafe extern "C" fn(CuContext) -> CuResult,
        cuvid_get_decoder_caps: unsafe extern "C" fn(*mut CuvidDecodeCaps) -> CuResult,
    }

    pub(super) fn probe() -> NvidiaProbe {
        let Some(api) = CudaApi::load() else {
            return NvidiaProbe::default();
        };

        unsafe {
            if (api.cu_init)(0) != CUDA_SUCCESS {
                return NvidiaProbe {
                    attempted: true,
                    ..NvidiaProbe::default()
                };
            }
        }

        let mut device_count = 0;
        unsafe {
            if (api.cu_device_get_count)(&raw mut device_count) != CUDA_SUCCESS || device_count <= 0
            {
                return NvidiaProbe {
                    attempted: true,
                    ..NvidiaProbe::default()
                };
            }
        }

        let mut decoders = Vec::new();
        let mut details = String::new();
        for index in 0..device_count {
            let Some(device) = api.device(index) else {
                continue;
            };
            let name = api.device_name(device);
            let major = api.device_attribute(device, CU_DEVICE_ATTRIBUTE_COMPUTE_CAPABILITY_MAJOR);
            let minor = api.device_attribute(device, CU_DEVICE_ATTRIBUTE_COMPUTE_CAPABILITY_MINOR);
            let _ = writeln!(
                details,
                "GPU #{index} < {name} > has Compute SM {}.{}",
                major.unwrap_or_default(),
                minor.unwrap_or_default()
            );
            let _ = writeln!(details);
            let _ = writeln!(details, "  Decoding:");

            let Some(context) = api.create_context(device) else {
                continue;
            };
            for spec in DECODER_SPECS {
                if api.decoder_supported(*spec) {
                    decoders.push(NvidiaDecoderProbe::new(spec.codec, spec.bit_depth));
                    let _ = writeln!(details, "    - Supports {}", spec.label);
                }
            }
            unsafe {
                let _ = (api.cu_ctx_destroy)(context);
            }
        }

        decoders.sort_by(|left, right| {
            left.codec
                .cmp(&right.codec)
                .then(left.bit_depth.cmp(&right.bit_depth))
        });
        decoders.dedup();

        NvidiaProbe {
            attempted: true,
            decoders,
            device_details: (!details.trim().is_empty()).then_some(details),
        }
    }

    impl CudaApi {
        fn load() -> Option<Self> {
            let cuda = Library::open_any(cuda_library_names())?;
            let cuvid = Library::open_any(cuvid_library_names())?;

            Some(Self {
                cu_init: cuda.symbol("cuInit\0")?,
                cu_device_get_count: cuda.symbol("cuDeviceGetCount\0")?,
                cu_device_get: cuda.symbol("cuDeviceGet\0")?,
                cu_device_get_name: cuda.symbol("cuDeviceGetName\0")?,
                cu_device_get_attribute: cuda.symbol("cuDeviceGetAttribute\0")?,
                cu_ctx_create: cuda.symbol_any(&["cuCtxCreate_v2\0", "cuCtxCreate\0"])?,
                cu_ctx_destroy: cuda.symbol_any(&["cuCtxDestroy_v2\0", "cuCtxDestroy\0"])?,
                cuvid_get_decoder_caps: cuvid.symbol("cuvidGetDecoderCaps\0")?,
                _cuda: cuda,
                _cuvid: cuvid,
            })
        }

        fn device(&self, index: c_int) -> Option<CuDevice> {
            let mut device = 0;
            unsafe {
                ((self.cu_device_get)(&raw mut device, index) == CUDA_SUCCESS).then_some(device)
            }
        }

        fn device_name(&self, device: CuDevice) -> String {
            let mut buffer = [0_i8; 256];
            unsafe {
                if (self.cu_device_get_name)(buffer.as_mut_ptr(), 256, device) == CUDA_SUCCESS {
                    return CStr::from_ptr(buffer.as_ptr())
                        .to_string_lossy()
                        .trim_end_matches('\0')
                        .to_owned();
                }
            }
            "Unknown NVIDIA GPU".to_owned()
        }

        fn device_attribute(&self, device: CuDevice, attribute: c_int) -> Option<c_int> {
            let mut value = 0;
            unsafe {
                ((self.cu_device_get_attribute)(&raw mut value, attribute, device) == CUDA_SUCCESS)
                    .then_some(value)
            }
        }

        fn create_context(&self, device: CuDevice) -> Option<CuContext> {
            let mut context = ptr::null_mut();
            unsafe {
                ((self.cu_ctx_create)(&raw mut context, 0, device) == CUDA_SUCCESS
                    && !context.is_null())
                .then_some(context)
            }
        }

        fn decoder_supported(&self, spec: DecoderSpec) -> bool {
            let mut caps = CuvidDecodeCaps::for_spec(spec);
            unsafe {
                (self.cuvid_get_decoder_caps)(&raw mut caps) == CUDA_SUCCESS
                    && caps.is_supported != 0
            }
        }
    }

    #[cfg(unix)]
    fn cuda_library_names() -> &'static [&'static str] {
        &["libcuda.so.1", "libcuda.so"]
    }

    #[cfg(windows)]
    fn cuda_library_names() -> &'static [&'static str] {
        &["nvcuda.dll"]
    }

    #[cfg(unix)]
    fn cuvid_library_names() -> &'static [&'static str] {
        &["libnvcuvid.so.1", "libnvcuvid.so"]
    }

    #[cfg(windows)]
    fn cuvid_library_names() -> &'static [&'static str] {
        &["nvcuvid.dll"]
    }

    struct Library {
        handle: *mut c_void,
    }

    impl Library {
        fn open_any(names: &[&str]) -> Option<Self> {
            names.iter().find_map(|name| Self::open(name))
        }

        fn symbol<T: Copy>(&self, name: &str) -> Option<T> {
            let address = self.symbol_address(name)?;
            debug_assert_eq!(std::mem::size_of::<T>(), std::mem::size_of::<*mut c_void>());
            Some(unsafe { std::mem::transmute_copy::<*mut c_void, T>(&address) })
        }

        fn symbol_any<T: Copy>(&self, names: &[&str]) -> Option<T> {
            names.iter().find_map(|name| self.symbol(name))
        }
    }

    impl Drop for Library {
        fn drop(&mut self) {
            close_library(self.handle);
        }
    }

    #[cfg(unix)]
    impl Library {
        fn open(name: &str) -> Option<Self> {
            let name = CString::new(name).ok()?;
            let handle = unsafe { dlopen(name.as_ptr(), RTLD_NOW) };
            (!handle.is_null()).then_some(Self { handle })
        }

        fn symbol_address(&self, name: &str) -> Option<*mut c_void> {
            let name = CString::new(name).ok()?;
            let address = unsafe { dlsym(self.handle, name.as_ptr()) };
            (!address.is_null()).then_some(address)
        }
    }

    #[cfg(unix)]
    const RTLD_NOW: c_int = 2;

    #[cfg(unix)]
    unsafe extern "C" {
        fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        fn dlclose(handle: *mut c_void) -> c_int;
    }

    #[cfg(unix)]
    fn close_library(handle: *mut c_void) {
        unsafe {
            let _ = dlclose(handle);
        }
    }

    #[cfg(windows)]
    impl Library {
        fn open(name: &str) -> Option<Self> {
            let name = CString::new(name).ok()?;
            let handle = unsafe { LoadLibraryA(name.as_ptr()) };
            (!handle.is_null()).then_some(Self {
                handle: handle.cast(),
            })
        }

        fn symbol_address(&self, name: &str) -> Option<*mut c_void> {
            let name = CString::new(name).ok()?;
            let address = unsafe { GetProcAddress(self.handle.cast(), name.as_ptr()) };
            (!address.is_null()).then_some(address.cast())
        }
    }

    #[cfg(windows)]
    unsafe extern "system" {
        fn LoadLibraryA(name: *const c_char) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
        fn FreeLibrary(module: *mut c_void) -> c_int;
    }

    #[cfg(windows)]
    fn close_library(handle: *mut c_void) {
        unsafe {
            let _ = FreeLibrary(handle);
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{CuvidDecodeCaps, DECODER_SPECS};

        #[test]
        fn decoder_specs_match_ersatztv_cuda_helper() {
            let labels = DECODER_SPECS
                .iter()
                .map(|spec| spec.label)
                .collect::<Vec<_>>();

            assert_eq!(
                labels,
                [
                    "mpeg2video 8-bit",
                    "mpeg2video 10-bit",
                    "mpeg4 8-bit",
                    "mpeg4 10-bit",
                    "vc1 8-bit",
                    "vc1 10-bit",
                    "h264 8-bit",
                    "h264 10-bit",
                    "hevc 8-bit",
                    "hevc 10-bit",
                    "vp8 8-bit",
                    "vp8 10-bit",
                    "vp9 8-bit",
                    "vp9 10-bit",
                    "av1 8-bit",
                    "av1 10-bit",
                ]
            );
        }

        #[test]
        fn cuvid_decode_caps_layout_covers_expected_fields() {
            assert!(std::mem::size_of::<CuvidDecodeCaps>() >= 64);
        }
    }
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
