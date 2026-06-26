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
    /// Whether this runtime attempted the native NVENC encoder probe.
    pub encoders_attempted: bool,
    /// Supported hardware encoder rows.
    pub encoders: Vec<NvidiaEncoderProbe>,
    /// Supported hardware decoder rows.
    pub decoders: Vec<NvidiaDecoderProbe>,
    /// Human-readable native device detail text, matching ErsatzTV's
    /// troubleshooting shape where practical.
    pub device_details: Option<String>,
}

/// One NVIDIA native encoder capability row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvidiaEncoderProbe {
    /// `FFmpeg` encoder name such as `h264_nvenc`, `hevc_nvenc`, or `av1_nvenc`.
    pub encoder: String,
    /// Pixel formats accepted by the encoder.
    pub pixel_formats: BTreeSet<String>,
    /// `FFmpeg` profile names accepted by the encoder.
    pub profiles: BTreeSet<String>,
    /// Whether the hardware encoder supports B-frame reference mode.
    pub supports_b_frames: bool,
}

impl NvidiaEncoderProbe {
    /// Create a native NVIDIA encoder row.
    #[must_use]
    pub fn new(
        encoder: impl Into<String>,
        pixel_formats: BTreeSet<String>,
        profiles: BTreeSet<String>,
        supports_b_frames: bool,
    ) -> Self {
        Self {
            encoder: encoder.into(),
            pixel_formats,
            profiles,
            supports_b_frames,
        }
    }
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
    use std::collections::BTreeSet;
    use std::ffi::{c_char, c_int, c_uint, c_void, CStr, CString};
    use std::fmt::Write;
    use std::ptr;

    use crate::{NvidiaDecoderProbe, NvidiaEncoderProbe, NvidiaProbe};

    type CuDevice = c_int;
    type CuContext = *mut c_void;
    type CuResult = c_int;
    type NvEncStatus = c_int;
    type NvEncoder = *mut c_void;

    const CUDA_SUCCESS: CuResult = 0;
    const CU_DEVICE_ATTRIBUTE_COMPUTE_CAPABILITY_MAJOR: c_int = 75;
    const CU_DEVICE_ATTRIBUTE_COMPUTE_CAPABILITY_MINOR: c_int = 76;
    const CUDA_VIDEO_CHROMA_FORMAT_YUV420: c_uint = 1;
    const NV_ENC_SUCCESS: c_int = 0;
    const NV_ENC_DEVICE_TYPE_CUDA: c_uint = 1;
    const NVENCAPI_VERSION: c_uint = 0x0c | (1 << 24);
    const NVENCAPI_STRUCT_VERSION_ONE: c_uint = NVENCAPI_VERSION | (1 << 16) | (0x7 << 28);
    const NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER: c_uint = NVENCAPI_STRUCT_VERSION_ONE;
    const NV_ENC_CAPS_PARAM_VER: c_uint = NVENCAPI_STRUCT_VERSION_ONE;
    const NV_ENCODE_API_FUNCTION_LIST_VER: c_uint = NVENCAPI_VERSION | (2 << 16) | (0x7 << 28);
    const NV_ENC_CAPS_SUPPORT_10BIT_ENCODE: c_uint = 39;
    const NV_ENC_CAPS_SUPPORT_BFRAME_REF_MODE: c_uint = 43;

    #[derive(Debug, Clone, Copy)]
    struct DecoderSpec {
        codec: &'static str,
        label: &'static str,
        cuda_codec: c_uint,
        bit_depth: u8,
    }

    #[derive(Debug, Clone, Copy)]
    struct EncoderSpec {
        encoder: &'static str,
        guid: Guid,
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Guid {
        data1: u32,
        data2: u16,
        data3: u16,
        data4: [u8; 8],
    }

    impl Guid {
        const fn new(data1: u32, data2: u16, data3: u16, data4: [u8; 8]) -> Self {
            Self {
                data1,
                data2,
                data3,
                data4,
            }
        }
    }

