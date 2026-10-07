//! Decode the finite application output, never a separately constructed proof manifest.
use super::*;

pub(super) struct Decoded {
    pub rgb: Vec<u8>,
    pub pcm: Vec<u8>,
    pub packets: serde_json::Value,
    pub seconds: f64,
}

pub(super) async fn decode(playlist: &FsPath) -> Decoded {
    let body = tokio::fs::read_to_string(playlist).await.unwrap();
    assert!(body.lines().any(|line| line == "#EXT-X-ENDLIST"));
    let seconds: f64 = body
        .lines()
        .filter_map(|line| line.strip_prefix("#EXTINF:"))
        .map(|value| value.split(',').next().unwrap().parse::<f64>().unwrap())
        .sum();
    assert!(
        (29.80..=30.20).contains(&seconds),
        "complete thirty-second output required, got {seconds}"
    );
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let mut video = Command::new(&ffmpeg);
    video
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(playlist)
        .args([
            "-map", "0:v:0", "-vf", "fps=1", "-pix_fmt", "rgb24", "-f", "rawvideo", "pipe:1",
        ]);
    let rgb = tool(video).await.unwrap();
    assert!(rgb.len().is_multiple_of(320 * 180 * 3));
    assert!(
        (29..=31).contains(&(rgb.len() / (320 * 180 * 3))),
        "decoded timeline was shortened"
    );
    let mut audio = Command::new(&ffmpeg);
    audio
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(playlist)
        .args([
            "-map",
            "0:a:0",
            "-af",
            "asetpts=N/SR/TB",
            "-ac",
            "1",
            "-ar",
            "8000",
            "-f",
            "s16le",
            "pipe:1",
        ]);
    let pcm = tool(audio).await.unwrap();
    assert!((29.80..=30.20).contains(&(f64::from(u32::try_from(pcm.len() / 2).unwrap()) / 8000.0)));
    let mut probe = Command::new(runtime_path("ERSATZRS_NEXT_TEST_FFPROBE"));
    probe.args(["-v", "error", "-i"]).arg(playlist).args([
        "-select_streams",
        "v:0",
        "-show_packets",
        "-show_entries",
        "packet=pts_time,duration_time",
        "-of",
        "json",
    ]);
    let packets = serde_json::from_slice(&tool(probe).await.unwrap()).unwrap();
    Decoded {
        rgb,
        pcm,
        packets,
        seconds,
    }
}

impl Decoded {
    fn frame(&self, second: usize) -> &[u8] {
        let size = 320 * 180 * 3;
        &self.rgb[second * size..(second + 1) * size]
    }

    fn pixel(&self, second: usize, x: usize, y: usize) -> &[u8] {
        let offset = (y * 320 + x) * 3;
        &self.frame(second)[offset..offset + 3]
    }

    fn color(&self, second: usize, expected: [u8; 3]) {
        let found = self.pixel(second, 100, 80);
        assert!(
            found
                .iter()
                .zip(expected)
                .all(|(actual, expected)| actual.abs_diff(expected) < 40),
            "wrong scheduled picture at second {second}: {found:?}, wanted {expected:?}"
        );
    }

    fn frequency(&self, second: usize, expected: f64) {
        let bytes = &self.pcm[second * 16000..(second + 1) * 16000];
        let samples: Vec<i16> = bytes
            .chunks_exact(2)
            .map(|value| i16::from_le_bytes([value[0], value[1]]))
            .collect();
        let crossings = samples
            .windows(2)
            .filter(|pair| pair[0] <= 0 && pair[1] > 0)
            .count();
        let measured = f64::from(u32::try_from(crossings).unwrap());
        assert!(
            (measured - expected).abs() < 45.0,
            "wrong audio at second {second}: {measured}, wanted {expected}"
        );
    }

