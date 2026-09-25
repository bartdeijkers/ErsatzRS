use super::{
    profile,
    value::{unescape, Value},
    ExpressionError, Parameters, Result, Rule,
};
use pest::iterators::Pair;
use pest::pratt_parser::{Assoc, Op, PrattParser};
use std::{cell::RefCell, cmp::Ordering, sync::LazyLock};

pub(super) fn evaluate(pair: Pair<'_, Rule>, parameters: &Parameters<'_>) -> Result<Value> {
    match pair.as_rule() {
        Rule::number => Value::number(pair.as_str()),
        Rule::boolean => Ok(Value::Boolean(pair.as_str().eq_ignore_ascii_case("true"))),
        Rule::string => unescape(pair.as_str()).map(Value::Text),
        Rule::guid => uuid::Uuid::parse_str(pair.as_str())
            .map(Value::Guid)
            .map_err(|_| ExpressionError),
        Rule::temporal => Value::temporal(pair.as_str()),
        Rule::parameter => {
            let text = pair.as_str();
            let name = if text.starts_with(['[', '{']) {
                &text[1..text.len() - 1]
            } else {
                text
            };
            Ok(parameters.get(name))
        }
        Rule::root => evaluate(pair.into_inner().next().ok_or(ExpressionError)?, parameters),
        Rule::collection => {
            let mut children = pair.into_inner();
            let Some(first) = children.next() else {
                return Ok(Value::List(Vec::new()));
            };
            let value = evaluate(first, parameters)?;
            if children.peek().is_none() {
                return Ok(value);
            }
            let mut values = vec![value];
            for child in children {
                values.push(evaluate(child, parameters)?);
            }
            Ok(Value::List(values))
        }
        Rule::function => function(pair, parameters),
        Rule::expression => {
            let mut children = pair.into_inner();
            let condition = evaluate(children.next().ok_or(ExpressionError)?, parameters)?;
            if let Some(yes) = children.next() {
                let no = children.next().ok_or(ExpressionError)?;
                evaluate(if condition.boolean()? { yes } else { no }, parameters)
            } else {
                Ok(condition)
            }
        }
        Rule::unary => unary(pair, parameters),
        Rule::factorial => {
            let mut children = pair.into_inner();
            let mut value = evaluate(children.next().ok_or(ExpressionError)?, parameters)?;
            for _ in children {
                value = factorial(&value)?;
            }
            Ok(value)
        }
        Rule::exponential => {
            let mut children = pair.into_inner();
            let mut operands = vec![children.next().ok_or(ExpressionError)?];
            while children.next().is_some() {
                operands.push(children.next().ok_or(ExpressionError)?);
            }
            // NCalc 6.2's exponential AST builder retains only the first
            // three operands for chains longer than three. Preserve that
            // versioned behavior, rather than silently changing saved rules.
            operands.truncate(3);
            let mut values = operands
                .into_iter()
                .map(|p| evaluate(p, parameters))
                .collect::<Result<Vec<_>>>()?;
            let mut value = values.pop().ok_or(ExpressionError)?;
            while let Some(left) = values.pop() {
                value = Value::Real(left.real()?.powf(value.real()?));
            }
            Ok(value)
        }
        Rule::binary => evaluate_binary(pair, parameters),
        _ => Err(ExpressionError),
    }
}

fn unary(pair: Pair<'_, Rule>, parameters: &Parameters<'_>) -> Result<Value> {
    let mut children: Vec<_> = pair.into_inner().collect();
    let mut value = evaluate(children.pop().ok_or(ExpressionError)?, parameters)?;
    for operator in children.into_iter().rev() {
        value = match operator.as_str().to_ascii_lowercase().as_str() {
            "!" | "not" => Value::Boolean(!value.boolean()?),
            "~" => Value::Unsigned(!value.unsigned()?),
            "-" => match value {
                Value::Real(number) => Value::Real(-number),
                other => arithmetic("-", Value::Int(0), other)?,
            },
            _ => return Err(ExpressionError),
        };
    }
    Ok(value)
}