    const NV_ENC_CODEC_H264_GUID: Guid = Guid::new(
        0x6bc8_2762,
        0x4e63,
        0x4ca4,
        [0xaa, 0x85, 0x1e, 0x50, 0xf3, 0x21, 0xf6, 0xbf],
    );
    const NV_ENC_CODEC_HEVC_GUID: Guid = Guid::new(
        0x790c_dc88,
        0x4522,
        0x4d7b,
        [0x94, 0x25, 0xbd, 0xa9, 0x97, 0x5f, 0x76, 0x03],
    );
    const NV_ENC_CODEC_AV1_GUID: Guid = Guid::new(
        0x0a35_2289,
        0x0aa7,
        0x4759,
        [0x86, 0x2d, 0x5d, 0x15, 0xcd, 0x16, 0xd2, 0x54],
    );
    const NV_ENC_H264_PROFILE_BASELINE_GUID: Guid = Guid::new(
        0x0727_bcaa,
        0x78c4,
        0x4c83,
        [0x8c, 0x2f, 0xef, 0x3d, 0xff, 0x26, 0x7c, 0x6a],
    );
    const NV_ENC_H264_PROFILE_MAIN_GUID: Guid = Guid::new(
        0x60b5_c1d4,
        0x67fe,
        0x4790,
        [0x94, 0xd5, 0xc4, 0x72, 0x6d, 0x7b, 0x6e, 0x6d],
    );
    const NV_ENC_H264_PROFILE_HIGH_GUID: Guid = Guid::new(
        0xe7cb_c309,
        0x4f7a,
        0x4b89,
        [0xaf, 0x2a, 0xd5, 0x37, 0xc9, 0x2b, 0xe3, 0x10],
    );
    const NV_ENC_H264_PROFILE_HIGH_444_GUID: Guid = Guid::new(
        0x7ac6_63cb,
        0xa598,
        0x4960,
        [0xb8, 0x44, 0x33, 0x9b, 0x26, 0x1a, 0x7d, 0x52],
    );
    const NV_ENC_HEVC_PROFILE_MAIN_GUID: Guid = Guid::new(
        0xb514_c39a,
        0xb55b,
        0x40fa,
        [0x87, 0x8f, 0xf1, 0x25, 0x3b, 0x4d, 0xfd, 0xec],
    );
    const NV_ENC_HEVC_PROFILE_MAIN10_GUID: Guid = Guid::new(
        0xfa4d_2b6c,
        0x3a5b,
        0x411a,
        [0x80, 0x18, 0x0a, 0x3f, 0x5e, 0x3c, 0x9b, 0xe5],
    );
    const NV_ENC_AV1_PROFILE_MAIN_GUID: Guid = Guid::new(
        0x5f2a_39f5,
        0xf14e,
        0x4f95,
        [0x9a, 0x9e, 0xb7, 0x6d, 0x56, 0x8f, 0xcf, 0x97],
    );

    const ENCODER_SPECS: &[EncoderSpec] = &[
        EncoderSpec {
            encoder: "h264_nvenc",
            guid: NV_ENC_CODEC_H264_GUID,
        },
        EncoderSpec {
            encoder: "hevc_nvenc",
            guid: NV_ENC_CODEC_HEVC_GUID,
        },
        EncoderSpec {
            encoder: "av1_nvenc",
            guid: NV_ENC_CODEC_AV1_GUID,
        },
    ];

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

    #[repr(C)]
    struct NvEncOpenEncodeSessionExParams {
        version: c_uint,
        device_type: c_uint,
        device: *mut c_void,
        reserved: *mut c_void,
        api_version: c_uint,
        reserved1: [c_uint; 253],
        reserved2: [*mut c_void; 64],
    }

    impl NvEncOpenEncodeSessionExParams {
        fn for_cuda_context(context: CuContext) -> Self {
            Self {
                version: NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER,
                device_type: NV_ENC_DEVICE_TYPE_CUDA,
                device: context,
                reserved: ptr::null_mut(),
                api_version: NVENCAPI_VERSION,
                reserved1: [0; 253],
                reserved2: [ptr::null_mut(); 64],
            }
        }
    }

    #[repr(C)]
    struct NvEncCapsParam {
        version: c_uint,
        caps_to_query: c_uint,
        reserved: [c_uint; 62],
    }

    impl NvEncCapsParam {
        fn new(caps_to_query: c_uint) -> Self {
            Self {
                version: NV_ENC_CAPS_PARAM_VER,
                caps_to_query,
                reserved: [0; 62],
            }
        }
    }

