#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFilter {
    None,
    LoudNorm,
}

const DEFAULT_TARGET_LOUDNESS: f64 = -16.0;
const DEFAULT_SAMPLE_RATE_KHZ: u32 = 48;

#[must_use]
pub fn normalize_loudness_filter(
    audio_filter: AudioFilter,
    target_loudness: Option<f64>,
    sample_rate_khz: Option<u32>,
) -> String {
    match audio_filter {
        AudioFilter::None => String::new(),
        AudioFilter::LoudNorm => {
            let loudness = target_loudness.unwrap_or(DEFAULT_TARGET_LOUDNESS);
            let sample_rate = sample_rate_khz.unwrap_or(DEFAULT_SAMPLE_RATE_KHZ) * 1000;
            format!("loudnorm=I={loudness}:TP=-1.5:LRA=11,aresample={sample_rate}")
        }
    }
}

#[must_use]
pub fn audio_resample_filter() -> String {
    "aresample=async=1".to_owned()
}

#[must_use]
pub fn audio_pad_filter() -> String {
    "apad".to_owned()
}