static PRECEDENCE: LazyLock<PrattParser<Rule>> = LazyLock::new(|| {
    PrattParser::new()
        .op(Op::infix(Rule::or_op, Assoc::Left))
        .op(Op::infix(Rule::and_op, Assoc::Left))
        .op(Op::infix(Rule::bor_op, Assoc::Left))
        .op(Op::infix(Rule::xor_op, Assoc::Left))
        .op(Op::infix(Rule::band_op, Assoc::Left))
        .op(Op::infix(Rule::eq_op, Assoc::Left))
        .op(Op::infix(Rule::rel_op, Assoc::Left))
        .op(Op::infix(Rule::shift_op, Assoc::Left))
        .op(Op::infix(Rule::add_op, Assoc::Left))
        .op(Op::infix(Rule::mul_op, Assoc::Left))
});

enum Node<'a> {
    Leaf(Pair<'a, Rule>),
    Binary {
        left: usize,
        operator: Pair<'a, Rule>,
        right: usize,
    },
}

enum Task<'a> {
    Visit(usize),
    LeftReady { operator: &'a str, right: usize },
    Apply { operator: &'a str, left: Value },
}

/// An arena and explicit evaluation stack keep long left-associative rules
/// bounded without losing lazy boolean evaluation. The Pratt parser itself
/// recurses by precedence, not once per node of a left-associative chain.
fn evaluate_binary(pair: Pair<'_, Rule>, parameters: &Parameters<'_>) -> Result<Value> {
    let arena = RefCell::new(Vec::new());
    let push = |node| {
        let mut nodes = arena.borrow_mut();
        let index = nodes.len();
        nodes.push(node);
        index
    };
    let root = PRECEDENCE
        .map_primary(|leaf| push(Node::Leaf(leaf)))
        .map_infix(|left, operator, right| {
            push(Node::Binary {
                left,
                operator,
                right,
            })
        })
        .parse(pair.into_inner());
    let nodes = arena.into_inner();
    let mut tasks = vec![Task::Visit(root)];
    let mut values = Vec::new();
    while let Some(task) = tasks.pop() {
        match task {
            Task::Visit(index) => match &nodes[index] {
                Node::Leaf(leaf) => values.push(evaluate(leaf.clone(), parameters)?),
                Node::Binary {
                    left,
                    operator,
                    right,
                } => {
                    tasks.push(Task::LeftReady {
                        operator: operator.as_str(),
                        right: *right,
                    });
                    tasks.push(Task::Visit(*left));
                }
            },
            Task::LeftReady { operator, right } => {
                let left = values.pop().ok_or(ExpressionError)?;
                let normalized = operator.to_ascii_lowercase();
                let short = match normalized.as_str() {
                    "and" | "&&" => !left.boolean()?,
                    "or" | "||" => left.boolean()?,
                    _ => false,
                };
                if short {
                    values.push(Value::Boolean(left.boolean()?));
                } else {
                    tasks.push(Task::Apply { operator, left });
                    tasks.push(Task::Visit(right));
                }
            }
            Task::Apply { operator, left } => {
                let right = values.pop().ok_or(ExpressionError)?;
                let normalized = operator.to_ascii_lowercase();
                values.push(match normalized.as_str() {
                    "and" | "&&" | "or" | "||" => Value::Boolean(right.boolean()?),
                    _ => binary(&normalized, left, right)?,
                });
            }
        }
    }
    values.pop().ok_or(ExpressionError)
}

fn binary(operator: &str, left: Value, right: Value) -> Result<Value> {
    match operator {
        "=" | "==" | "!=" | "<>" | ">" | ">=" | "<" | "<=" => {
            let nan_operand = matches!(&left, Value::Real(value) if value.is_nan())
                || matches!(&right, Value::Real(value) if value.is_nan());
            let result = if nan_operand {
                matches!(operator, "!=" | "<>")
            } else if left.is_blank() == right.is_blank() {
                let order = left.compare(&right)?;
                match operator {
                    "=" | "==" => order == Ordering::Equal,
                    "!=" | "<>" => order != Ordering::Equal,
                    ">" => order == Ordering::Greater,
                    ">=" => order != Ordering::Less,
                    "<" => order == Ordering::Less,
                    _ => order != Ordering::Greater,
                }
            } else {
                matches!(operator, "!=" | "<>")
            };
            Ok(Value::Boolean(result))
        }
        "like" | "not like" => {
            let matched = if matches!(left, Value::Null) || matches!(right, Value::Null) {
                false
            } else {
                like(&left.text(), &right.text())?
            };
            Ok(Value::Boolean(matched != (operator == "not like")))
        }
        "in" | "not in" => Ok(Value::Boolean(
            contains(&left, &right)? != (operator == "not in"),
        )),
        "&" => Ok(Value::Unsigned(left.unsigned()? & right.unsigned()?)),
        "|" => Ok(Value::Unsigned(left.unsigned()? | right.unsigned()?)),
        "^" => Ok(Value::Unsigned(left.unsigned()? ^ right.unsigned()?)),
        "<<" | ">>" => {
            let shift = right.int32()?.unsigned_abs() & 63;
            // CLR masks the two's-complement shift count, including negatives.
            let shift = if right.int32()? < 0 {
                64_u32.wrapping_sub(shift) & 63
            } else {
                shift
            };
            Ok(Value::Unsigned(if operator == "<<" {
                left.unsigned()?.wrapping_shl(shift)
            } else {
                left.unsigned()?.wrapping_shr(shift)
            }))
        }
        _ => arithmetic(operator, left, right),
    }
}

fn contains(left: &Value, right: &Value) -> Result<bool> {
    match right {
        Value::Null => Err(ExpressionError),
        Value::Text(text) => {
            let sought = left.text();
            Ok(if sought.is_empty() {
                text.is_empty()
            } else {
                text.contains(&sought)
            })
        }
        Value::List(values) if values.iter().all(|v| matches!(v, Value::Text(_))) => {
            let sought = left.text();
            for value in values {
                if profile::compare(&value.text(), &sought)? == Ordering::Equal {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Value::List(values) => Ok(values.iter().any(|v| membership_equal(v, left))),
        value => Ok(membership_equal(value, left)),
    }
}

fn membership_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, _) | (_, Value::Null) => false,
        (Value::Int(n), Value::Text(s)) | (Value::Text(s), Value::Int(n)) => n.to_string() == *s,
        (Value::Real(a), Value::Real(b)) if a.is_nan() && b.is_nan() => true,
        _ => a == b,
    }
}

fn numeric(value: Value) -> Result<Value> {
    match value {
        Value::Text(_) => value.real().map(Value::Real),
        Value::Int(_) | Value::Long(_) | Value::Unsigned(_) | Value::Real(_) => Ok(value),
        _ => Err(ExpressionError),
    }
}

fn arithmetic(operator: &str, left: Value, right: Value) -> Result<Value> {
    if matches!(left, Value::Null) || matches!(right, Value::Null) {
        return Ok(Value::Null);
    }
    if operator == "+" {
        if let (Value::Text(a), Value::Text(b)) = (&left, &right) {
            if profile::real(a).is_err() || profile::real(b).is_err() {
                return Ok(Value::Text(format!("{a}{b}")));
            }
        }
    }
    let (left, right) = (numeric(left)?, numeric(right)?);
    if operator == "/" {
        return Ok(Value::Real(left.real()? / right.real()?));
    }
    match (&left, &right) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(match operator {
            "+" => a.wrapping_add(*b),
            "-" => a.wrapping_sub(*b),
            "*" => a.wrapping_mul(*b),
            "%" => a.checked_rem(*b).ok_or(ExpressionError)?,
            _ => return Err(ExpressionError),
        })),
        (Value::Long(_) | Value::Int(_), Value::Long(_) | Value::Int(_)) => {
            let convert = |v: &Value| match v {
                Value::Long(n) => *n,
                Value::Int(n) => i64::from(*n),
                _ => unreachable!(),
            };
            let (a, b) = (convert(&left), convert(&right));
            Ok(Value::Long(match operator {
                "+" => a.wrapping_add(b),
                "-" => a.wrapping_sub(b),
                "*" => a.wrapping_mul(b),
                "%" => a.checked_rem(b).ok_or(ExpressionError)?,
                _ => return Err(ExpressionError),
            }))
        }
        (Value::Unsigned(_), Value::Unsigned(_) | Value::Int(_) | Value::Long(_))
        | (Value::Int(_) | Value::Long(_), Value::Unsigned(_)) => {
            let (a, b) = (left.unsigned()?, right.unsigned()?);
            Ok(Value::Unsigned(match operator {
                "+" => a.wrapping_add(b),
                "-" => a.wrapping_sub(b),
                "*" => a.wrapping_mul(b),
                "%" => a.checked_rem(b).ok_or(ExpressionError)?,
                _ => return Err(ExpressionError),
            }))
        }
        _ => {
            let (a, b) = (left.real()?, right.real()?);
            Ok(Value::Real(match operator {
                "+" => a + b,
                "-" => a - b,
                "*" => a * b,
                "%" => a % b,
                _ => return Err(ExpressionError),
            }))
        }
    }
}