    type NvEncOpenEncodeSession =
        Option<unsafe extern "C" fn(*mut c_void, c_uint, *mut NvEncoder) -> NvEncStatus>;
    type NvEncGetEncodeGuidCount =
        Option<unsafe extern "C" fn(NvEncoder, *mut c_uint) -> NvEncStatus>;
    type NvEncGetEncodeGuids =
        Option<unsafe extern "C" fn(NvEncoder, *mut Guid, c_uint, *mut c_uint) -> NvEncStatus>;
    type NvEncGetEncodeProfileGuidCount =
        Option<unsafe extern "C" fn(NvEncoder, Guid, *mut c_uint) -> NvEncStatus>;
    type NvEncGetEncodeProfileGuids = Option<
        unsafe extern "C" fn(NvEncoder, Guid, *mut Guid, c_uint, *mut c_uint) -> NvEncStatus,
    >;
    type NvEncGetEncodeCaps = Option<
        unsafe extern "C" fn(NvEncoder, Guid, *mut NvEncCapsParam, *mut c_int) -> NvEncStatus,
    >;
    type NvEncDestroyEncoder = Option<unsafe extern "C" fn(NvEncoder) -> NvEncStatus>;
    type NvEncOpenEncodeSessionEx = Option<
        unsafe extern "C" fn(*mut NvEncOpenEncodeSessionExParams, *mut NvEncoder) -> NvEncStatus,
    >;

    #[repr(C)]
    struct NvEncodeApiFunctionList {
        version: c_uint,
        reserved: c_uint,
        nv_enc_open_encode_session: NvEncOpenEncodeSession,
        nv_enc_get_encode_guid_count: NvEncGetEncodeGuidCount,
        nv_enc_get_encode_profile_guid_count: NvEncGetEncodeProfileGuidCount,
        nv_enc_get_encode_profile_guids: NvEncGetEncodeProfileGuids,
        nv_enc_get_encode_guids: NvEncGetEncodeGuids,
        nv_enc_get_input_format_count: *mut c_void,
        nv_enc_get_input_formats: *mut c_void,
        nv_enc_get_encode_caps: NvEncGetEncodeCaps,
        nv_enc_get_encode_preset_count: *mut c_void,
        nv_enc_get_encode_preset_guids: *mut c_void,
        nv_enc_get_encode_preset_config: *mut c_void,
        nv_enc_initialize_encoder: *mut c_void,
        nv_enc_create_input_buffer: *mut c_void,
        nv_enc_destroy_input_buffer: *mut c_void,
        nv_enc_create_bitstream_buffer: *mut c_void,
        nv_enc_destroy_bitstream_buffer: *mut c_void,
        nv_enc_encode_picture: *mut c_void,
        nv_enc_lock_bitstream: *mut c_void,
        nv_enc_unlock_bitstream: *mut c_void,
        nv_enc_lock_input_buffer: *mut c_void,
        nv_enc_unlock_input_buffer: *mut c_void,
        nv_enc_get_encode_stats: *mut c_void,
        nv_enc_get_sequence_params: *mut c_void,
        nv_enc_register_async_event: *mut c_void,
        nv_enc_unregister_async_event: *mut c_void,
        nv_enc_map_input_resource: *mut c_void,
        nv_enc_unmap_input_resource: *mut c_void,
        nv_enc_destroy_encoder: NvEncDestroyEncoder,
        nv_enc_invalidate_ref_frames: *mut c_void,
        nv_enc_open_encode_session_ex: NvEncOpenEncodeSessionEx,
        nv_enc_register_resource: *mut c_void,
        nv_enc_unregister_resource: *mut c_void,
        nv_enc_reconfigure_encoder: *mut c_void,
        reserved1: *mut c_void,
        nv_enc_create_mv_buffer: *mut c_void,
        nv_enc_destroy_mv_buffer: *mut c_void,
        nv_enc_run_motion_estimation_only: *mut c_void,
        nv_enc_get_last_error_string: *mut c_void,
        nv_enc_set_io_cuda_streams: *mut c_void,
        nv_enc_get_encode_preset_config_ex: *mut c_void,
        nv_enc_get_sequence_param_ex: *mut c_void,
        nv_enc_restore_encoder_state: *mut c_void,
        nv_enc_lookahead_picture: *mut c_void,
        reserved2: [*mut c_void; 275],
    }