    pub fn timeline(&self, normalize: bool) -> f64 {
        for (second, color, tone) in [
            (5, [0, 0, 255], 880.0),
            (8, [0, 255, 0], 220.0),
            (12, [255, 0, 0], 1760.0),
            (22, [255, 0, 0], 1760.0),
            (27, [0, 255, 0], 220.0),
        ] {
            self.color(second, color);
            self.frequency(second, tone);
        }
        assert!(
            self.pixel(5, 250, 130)[0] < 40,
            "first pinned subtitle-off was ignored"
        );
        let caption = self.pixel(12, 250, 130);
        assert!(
            caption.iter().all(|value| *value > 180),
            "later pinned Dutch caption was lost: {caption:?}"
        );
        let canvas = self.pixel(12, 24, 24);
        assert!(
            canvas[0] > 180 && canvas[1] > 180 && canvas[2] < 40,
            "later canvas was not decoded: {canvas:?}"
        );
        let first = self.pixel(5, 24, 24);
        assert!(
            first[0] < 40 && first[1] < 40 && first[2] > 180,
            "later canvas leaked into first occurrence"
        );
        self.color(0, if normalize { [0, 0, 255] } else { [0, 255, 0] });
        self.frequency(0, if normalize { 880.0 } else { 220.0 });
        let early = self.clock_ink(12);
        let later = self.clock_ink(18);
        assert!(
            early > 20 && later > early * 3 / 2,
            "Mirror broadcast-clock refresh did not advance (X then XX): {early}, {later}"
        );
        self.cadence(12.0, 18.0, if normalize { 25.0 } else { 30.0 })
    }

    fn clock_ink(&self, second: usize) -> usize {
        (60..90)
            .flat_map(|y| (80..180).map(move |x| (x, y)))
            .filter(|(x, y)| self.pixel(second, *x, *y).iter().all(|value| *value > 180))
            .count()
    }

    pub fn empty(&self) {
        for second in [0, 5, 12, 22, 27] {
            self.color(second, [0, 255, 0]);
            self.frequency(second, 220.0);
        }
    }

    pub fn credits(&self, scenario: Scenario) {
        self.frequency(0, 880.0);
        self.frequency(8, 880.0);
        if scenario.finite_producer() {
            self.color(0, [0, 0, 255]);
        }
        let first = self.pixel(0, 250, 130);
        let later = self.pixel(4, 250, 130);
        if scenario.pinned() || scenario.custom() {
            for color in [first, later] {
                assert!(
                    color[0] < 40 && color[1] > 180 && color[2] > 180,
                    "generated credits replaced an exact/Custom Dutch caption: {color:?}"
                );
            }
        } else {
            assert!(
                first.iter().all(|component| *component > 180),
                "joined active credits were late or double-sought: {first:?}"
            );
            assert!(
                later[0] < 40 && later[1] < 40 && later[2] > 180,
                "expired joined credits repeated or ended late: {later:?}"
            );
        }
        for second in [0, 1, 4, 8] {
            let color = self.pixel(second, 250, 130);
            assert!(
                !(color[0] > 180 && color[1] < 40 && color[2] < 40),
                "a pre-join expired cue reappeared"
            );
        }
        self.color(13, [0, 255, 0]);
        self.frequency(13, 220.0);
        self.color(27, [0, 255, 0]);
        self.frequency(27, 220.0);
    }

    fn cadence(&self, begin: f64, end: f64, expected: f64) -> f64 {
        let mut pts: Vec<f64> = self.packets["packets"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|packet| packet["pts_time"].as_str()?.parse().ok())
            .collect();
        pts.sort_by(f64::total_cmp);
        let zero = *pts.first().unwrap();
        let count = pts
            .iter()
            .filter(|pts| **pts - zero >= begin && **pts - zero < end)
            .count();
        let measured = f64::from(u32::try_from(count).unwrap()) / (end - begin);
        assert!(
            (measured - expected).abs() < 1.0,
            "real packet cadence {measured} disagrees with selected profile/source {expected}"
        );
        measured
    }
}