fn factorial(value: &Value) -> Result<Value> {
    let n = value.real()?;
    if n < 0.0 {
        return Err(ExpressionError);
    }
    // All supported numeric types have reached their final overflow result by
    // 171!, so enormous operands never cause an unbounded evaluation loop.
    match value {
        Value::Int(n) => Ok(Value::Int(
            (2..=(*n).min(34)).fold(1_i32, i32::wrapping_mul),
        )),
        Value::Long(n) => Ok(Value::Long(
            (2..=(*n).min(66)).fold(1_i64, i64::wrapping_mul),
        )),
        Value::Real(_) => {
            if n >= 171.0 {
                return Ok(Value::Real(f64::INFINITY));
            }
            let mut result = 1.0;
            let mut i = 2.0;
            while i <= n {
                result *= i;
                i += 1.0;
            }
            Ok(Value::Real(result))
        }
        _ => Err(ExpressionError),
    }
}

fn function(pair: Pair<'_, Rule>, parameters: &Parameters<'_>) -> Result<Value> {
    let mut children = pair.into_inner();
    let name = children.next().ok_or(ExpressionError)?.as_str();
    let arguments: Vec<_> = children.collect();
    let eval = |i: usize| evaluate(arguments.get(i).ok_or(ExpressionError)?.clone(), parameters);
    match name {
        "if" if arguments.len() == 3 => eval(if eval(0)?.boolean()? { 1 } else { 2 }),
        "ifs" if arguments.len() >= 3 && arguments.len() % 2 == 1 => {
            for i in (0..arguments.len() - 1).step_by(2) {
                if eval(i)?.boolean()? {
                    return eval(i + 1);
                }
            }
            eval(arguments.len() - 1)
        }
        "in" if arguments.len() >= 2 => {
            let value = eval(0)?;
            for i in 1..arguments.len() {
                if value.compare(&eval(i)?)? == Ordering::Equal {
                    return Ok(Value::Boolean(true));
                }
            }
            Ok(Value::Boolean(false))
        }
        _ => {
            let values = (0..arguments.len()).map(eval).collect::<Result<Vec<_>>>()?;
            math_function(name, &values)
        }
    }
}