    impl NvEncodeApiFunctionList {
        fn new() -> Self {
            Self {
                version: NV_ENCODE_API_FUNCTION_LIST_VER,
                reserved: 0,
                nv_enc_open_encode_session: None,
                nv_enc_get_encode_guid_count: None,
                nv_enc_get_encode_profile_guid_count: None,
                nv_enc_get_encode_profile_guids: None,
                nv_enc_get_encode_guids: None,
                nv_enc_get_input_format_count: ptr::null_mut(),
                nv_enc_get_input_formats: ptr::null_mut(),
                nv_enc_get_encode_caps: None,
                nv_enc_get_encode_preset_count: ptr::null_mut(),
                nv_enc_get_encode_preset_guids: ptr::null_mut(),
                nv_enc_get_encode_preset_config: ptr::null_mut(),
                nv_enc_initialize_encoder: ptr::null_mut(),
                nv_enc_create_input_buffer: ptr::null_mut(),
                nv_enc_destroy_input_buffer: ptr::null_mut(),
                nv_enc_create_bitstream_buffer: ptr::null_mut(),
                nv_enc_destroy_bitstream_buffer: ptr::null_mut(),
                nv_enc_encode_picture: ptr::null_mut(),
                nv_enc_lock_bitstream: ptr::null_mut(),
                nv_enc_unlock_bitstream: ptr::null_mut(),
                nv_enc_lock_input_buffer: ptr::null_mut(),
                nv_enc_unlock_input_buffer: ptr::null_mut(),
                nv_enc_get_encode_stats: ptr::null_mut(),
                nv_enc_get_sequence_params: ptr::null_mut(),
                nv_enc_register_async_event: ptr::null_mut(),
                nv_enc_unregister_async_event: ptr::null_mut(),
                nv_enc_map_input_resource: ptr::null_mut(),
                nv_enc_unmap_input_resource: ptr::null_mut(),
                nv_enc_destroy_encoder: None,
                nv_enc_invalidate_ref_frames: ptr::null_mut(),
                nv_enc_open_encode_session_ex: None,
                nv_enc_register_resource: ptr::null_mut(),
                nv_enc_unregister_resource: ptr::null_mut(),
                nv_enc_reconfigure_encoder: ptr::null_mut(),
                reserved1: ptr::null_mut(),
                nv_enc_create_mv_buffer: ptr::null_mut(),
                nv_enc_destroy_mv_buffer: ptr::null_mut(),
                nv_enc_run_motion_estimation_only: ptr::null_mut(),
                nv_enc_get_last_error_string: ptr::null_mut(),
                nv_enc_set_io_cuda_streams: ptr::null_mut(),
                nv_enc_get_encode_preset_config_ex: ptr::null_mut(),
                nv_enc_get_sequence_param_ex: ptr::null_mut(),
                nv_enc_restore_encoder_state: ptr::null_mut(),
                nv_enc_lookahead_picture: ptr::null_mut(),
                reserved2: [ptr::null_mut(); 275],
            }
        }
    }

    struct CudaApi {
        _cuda: Library,
        _cuvid: Library,
        nvenc: Option<NvencApi>,
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

        let encoders_attempted = api.nvenc.is_some();
        let mut encoders = Vec::new();
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

            let Some(context) = api.create_context(device) else {
                continue;
            };

            let _ = writeln!(details, "  Encoding:");
            if let Some(nvenc) = &api.nvenc {
                for encoder in nvenc.encoders(context) {
                    let b_frames = if encoder.supports_b_frames {
                        " (with B-frames)"
                    } else {
                        ""
                    };
                    let supports_10_bit = encoder
                        .pixel_formats
                        .iter()
                        .any(|format| format.eq_ignore_ascii_case("p010le"));
                    let label = encoder_label(&encoder.encoder);
                    let _ = writeln!(details, "    - Supports {label} 8-bit{b_frames}");
                    if supports_10_bit {
                        let _ = writeln!(details, "    - Supports {label} 10-bit{b_frames}");
                    }
                    encoders.push(encoder);
                }
            }

            let _ = writeln!(details);
            let _ = writeln!(details, "  Decoding:");
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

        encoders.sort_by(|left, right| left.encoder.cmp(&right.encoder));
        encoders.dedup_by(|left, right| {
            left.encoder == right.encoder
                && left.pixel_formats == right.pixel_formats
                && left.profiles == right.profiles
                && left.supports_b_frames == right.supports_b_frames
        });
        decoders.sort_by(|left, right| {
            left.codec
                .cmp(&right.codec)
                .then(left.bit_depth.cmp(&right.bit_depth))
        });
        decoders.dedup();

