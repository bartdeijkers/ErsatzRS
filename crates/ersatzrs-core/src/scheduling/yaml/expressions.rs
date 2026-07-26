// ── count expression evaluator ────────────────────────────────────────────────
//
// Mirrors `ErsatzTV`'s `CountExpression.Evaluate` (NCalc-based).
// Supported: integer literals, `count` (enumerator length), `random`
// (deterministic value mod count), and `+`, `-`, `*`, `/`, `%` operators with
// parentheses. Returns `0` on any parse failure.

fn expr_skip_ws(chars: &[u8], pos: usize) -> usize {
    let mut p = pos;
    while p < chars.len() && chars[p] == b' ' {
        p += 1;
    }
    p
}

fn expr_parse_expr(chars: &[u8], pos: usize, count: i64, random: i64) -> (i64, usize) {
    let (mut left, mut pos) = expr_parse_term(chars, pos, count, random);
    loop {
        let p = expr_skip_ws(chars, pos);
        if p >= chars.len() {
            break;
        }
        match chars[p] {
            b'+' => {
                let (right, next) = expr_parse_term(chars, p + 1, count, random);
                left = left.saturating_add(right);
                pos = next;
            }
            b'-' => {
                let (right, next) = expr_parse_term(chars, p + 1, count, random);
                left = left.saturating_sub(right);
                pos = next;
            }
            _ => break,
        }
    }
    (left, pos)
}

fn expr_parse_term(chars: &[u8], pos: usize, count: i64, random: i64) -> (i64, usize) {
    let (mut left, mut pos) = expr_parse_factor(chars, pos, count, random);
    loop {
        let p = expr_skip_ws(chars, pos);
        if p >= chars.len() {
            break;
        }
        match chars[p] {
            b'*' => {
                let (right, next) = expr_parse_factor(chars, p + 1, count, random);
                left = left.saturating_mul(right);
                pos = next;
            }
            b'/' => {
                let (right, next) = expr_parse_factor(chars, p + 1, count, random);
                left = if right != 0 { left / right } else { 0 };
                pos = next;
            }
            b'%' => {
                let (right, next) = expr_parse_factor(chars, p + 1, count, random);
                left = if right != 0 { left % right } else { 0 };
                pos = next;
            }
            _ => break,
        }
    }
    (left, pos)
}

fn expr_parse_factor(chars: &[u8], pos: usize, count: i64, random: i64) -> (i64, usize) {
    let p = expr_skip_ws(chars, pos);
    if p >= chars.len() {
        return (0, p);
    }
    if chars[p] == b'-' {
        let (value, next) = expr_parse_factor(chars, p + 1, count, random);
        return (value.saturating_neg(), next);
    }
    if chars[p] == b'+' {
        return expr_parse_factor(chars, p + 1, count, random);
    }
    if chars[p] == b'(' {
        let (val, mut next) = expr_parse_expr(chars, p + 1, count, random);
        next = expr_skip_ws(chars, next);
        if next >= chars.len() || chars[next] != b')' {
            return (0, p);
        }
        next += 1;
        return (val, next);
    }
    let rest = &chars[p..];
    let is_not_ident = |c: &u8| !c.is_ascii_alphanumeric() && *c != b'_';
    if rest.starts_with(b"count") && rest.get(5).is_none_or(is_not_ident) {
        return (count, p + 5);
    }
    if rest.starts_with(b"random") && rest.get(6).is_none_or(is_not_ident) {
        return (random, p + 6);
    }
    // Integer literal (optional leading minus handled by term subtraction)
    let start = p;
    let mut end = p;
    while end < chars.len() && chars[end].is_ascii_digit() {
        end += 1;
    }
    if end > start {
        let s = std::str::from_utf8(&chars[start..end]).unwrap_or("0");
        return (s.parse::<i64>().unwrap_or(0), end);
    }
    (0, p)
}

/// Evaluate a count expression (see module-level helpers above).
pub(in crate::scheduling) fn eval_count_expression(
    expr: &str,
    enumerator_len: usize,
    seed: u64,
) -> usize {
    let count = i64::try_from(enumerator_len).unwrap_or(i64::MAX);
    let random: i64 = if enumerator_len > 0 {
        let r = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        i64::try_from(r % u64::try_from(enumerator_len).unwrap_or(u64::MAX)).unwrap_or(0)
    } else {
        0
    };
    let chars = expr.as_bytes();
    let (result, pos) = expr_parse_expr(chars, 0, count, random);
    if expr_skip_ws(chars, pos) != chars.len() {
        return 0;
    }
    usize::try_from(result.max(0)).unwrap_or(0)
}