fn math_function(name: &str, values: &[Value]) -> Result<Value> {
    if matches!(name, "Min" | "Max") && values.len() == 2 {
        return minimum_maximum(name, &values[0], &values[1]);
    }
    if values.len() == 1 {
        let x = values[0].real()?;
        let result = match name {
            "Abs" => x.abs(),
            "Acos" => x.acos(),
            "Asin" => x.asin(),
            "Atan" => x.atan(),
            "Ceiling" => x.ceil(),
            "Cos" => x.cos(),
            "Exp" => x.exp(),
            "Floor" => x.floor(),
            "Ln" => x.ln(),
            "Log10" => x.log10(),
            "Sin" => x.sin(),
            "Sqrt" => x.sqrt(),
            "Tan" => x.tan(),
            "Truncate" => x.trunc(),
            "Sign" => {
                return if x.is_nan() {
                    Err(ExpressionError)
                } else {
                    Ok(Value::Int(if x == 0.0 {
                        0
                    } else if x > 0.0 {
                        1
                    } else {
                        -1
                    }))
                };
            }
            _ => return Err(ExpressionError),
        };
        return Ok(Value::Real(result));
    }
    if values.len() != 2 {
        return Err(ExpressionError);
    }
    let (x, y) = (values[0].real()?, values[1].real()?);
    let result = match name {
        "Atan2" => x.atan2(y),
        "Log" => x.log(y),
        "Pow" => x.powf(y),
        "IEEERemainder" => {
            if (x == 0.0 && y.is_finite() && y != 0.0) || (x.is_finite() && y.is_infinite()) {
                x
            } else {
                x - y * (x / y).round_ties_even()
            }
        }
        "Round" => {
            let digits = values[1].int32()?;
            if !(0..=15).contains(&digits) {
                return Err(ExpressionError);
            }
            let scale = 10.0_f64.powi(digits);
            if x.abs() >= 1e16 {
                x
            } else {
                (x * scale).round_ties_even() / scale
            }
        }
        _ => return Err(ExpressionError),
    };
    Ok(Value::Real(result))
}

