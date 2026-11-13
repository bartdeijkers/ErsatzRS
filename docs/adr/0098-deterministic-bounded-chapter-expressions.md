# ADR 0098: Use deterministic bounded chapter expressions

## Status

Accepted 2026-09-23

Amended 2026-11-11 for UCU-16 row289: the maintainer chose full-content fallback
on invalid filler expressions, with actionable redacted diagnostics. Unknown
callback parameters now fail evaluation. This supersedes the original unknown-null
and whole-build-refusal outcome; culture, grammar and resource limits are retained.

Updated 2026-09-25 for upstream row 57: the retained NCalc behavior is checked
against NCalcSync 6.2.0. IEEE-754 NaN comparisons and the sign of zero from
`IEEERemainder` follow that release; the deterministic culture and resource
limits remain the accepted contract.

Upstream row 64 raises NCalcSync to 6.4.0. Its intervening 6.3.x releases
change numeric coercion in `in` comparisons; the accepted deterministic 6.2
profile remains in force. This row does not claim compatibility with later
NCalc package semantics. See the [NCalc changelog](https://github.com/ncalc/ncalc/blob/master/CHANGELOG.md).

Upstream row 97 raises NCalcSync to 7.1.0, adding null helpers/coalescing and
changing its .NET configuration API. The same accepted 6.2 profile remains in
force; this package update does not expand the saved-expression language.

The maintainer explicitly accepted this deterministic expression contract on
2026-09-23 and requested implementation. ADR 0097 accepts the separate no-match
padding correction; implementation and platform/runtime acceptance remain distinct.

## Implementation

Partial — the Classic chapter evaluator implements the accepted profile with
pinned ICU4X 2.2.0 compiled collation data, invariant numeric conversions,
validated Gregorian/elapsed-time
literals and eager literal validation. UCU-16 replaces expression build refusal
with a redacted full-content fallback; its verification is recorded separately
in milestone ucu-16. The parser
retains the NCalc 6.2 grammar, iterative binary evaluation, all nine callbacks,
65,536-byte text limit and 32-level nesting limit.

Focused Linux verification covers the five upstream chapter fixtures, 119
NCalcSync 6.2.0 oracle outcomes, explicit deterministic-profile regressions and
SQL preservation of prior playout state on rejected expressions. The profile
suite passes in separate processes under invariant, en-US, nl-NL and tr-TR
language environments. Dependency advisory, license, source and ban checks pass.
Native MPEG-TS/HLS smoke and decoded production-builder playback also pass for
Unicode-selected chapter breaks and persisted fractional fallback source offsets.
UCU-09 also verifies native Windows chapter fixtures and oracle outcomes, plus
persisted SQL atomic failure with redacted input. ARM/macOS application proof
remains unverified under the explicit maintainer-approved debt below.

On 2026-10-01 the maintainer approved explicit unverified ARM/macOS application
acceptance debt for UCU-09. Linux x64 and Windows x64 application checks remain
required; see `ucu-arm-macos-application-acceptance` in the catch-up parity record.

## Context

UCU-02 row 22 reviews ErsatzTV commit
`7e38297d6ed240f784d388696e687f781e550a71` (`#2867`). Its
`FillerExpression.FilterChapters` evaluates saved filler expressions through
NCalcSync 5.12.0 using `CaseInsensitiveStringComparer`. The expression decides
which chapter boundaries receive filler, so changing its result changes the
viewer's programme and break sequence.

NCalc's expression context takes `CultureInfo.CurrentCulture`. Comparisons use
`StringComparer.CurrentCultureIgnoreCase`; numeric conversions and date/time
literal parsing also consult culture. ErsatzTV's background worker reads
`BuildPlayout` messages containing the playout ID, mode and start time, with no
culture field. It does not apply the HTTP request's language selection to that
worker. The source therefore follows the worker's inherited startup/OS culture,
not the configured admin UI language. Reproducing that behavior requires an
explicit host-formatting-locale provider and verification of the underlying
platform globalization behavior.

A disposable exact-version NCalcSync 5.12.0 probe, with only `CurrentCulture`
changed, produced these Linux results. The first row compares a composed accent
with its decomposed equivalent.

| Saved expression | Invariant | en-US | nl-NL | tr-TR |
|---|---|---|---|---|
| `'é' = 'é'` | true | true | true | true |
| `'I' = 'i'` | true | true | true | false |
| `'1,000' + 0 = 1000` | true | true | false | false |

The pre-decision Rust implementation returned false, true and an error respectively.
The probe demonstrates both missing behavior and the fact that exact ambient
culture parity makes saved schedules sensitive to their deployment environment.
The probe established the need for an explicit deterministic contract.

The existing interpreter also refuses expressions above explicit complexity
bounds and rejects unpaired UTF-16 surrogate escapes. Both restrictions need an
explicit contract rather than an implicit claim of full NCalc compatibility.

## Decision

Adopt one deterministic expression profile for saved Classic filler expressions,
independent of host locale, admin UI language, request headers and worker thread.
Use the existing Rust evaluator with a versioned NCalcSync 6.2-compatible grammar,
operator precedence, lazy evaluation, built-in functions and value-conversion
behavior, subject to the following explicit differences.

NCalcSync 6.2 uses IEEE-754 comparisons for NaN: equality and ordered
comparisons are false; inequality is true. `IEEERemainder` preserves the sign
of a zero dividend. These replace the earlier 5.12 oracle outcomes.

### Culture and strings

Use ICU4X root-locale collation for string equality and ordering, with
`Strength::Secondary`, `CaseLevel::Off`, and
`AlternateHandling::NonIgnorable`. This preserves accent distinctions while
ignoring case and recognizing canonical Unicode equivalence. Root collation is a
specified product contract; it must not be described as exact equivalence to
every .NET culture or globalization backend.

Use the maintained `icu_collator` component from the existing ICU4X 2.2 dependency
family. Its official documentation defines these options. Select and lock the
concrete compatible version through the normal dependency/license checks; do not
load host ICU libraries or query OS collation APIs. A collation-data upgrade must
run the saved-expression compatibility corpus because data changes can affect
break selection.

Keep the distinct NCalc comparison rules distinct: all-string membership uses
the specified collator; mixed-value membership retains its existing type rules;
string containment remains case-sensitive. LIKE retains its UTF-16 code-unit
wildcards and terminal-newline behavior, using invariant case matching rather
than locale-dependent casing. LIKE must not substitute collation equality for
pattern matching. Turkish dotted/dotless-I, composed/decomposed accents and
supplementary characters require explicit regressions.

Reject unpaired UTF-16 surrogate literals rather than replacing them or passing
ill-formed text into Rust strings. Valid surrogate pairs continue to decode to
the corresponding character. This is a deliberate difference from .NET strings.

### Numbers and date/time literals

Numeric literals use `.` as the decimal separator; `,` remains the argument/list
separator. String-to-real conversion follows invariant .NET-style conversion
with `.` decimals and `,` grouping, including the applicable sign, exponent and
whitespace rules. For example, `'1,000' + 0` is 1000 everywhere, while
`'1.000' + 0` is 1 everywhere. String-to-integer conversions retain invariant
integer rules; this does not make grouped or fractional strings valid integer
inputs merely because real conversion accepts them.

Use this closed, culture-independent set of `#...#` literal formats:

| Kind | Accepted form |
|---|---|
| Gregorian date | `#M/d/yyyy#` |
| Gregorian date and time | `#M/d/yyyy H:mm:ss[.fraction]#` |
| Elapsed time | `#H:mm:ss[.fraction]#` |
| ISO Gregorian date | `#yyyy-MM-dd#` |
| ISO Gregorian date and time | `#yyyy-MM-ddTHH:mm:ss[.fraction]#` |

Here brackets denote an optional suffix, not literal characters. `M` and `d`
allow one or two digits; slash-format `H` allows one or two digits for a date/time
and any valid nonnegative elapsed-hour count for a duration. ISO fields use their
shown widths. Years contain exactly four digits and range from 0001 to 9999.
Minutes and seconds contain two digits and range from 00 to 59; date/time hours
range from 00 to 23. Optional fractions contain one through seven digits, using
100-nanosecond resolution. Elapsed times must fit a signed 64-bit tick count.
Invalid calendar dates are errors.

No localized date separators, ambiguous two-digit years, month names, timezone
suffixes or implicit timezone conversion are supported. This explicit format set
replaces ambient `.NET DateTime.Parse`/`TimeSpan.Parse` behavior; it is not a claim
to implement those APIs generally.

### Chapter selection and bounded failure

Preserve all nine callback names and meanings: `last_mid_filler`, `total_points`,
`total_duration`, `total_progress`, `remaining_duration`, `matched_points`,
`point`, `num`, and `title`. Preserve their original seconds/source-offset basis,
case-sensitive parameter names and the rule that
only a boolean true selects a point. Preserve stateful match counting, elapsed
time since the last selected point, merged chapter intervals and the trailing
content interval. The no-match padding behavior remains owned by ADR 0097.
An evaluated unknown parameter is an error; lazy branches still do not evaluate
unused callbacks.

Limit evaluated expression text to 65,536 UTF-8 bytes and syntactic parenthesis
nesting to 32, counting groups, lists and function calls but excluding quoted
text and bracketed parameter names. Preserve the upstream empty-expression,
empty-chapter and single-chapter bypasses, which perform no evaluation. Keep
binary-chain parsing/evaluation iterative and avoid unbounded factorial loops.
These bounds intentionally differ from accepting arbitrary valid NCalc input.

A malformed, unsupported or over-limit expression returns one unlabelled chapter
spanning the original minimum start and maximum end. Discard any partial point
selection. This disables internal breaks for that preset while preserving the
complete programme and the content duration used for padding. The Classic build
continues and publishes a complete replacement through the normal atomic path.
Report the filler preset name/id and schedule-item id in a diagnostic. Do not
include saved expression text, unknown parameter text or chapter titles.

## Accepted UCU-16 row289 amendment

The maintainer approved option2 (upstream fallback) on 2026-11-11.
Upstream c41969b3 now rejects unknown callback parameters and returns one
unlabelled chapter spanning the original start/end on invalid evaluation.
This disables internal breaks while retaining complete content duration for
clock-aligned padding. The Classic caller would publish that full-content
fallback with an actionable preset/schedule diagnostic, rather than discard the
programme or replace a whole schedule with a partial result.

The amendment keeps the deterministic culture/literal rules, lazy callback
evaluation, parser byte/nesting limits and redaction of expression text/chapter
titles. It changes the caller's failure outcome after a bounded parser error;
it does not relax parsing or allow unsupported input to execute. Unknown callback
names return an error instead of null. Repeated filtering must preserve input
chapter times and metadata. The original unknown-null/whole-build-refusal policy
is superseded. Milestone ucu-16 owns required source/runtime proof.

## Consequences

A saved expression has a stable meaning across deployment hosts and UI languages.
Users migrating culture-dependent rules may need to rewrite numeric strings,
date/time literals or casing assumptions to the documented profile. This is
visible parity debt until the complete accepted implementation is verified.

Acceptance requires differential tests for retained NCalc behavior, explicit
expected-result tests for every accepted difference, identical results under the
four culture environments above, and build/execution evidence across all six
supported platforms, subject to current explicit platform deferrals. Confirm
redacted whole-content fallback, retained padding, unchanged source chapters and
atomic publication for malformed, unsupported and over-limit expressions.
Publish the actual
supported profile in the expression reference after implementation. UCU-02's
placement, persistence and playback gates remain required; this ADR does not
close row 22 or the milestone.

## Alternatives Considered

- **Exact ambient .NET culture parity.** A viable alternative requiring a
  host-formatting-locale provider, culture data and compatible numeric/date/string
  behavior, plus platform-specific differential proof. It would preserve upstream
  environment sensitivity. The deterministic profile is preferred for
  saved scheduling rules; exact parity is not declared impossible.
- **Use the admin UI language as expression culture.** Does not match the
  background-worker source and would change scheduling behavior when a display
  preference changes.
- **Keep lowercase lexical comparison and Rust parsing as sufficient parity.**
  Rejected: the concrete probe results above demonstrate different selected
  chapter points and errors for valid upstream expressions.
- **Require invariant culture without specifying its operations.** Insufficient:
  .NET invariant comparison still recognizes canonical equivalence, numeric
  grouping and other behavior that ordinary Rust string/number parsing lacks.

## References

- [ADR 0001: Record architecture decisions](0001-record-architecture-decisions.md)
- [ADR 0097: Preserve primary content when padding matches no chapters](0097-preserve-primary-content-when-padding-matches-no-chapters.md)
- [Upstream catch-up plan](../../plans/upstream-catch-up.md)
- [Classic scheduling parity](../progress/parity/classic-scheduling.toml)
- [Classic chapter expression reference](../reference/classic-chapter-expressions.md)
- [Chapter expression implementation and oracle fixtures](../../crates/ersatzrs-core/src/scheduling/classic/chapter_expression/)
- [Pinned upstream FillerExpression](https://github.com/ErsatzTV/legacy/blob/7e38297d6ed240f784d388696e687f781e550a71/ErsatzTV.Core/Scheduling/FillerExpression.cs)
- [NCalc 5.12 expression context](https://github.com/ncalc/ncalc/blob/bd0afe72c1b5a5679255e91f902f35021acbb48a/src/NCalc.Core/ExpressionContextBase.cs)
- [NCalc 5.12 type comparison](https://github.com/ncalc/ncalc/blob/bd0afe72c1b5a5679255e91f902f35021acbb48a/src/NCalc.Core/Helpers/TypeHelper.cs)
- [NCalc 6.2 release history](https://github.com/ncalc/ncalc/blob/master/CHANGELOG.md)
- [ICU4X 2.2 collation options](https://docs.rs/icu_collator/2.2.0/icu_collator/)
- [.NET CurrentCulture and startup culture selection](https://learn.microsoft.com/en-us/dotnet/api/system.globalization.cultureinfo.currentculture)
