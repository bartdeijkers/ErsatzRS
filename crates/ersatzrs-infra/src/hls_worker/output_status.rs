//! Segment provenance survives the worker moving ahead of the browser.

use std::collections::{HashMap, HashSet, VecDeque};

use ersatzrs_core::hls::HlsSegmentOutput;

const MAX_SEGMENT_REPORTS: usize = 256;

#[derive(Default)]
pub(super) struct SegmentOutputs {
    completed: HashMap<String, HlsSegmentOutput>,
    order: VecDeque<String>,
    active: Option<(HashSet<String>, HlsSegmentOutput)>,
}

impl SegmentOutputs {
    pub(super) fn begin(&mut self, baseline: Option<HashSet<String>>, error: Option<String>) {
        // A failed baseline read cannot establish which process owns a segment.
        self.active = baseline.map(|baseline| (baseline, HlsSegmentOutput { error }));
    }

    pub(super) fn get(&self, segment: &str, served: &HashSet<String>) -> Option<HlsSegmentOutput> {
        if !served.contains(segment) {
            return None;
        }
        if let Some(output) = self.completed.get(segment) {
            return Some(output.clone());
        }
        self.active
            .as_ref()
            .and_then(|(baseline, output)| (!baseline.contains(segment)).then(|| output.clone()))
    }

    pub(super) fn finish(&mut self, served: Option<&HashSet<String>>) {
        if let (Some((baseline, output)), Some(served)) = (self.active.take(), served) {
            for segment in served.difference(&baseline) {
                self.completed.insert(segment.clone(), output.clone());
                self.order.push_back(segment.clone());
            }
        }
        while self.order.len() > MAX_SEGMENT_REPORTS {
            if let Some(segment) = self.order.pop_front() {
                self.completed.remove(&segment);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segments(values: &[&str]) -> HashSet<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn buffered_slate_keeps_original_failure_after_next_programme_starts() {
        let mut outputs = SegmentOutputs::default();
        outputs.begin(Some(HashSet::new()), None);
        let ordinary = segments(&["000.ts"]);
        outputs.finish(Some(&ordinary));
        outputs.begin(
            Some(ordinary.clone()),
            Some("drawvg: Invalid argument".into()),
        );
        let slate = segments(&["000.ts", "001.ts"]);
        assert!(outputs.get("000.ts", &slate).unwrap().error.is_none());
        assert_eq!(
            outputs.get("001.ts", &slate).unwrap().error.as_deref(),
            Some("drawvg: Invalid argument")
        );
        outputs.finish(Some(&slate));
        outputs.begin(Some(slate.clone()), None);
        let next = segments(&["000.ts", "001.ts", "002.ts"]);
        assert!(outputs.get("001.ts", &next).unwrap().error.is_some());
        assert!(outputs.get("002.ts", &next).unwrap().error.is_none());
        assert!(outputs.get("missing.ts", &next).is_none());
        assert!(outputs.get("001.ts", &segments(&["002.ts"])).is_none());
    }

    #[test]
    fn unknown_baseline_and_retired_segments_do_not_claim_healthy_playback() {
        let mut outputs = SegmentOutputs::default();
        let served = segments(&["001.ts"]);
        outputs.begin(None, Some("failure".into()));
        assert!(outputs.get("001.ts", &served).is_none());
        outputs.finish(None);
        for index in 0..300 {
            outputs.begin(Some(HashSet::new()), None);
            outputs.finish(Some(&segments(&[&format!("{index:03}.ts")])));
        }
        assert_eq!(outputs.completed.len(), MAX_SEGMENT_REPORTS);
        assert!(outputs.get("000.ts", &segments(&["000.ts"])).is_none());
    }
}