        NvidiaProbe {
            attempted: true,
            encoders_attempted,
            encoders,
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
                nvenc: NvencApi::load(),
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
            let mut buffer = [0 as c_char; 256];
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

    struct NvencApi {
        _nvenc: Library,
        functions: NvEncodeApiFunctionList,
    }

    impl NvencApi {
        fn load() -> Option<Self> {
            let nvenc = Library::open_any(nvenc_library_names())?;
            let create_instance: unsafe extern "C" fn(*mut NvEncodeApiFunctionList) -> NvEncStatus =
                nvenc.symbol("NvEncodeAPICreateInstance\0")?;
            let mut functions = NvEncodeApiFunctionList::new();
            unsafe {
                if create_instance(&raw mut functions) != NV_ENC_SUCCESS {
                    return None;
                }
            }

            if functions.nv_enc_open_encode_session_ex.is_none()
                || functions.nv_enc_get_encode_guid_count.is_none()
                || functions.nv_enc_get_encode_guids.is_none()
                || functions.nv_enc_get_encode_profile_guid_count.is_none()
                || functions.nv_enc_get_encode_profile_guids.is_none()
                || functions.nv_enc_get_encode_caps.is_none()
                || functions.nv_enc_destroy_encoder.is_none()
            {
                return None;
            }

            Some(Self {
                _nvenc: nvenc,
                functions,
            })
        }

        fn encoders(&self, context: CuContext) -> Vec<NvidiaEncoderProbe> {
            let Some(encoder) = self.open_encoder(context) else {
                return Vec::new();
            };

            let codec_guids = self.encode_guids(encoder);
            let mut encoders = Vec::new();
            for spec in ENCODER_SPECS {
                if !codec_guids.contains(&spec.guid) {
                    continue;
                }

                let supports_10_bit =
                    self.capability(encoder, spec.guid, NV_ENC_CAPS_SUPPORT_10BIT_ENCODE) > 0;
                let supports_b_frames =
                    self.capability(encoder, spec.guid, NV_ENC_CAPS_SUPPORT_BFRAME_REF_MODE) > 0;
                let mut pixel_formats = BTreeSet::from(["nv12".to_owned()]);
                if supports_10_bit {
                    pixel_formats.insert("p010le".to_owned());
                }
                let profiles = self
                    .profile_guids(encoder, spec.guid)
                    .into_iter()
                    .filter_map(profile_name)
                    .map(str::to_owned)
                    .collect();

                encoders.push(NvidiaEncoderProbe::new(
                    spec.encoder,
                    pixel_formats,
                    profiles,
                    supports_b_frames,
                ));
            }

            unsafe {
                let _ = (self.functions.nv_enc_destroy_encoder.expect("checked"))(encoder);
            }
            encoders
        }

        fn open_encoder(&self, context: CuContext) -> Option<NvEncoder> {
            let mut params = NvEncOpenEncodeSessionExParams::for_cuda_context(context);
            let mut encoder = ptr::null_mut();
            unsafe {
                ((self
                    .functions
                    .nv_enc_open_encode_session_ex
                    .expect("checked"))(&raw mut params, &raw mut encoder)
                    == NV_ENC_SUCCESS
                    && !encoder.is_null())
                .then_some(encoder)
            }
        }

        fn encode_guids(&self, encoder: NvEncoder) -> Vec<Guid> {
            let Some(count) = self.count(encoder, CountKind::EncodeGuid) else {
                return Vec::new();
            };
            let mut guids = vec![zeroed_guid(); count as usize];
            let mut actual = 0;
            unsafe {
                if (self.functions.nv_enc_get_encode_guids.expect("checked"))(
                    encoder,
                    guids.as_mut_ptr(),
                    count,
                    &raw mut actual,
                ) != NV_ENC_SUCCESS
                {
                    return Vec::new();
                }
            }
            guids.truncate(actual as usize);
            guids
        }

        fn profile_guids(&self, encoder: NvEncoder, codec: Guid) -> Vec<Guid> {
            let Some(count) = self.count_for_codec(encoder, codec, CountKind::ProfileGuid) else {
                return Vec::new();
            };
            let mut guids = vec![zeroed_guid(); count as usize];
            let mut actual = 0;
            unsafe {
                if (self
                    .functions
                    .nv_enc_get_encode_profile_guids
                    .expect("checked"))(
                    encoder, codec, guids.as_mut_ptr(), count, &raw mut actual
                ) != NV_ENC_SUCCESS
                {
                    return Vec::new();
                }
            }
            guids.truncate(actual as usize);
            guids
        }

        fn capability(&self, encoder: NvEncoder, codec: Guid, capability: c_uint) -> c_int {
            let mut params = NvEncCapsParam::new(capability);
            let mut value = 0;
            unsafe {
                if (self.functions.nv_enc_get_encode_caps.expect("checked"))(
                    encoder,
                    codec,
                    &raw mut params,
                    &raw mut value,
                ) == NV_ENC_SUCCESS
                {
                    value
                } else {
                    0
                }
            }
        }

        fn count(&self, encoder: NvEncoder, kind: CountKind) -> Option<c_uint> {
            let mut count = 0;
            let status = unsafe {
                match kind {
                    CountKind::EncodeGuid => {
                        (self
                            .functions
                            .nv_enc_get_encode_guid_count
                            .expect("checked"))(encoder, &raw mut count)
                    }
                    CountKind::ProfileGuid => return None,
                }
            };
            (status == NV_ENC_SUCCESS).then_some(count)
        }

        fn count_for_codec(
            &self,
            encoder: NvEncoder,
            codec: Guid,
            kind: CountKind,
        ) -> Option<c_uint> {
            let mut count = 0;
            let status = unsafe {
                match kind {
                    CountKind::EncodeGuid => return None,
                    CountKind::ProfileGuid => (self
                        .functions
                        .nv_enc_get_encode_profile_guid_count
                        .expect("checked"))(
                        encoder, codec, &raw mut count
                    ),
                }
            };
            (status == NV_ENC_SUCCESS).then_some(count)
        }
    }

    #[derive(Debug, Clone, Copy)]
    enum CountKind {
        EncodeGuid,
        ProfileGuid,
    }

    fn zeroed_guid() -> Guid {
        Guid::new(0, 0, 0, [0; 8])
    }

    fn encoder_label(encoder: &str) -> &str {
        match encoder {
            "h264_nvenc" => "h264",
            "hevc_nvenc" => "hevc",
            "av1_nvenc" => "av1",
            _ => encoder,
        }
    }

    fn profile_name(guid: Guid) -> Option<&'static str> {
        match guid {
            NV_ENC_H264_PROFILE_BASELINE_GUID => Some("baseline"),
            NV_ENC_H264_PROFILE_MAIN_GUID
            | NV_ENC_HEVC_PROFILE_MAIN_GUID
            | NV_ENC_AV1_PROFILE_MAIN_GUID => Some("main"),
            NV_ENC_H264_PROFILE_HIGH_GUID => Some("high"),
            NV_ENC_H264_PROFILE_HIGH_444_GUID => Some("high444p"),
            NV_ENC_HEVC_PROFILE_MAIN10_GUID => Some("main10"),
            _ => None,
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

    #[cfg(unix)]
    fn nvenc_library_names() -> &'static [&'static str] {
        &["libnvidia-encode.so.1", "libnvidia-encode.so"]
    }

    #[cfg(windows)]
    fn nvenc_library_names() -> &'static [&'static str] {
        &["nvEncodeAPI64.dll"]
    }

