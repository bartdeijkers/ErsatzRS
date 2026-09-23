# Classic chapter expressions

Classic filler presets use an expression to select chapter boundaries for breaks.
This page describes the deterministic expression profile.
Cross-platform acceptance remains open under UCU-02; consult
[Classic scheduling parity](../progress/parity/classic-scheduling.toml) for remaining evidence.

## Selection rules

The schedule must expose chapters through a mid-roll preset. Each chapter except
the last supplies one candidate point at its source end time. Only a boolean
`true` selects a point; numeric, string and null results do not select it.
A blank expression uses every available boundary. Without effective chapters,
expressions are not evaluated.

Rejected boundaries merge into the next selected interval. The final content
interval remains after the last selected break. If a valid mid-roll expression
selects no boundaries, the programme remains whole. A mid-roll Pad preset then
pads after the programme.
Pre/post-roll Pad expressions are evaluated independently when effective chapters
exist; a no-match result places their padding after the programme.

## Parameters

Names are case-sensitive. Time values are seconds in the original source,
not elapsed channel time after inserted filler.

| Parameter | Value |
|---|---|
| `num` | Candidate number, starting at 1. |
| `point` | Candidate chapter end in source seconds. |
| `title` | Candidate chapter title, or an empty string. |
| `total_points` | Number of candidate boundaries; one fewer than the chapter count. |
| `total_duration` | Original programme duration in seconds. |
| `total_progress` | Candidate source end divided by the final chapter end. |
| `remaining_duration` | Original programme duration minus `point`. |
| `matched_points` | Number of earlier candidates selected by this expression. |
| `last_mid_filler` | Seconds since the previous selected point. Before any match, the origin is the earliest chapter start minus 99,999 seconds. |

Unknown parameter names return null. They do not read application state, files,
network resources or environment variables.

Examples:

```text
num = 2
num % 2 = 0
title = 'Opening'
point >= 600 and remaining_duration >= 300 and last_mid_filler >= 600
```

The last example waits until at least ten source minutes, leaves at least five
minutes of programme, and spaces selected breaks at least ten source minutes apart.

## Values and comparisons

The grammar and retained operators/functions follow NCalcSync 5.12, with the
explicit deterministic differences below. Conditionals and boolean operators
short-circuit. Function names are case-sensitive: for example, `if`, `ifs`, `in`,
`Abs`, `Min`, `Max`, `Round`, `Sqrt` and `Pow` retain their upstream casing.

String equality and ordering use ICU4X root collation: case is ignored, accents
remain significant, and canonically equivalent Unicode text compares equally.
The host locale and admin UI language do not change a saved expression's result.
All-string membership uses this comparison; mixed-value membership retains its
type rules. String containment remains case-sensitive.

LIKE is a separate pattern operation. `%` matches zero or more UTF-16 code units;
`_` matches one. A supplementary Unicode character therefore occupies two `_`
positions. Matching uses invariant case rules and retains NCalc's terminal-newline
behavior; it does not perform collation-based pattern matching.

Numeric literals use a decimal point. String-to-real conversion uses invariant
`.` decimals and `,` grouping: `'1,000' + 0` is 1000, and `'1.000' + 0` is 1 on
every host. String-to-integer conversion does not accept grouped or fractional
strings. Arithmetic precedence, numeric type conversion and overflow behavior
otherwise retain the versioned NCalc contract.

## Date and elapsed-time literals

Enclose these values in `#` characters:

| Value | Format | Example |
|---|---|---|
| Date | `M/d/yyyy` | `#9/23/2026#` |
| Date and time | `M/d/yyyy H:mm:ss[.fraction]` | `#9/23/2026 8:30:00#` |
| Elapsed time | `H:mm:ss[.fraction]` | `#25:00:00.5#` |
| ISO date | `yyyy-MM-dd` | `#2026-09-23#` |
| ISO date and time | `yyyy-MM-ddTHH:mm:ss[.fraction]` | `#2026-09-23T08:30:00#` |

Brackets above mean an optional suffix. Fractions contain one to seven digits,
with 100-nanosecond precision. Years are four digits from 0001 through 9999.
Slash-format months/days and date/time hours allow one or two digits; ISO fields
use the shown widths. Minutes and seconds require two digits from 00 through 59.
Date/time hours range from 00 through 23. Elapsed hours are nonnegative and the
complete duration must fit a signed 64-bit tick count. Invalid calendar dates
are rejected.

Localized separators, month names, two-digit years and timezone suffixes are not
supported. These values do not implicitly convert between timezones.

## Limits and failure behavior

Evaluated expressions allow at most 65,536 UTF-8 bytes and 32 nested groups,
lists or function calls. Parentheses inside strings or bracketed parameter names
do not count. Blank expressions and cases without evaluable chapter points retain
their bypass behavior. Valid UTF-16 surrogate pairs are supported in escaped
literals; unpaired surrogate escapes are rejected. Literal validation occurs
before evaluation, including literals in branches that would otherwise be skipped.
Function calls and boolean/conditional execution still short-circuit.

Malformed, unsupported or over-limit expressions fail the complete playout
build. Existing persisted planning remains intact; the expression is not silently
converted to `false`, and no partial replacement is saved. Errors omit the saved
expression text and chapter titles.

When importing rules from ErsatzTV, review rules that rely on local date formats,
locale-specific string casing or locale-specific numeric strings. Their meaning
is governed by this profile, not by the source machine's culture.
