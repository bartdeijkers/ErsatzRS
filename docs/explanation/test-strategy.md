# Test Strategy

ErsatzRS relies on a layered test strategy because the highest-risk behavior is not
only pure Rust logic. Generated FFmpeg arguments, filter graphs, scheduling
timelines, migrations, and protocol output are all project contracts.

Unit tests protect small invariants. Integration tests protect crate
collaboration. Snapshot tests protect generated output from unreviewed drift.
End-to-end tests protect the main user workflows once the web host exists.

There is no coverage percentage gate. A percentage can look precise while
missing the behavior that matters most. Reviewers should instead ask whether the
test level matches the risk.

Property tests are optional by default and encouraged for scheduling engines,
timeline math, and parsers once their APIs stabilize.