    struct Library {
        // NVIDIA's Linux/WSL driver stack can fault from `dlclose` after
        // CUDA/NVENC probing. Keep these process-global driver handles resident
        // once opened; the probe only loads a bounded set of well-known
        // libraries.
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
    }

    #[cfg(test)]
    mod tests {
        use super::{
            profile_name, CuvidDecodeCaps, NvEncOpenEncodeSessionExParams, NvEncodeApiFunctionList,
            DECODER_SPECS, ENCODER_SPECS, NV_ENC_AV1_PROFILE_MAIN_GUID,
            NV_ENC_H264_PROFILE_HIGH_444_GUID, NV_ENC_HEVC_PROFILE_MAIN10_GUID,
        };

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

        #[test]
        fn encoder_specs_match_ersatztv_cuda_helper() {
            let encoders = ENCODER_SPECS
                .iter()
                .map(|spec| spec.encoder)
                .collect::<Vec<_>>();

            assert_eq!(encoders, ["h264_nvenc", "hevc_nvenc", "av1_nvenc"]);
            assert_eq!(
                profile_name(NV_ENC_H264_PROFILE_HIGH_444_GUID),
                Some("high444p")
            );
            assert_eq!(
                profile_name(NV_ENC_HEVC_PROFILE_MAIN10_GUID),
                Some("main10")
            );
            assert_eq!(profile_name(NV_ENC_AV1_PROFILE_MAIN_GUID), Some("main"));
        }

        #[test]
        fn nvenc_layouts_cover_expected_fields() {
            assert!(std::mem::size_of::<NvEncOpenEncodeSessionExParams>() >= 1_280);
            assert!(std::mem::size_of::<NvEncodeApiFunctionList>() >= 2_500);
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