fn minimum_maximum(name: &str, left: &Value, right: &Value) -> Result<Value> {
    if matches!(left, Value::Null) {
        return Ok(right.clone());
    }
    if matches!(right, Value::Null) {
        return Ok(left.clone());
    }
    if matches!(left, Value::Boolean(_)) || matches!(right, Value::Boolean(_)) {
        return Ok(Value::Null);
    }
    let (left, right) = (numeric(left.clone())?, numeric(right.clone())?);
    let (a, b) = (left.real()?, right.real()?);
    if a.is_nan() || b.is_nan() {
        return Ok(Value::Real(f64::NAN));
    }
    let order = left.compare(&right)?;
    let selected = if (name == "Max" && order == Ordering::Less)
        || (name == "Min" && order == Ordering::Greater)
    {
        &right
    } else {
        &left
    };
    if matches!(left, Value::Real(_)) || matches!(right, Value::Real(_)) {
        return selected.real().map(Value::Real);
    }
    if matches!(left, Value::Unsigned(_)) || matches!(right, Value::Unsigned(_)) {
        // Promotion validates both operands, including negative signed values.
        left.unsigned()?;
        right.unsigned()?;
        return selected.unsigned().map(Value::Unsigned);
    }
    if matches!(left, Value::Long(_)) || matches!(right, Value::Long(_)) {
        return Ok(Value::Long(match selected {
            Value::Long(n) => *n,
            Value::Int(n) => i64::from(*n),
            _ => return Err(ExpressionError),
        }));
    }
    Ok(selected.clone())
}

/// .NET regex wildcards operate on UTF-16 code units. Map each code unit to
/// one private-use scalar before using Rust's linear-time regex engine. Keep
/// LF unchanged because `.` excludes LF, and `$` may precede a terminal LF.
fn like(text: &str, pattern: &str) -> Result<bool> {
    let encode = |unit: u16| -> Result<char> {
        if unit == 10 {
            Ok('\n')
        } else {
            char::from_u32(0xF_0000 + u32::from(unit)).ok_or(ExpressionError)
        }
    };
    let encoded_text = profile::like_units(text)
        .into_iter()
        .map(encode)
        .collect::<Result<String>>()?;
    let mut encoded_pattern = String::from("^");
    for unit in profile::like_units(pattern) {
        match unit {
            37 => encoded_pattern.push_str(".*"),
            95 => encoded_pattern.push('.'),
            _ => encoded_pattern.push_str(&regex::escape(&encode(unit)?.to_string())),
        }
    }
    encoded_pattern.push_str("(?:\\n)?\\z");
    regex::Regex::new(&encoded_pattern)
        .map_err(|_| ExpressionError)
        .map(|re| re.is_match(&encoded_text))
}
