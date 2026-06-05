//! Self-contained boolean expression evaluator for the custom stream selector.
//!
//! ErsatzTV evaluates `audio_condition` / `subtitle_condition` /
//! `content_condition` expressions with `NCalc` (see
//! `$ERSATZTV_SRC/ErsatzTV.Core/FFmpeg/CustomStreamSelector.cs`). The conditions
//! are small boolean predicates over a fixed set of named parameters, e.g.
//! `channels >= 6 and default = false`. Rather than pull in a copyleft
//! expression crate, we normalize the user-facing syntax (see
//! [`normalize`]) and evaluate the result with the recursive-descent parser
//! below.
//!
//! The supported grammar, in order of increasing precedence, is:
//!
//! ```text
//! or         := and ( "||" and )*
//! and        := not ( "&&" not )*
//! not        := "!" not | comparison
//! comparison := unary ( ( "==" | "!=" | "<" | "<=" | ">" | ">=" ) unary )?
//! unary      := "-" unary | primary
//! primary    := "(" or ")" | integer | string | "true" | "false" | identifier
//! ```
//!
//! Any parse or type error makes the whole expression evaluate to `false`,
//! matching ErsatzTV's `expression.Evaluate() as bool? == true` fallback.

use std::collections::HashMap;

/// A value that a condition parameter or literal can hold.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ConditionValue {
    Int(i64),
    Str(String),
    Bool(bool),
}

impl From<i64> for ConditionValue {
    fn from(value: i64) -> Self {
        ConditionValue::Int(value)
    }
}

impl From<bool> for ConditionValue {
    fn from(value: bool) -> Self {
        ConditionValue::Bool(value)
    }
}

impl From<String> for ConditionValue {
    fn from(value: String) -> Self {
        ConditionValue::Str(value)
    }
}

impl From<&str> for ConditionValue {
    fn from(value: &str) -> Self {
        ConditionValue::Str(value.to_owned())
    }
}

/// Named parameters made available to a condition expression.
#[derive(Debug, Default)]
pub(crate) struct ConditionContext {
    values: HashMap<String, ConditionValue>,
}

impl ConditionContext {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Bind `name` to `value`, overwriting any previous binding.
    pub(crate) fn set(&mut self, name: impl Into<String>, value: impl Into<ConditionValue>) {
        self.values.insert(name.into(), value.into());
    }

    fn get(&self, name: &str) -> Option<&ConditionValue> {
        self.values.get(name)
    }
}

/// Evaluate the raw condition against `context`, returning `true` only when the
/// expression both parses and yields boolean `true`. Empty/whitespace-only
/// conditions and any parse or type error yield `false`.
pub(crate) fn evaluate(condition: &str, context: &ConditionContext) -> bool {
    let normalized = normalize(condition);
    let Some(tokens) = tokenize(&normalized) else {
        return false;
    };
    let mut parser = Parser::new(&tokens, context);
    let result = parser.parse_or();
    match result {
        Ok(ConditionValue::Bool(value)) if parser.at_end() => value,
        _ => false,
    }
}

/// Translate the user-facing NCalc-flavored syntax into the internal grammar:
/// `and`/`or`/`not` keywords become `&&`/`||`/`!`, a single `=` becomes `==`,
/// and single-quoted strings become double-quoted. String contents and bare
/// identifiers are lowercased so comparisons are case-insensitive, mirroring the
/// lowercased parameter values bound by the caller.
fn normalize(condition: &str) -> String {
    let mut normalized = String::with_capacity(condition.len());
    let chars = condition.chars().collect::<Vec<_>>();
    let mut index = 0usize;
    while index < chars.len() {
        let current = chars[index];
        if current == '\'' || current == '"' {
            let quote = current;
            normalized.push('"');
            index += 1;
            while index < chars.len() {
                let inner = chars[index];
                index += 1;
                if inner == quote {
                    break;
                }
                if inner == '"' || inner == '\\' {
                    normalized.push('\\');
                }
                normalized.push(inner.to_ascii_lowercase());
            }
            normalized.push('"');
            continue;
        }
        if current.is_alphabetic() || current == '_' {
            let start = index;
            index += 1;
            while index < chars.len() && (chars[index].is_alphanumeric() || chars[index] == '_') {
                index += 1;
            }
            let word = chars[start..index].iter().collect::<String>();
            match word.to_ascii_lowercase().as_str() {
                "and" => normalized.push_str("&&"),
                "or" => normalized.push_str("||"),
                "not" => normalized.push('!'),
                _ => normalized.push_str(&word),
            }
            continue;
        }
        if current == '='
            && chars.get(index + 1) != Some(&'=')
            && chars.get(index.wrapping_sub(1)) != Some(&'!')
            && chars.get(index.wrapping_sub(1)) != Some(&'>')
            && chars.get(index.wrapping_sub(1)) != Some(&'<')
        {
            normalized.push_str("==");
            index += 1;
            continue;
        }
        normalized.push(current);
        index += 1;
    }
    normalized
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    LParen,
    RParen,
    Not,
    And,
    Or,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Minus,
    Int(i64),
    Str(String),
    Ident(String),
}

/// Lex a double-quoted string literal. `index` must point at the opening quote
/// and is advanced past the closing quote. Returns `None` if the string is
/// unterminated or a trailing backslash escapes nothing.
fn lex_string(chars: &[char], index: &mut usize) -> Option<String> {
    *index += 1;
    let mut text = String::new();
    while *index < chars.len() {
        let inner = chars[*index];
        *index += 1;
        if inner == '\\' {
            text.push(*chars.get(*index)?);
            *index += 1;
            continue;
        }
        if inner == '"' {
            return Some(text);
        }
        text.push(inner);
    }
    None
}

/// Split the normalized expression into tokens, or `None` on an invalid
/// character or unterminated string.
fn tokenize(input: &str) -> Option<Vec<Token>> {
    let chars = input.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < chars.len() {
        let current = chars[index];
        match current {
            c if c.is_whitespace() => index += 1,
            '(' => {
                tokens.push(Token::LParen);
                index += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                index += 1;
            }
            '-' => {
                tokens.push(Token::Minus);
                index += 1;
            }
            '&' if chars.get(index + 1) == Some(&'&') => {
                tokens.push(Token::And);
                index += 2;
            }
            '|' if chars.get(index + 1) == Some(&'|') => {
                tokens.push(Token::Or);
                index += 2;
            }
            '=' if chars.get(index + 1) == Some(&'=') => {
                tokens.push(Token::Eq);
                index += 2;
            }
            '!' if chars.get(index + 1) == Some(&'=') => {
                tokens.push(Token::Ne);
                index += 2;
            }
            '!' => {
                tokens.push(Token::Not);
                index += 1;
            }
            '<' if chars.get(index + 1) == Some(&'=') => {
                tokens.push(Token::Le);
                index += 2;
            }
            '<' => {
                tokens.push(Token::Lt);
                index += 1;
            }
            '>' if chars.get(index + 1) == Some(&'=') => {
                tokens.push(Token::Ge);
                index += 2;
            }
            '>' => {
                tokens.push(Token::Gt);
                index += 1;
            }
            '"' => tokens.push(Token::Str(lex_string(&chars, &mut index)?)),
            c if c.is_ascii_digit() => {
                let start = index;
                while index < chars.len() && chars[index].is_ascii_digit() {
                    index += 1;
                }
                let literal = chars[start..index].iter().collect::<String>();
                let value = literal.parse::<i64>().ok()?;
                tokens.push(Token::Int(value));
            }
            c if c.is_alphabetic() || c == '_' => {
                let start = index;
                index += 1;
                while index < chars.len() && (chars[index].is_alphanumeric() || chars[index] == '_')
                {
                    index += 1;
                }
                let word = chars[start..index].iter().collect::<String>();
                tokens.push(Token::Ident(word));
            }
            _ => return None,
        }
    }
    Some(tokens)
}

struct Parser<'a> {
    tokens: &'a [Token],
    position: usize,
    context: &'a ConditionContext,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token], context: &'a ConditionContext) -> Self {
        Self {
            tokens,
            position: 0,
            context,
        }
    }

    fn at_end(&self) -> bool {
        self.position >= self.tokens.len()
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    fn advance(&mut self) -> Option<&Token> {
        let token = self.tokens.get(self.position);
        if token.is_some() {
            self.position += 1;
        }
        token
    }

    fn parse_or(&mut self) -> Result<ConditionValue, ()> {
        let mut left = self.parse_and()?;
        while self.peek() == Some(&Token::Or) {
            self.advance();
            let right = self.parse_and()?;
            left = ConditionValue::Bool(as_bool(&left)? || as_bool(&right)?);
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<ConditionValue, ()> {
        let mut left = self.parse_not()?;
        while self.peek() == Some(&Token::And) {
            self.advance();
            let right = self.parse_not()?;
            left = ConditionValue::Bool(as_bool(&left)? && as_bool(&right)?);
        }
        Ok(left)
    }

    fn parse_not(&mut self) -> Result<ConditionValue, ()> {
        if self.peek() == Some(&Token::Not) {
            self.advance();
            let value = self.parse_not()?;
            return Ok(ConditionValue::Bool(!as_bool(&value)?));
        }
        self.parse_comparison()
    }

    fn parse_comparison(&mut self) -> Result<ConditionValue, ()> {
        let left = self.parse_unary()?;
        let operator = match self.peek() {
            Some(Token::Eq) => Comparison::Eq,
            Some(Token::Ne) => Comparison::Ne,
            Some(Token::Lt) => Comparison::Lt,
            Some(Token::Le) => Comparison::Le,
            Some(Token::Gt) => Comparison::Gt,
            Some(Token::Ge) => Comparison::Ge,
            _ => return Ok(left),
        };
        self.advance();
        let right = self.parse_unary()?;
        Ok(ConditionValue::Bool(compare(operator, &left, &right)?))
    }

    fn parse_unary(&mut self) -> Result<ConditionValue, ()> {
        if self.peek() == Some(&Token::Minus) {
            self.advance();
            let value = self.parse_unary()?;
            return match value {
                ConditionValue::Int(number) => Ok(ConditionValue::Int(-number)),
                _ => Err(()),
            };
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<ConditionValue, ()> {
        let token = self.advance().cloned();
        match token {
            Some(Token::LParen) => {
                let value = self.parse_or()?;
                if self.advance() == Some(&Token::RParen) {
                    Ok(value)
                } else {
                    Err(())
                }
            }
            Some(Token::Int(number)) => Ok(ConditionValue::Int(number)),
            Some(Token::Str(text)) => Ok(ConditionValue::Str(text)),
            Some(Token::Ident(name)) => match name.as_str() {
                "true" => Ok(ConditionValue::Bool(true)),
                "false" => Ok(ConditionValue::Bool(false)),
                _ => self.context.get(&name).cloned().ok_or(()),
            },
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy)]
enum Comparison {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

fn as_bool(value: &ConditionValue) -> Result<bool, ()> {
    match value {
        ConditionValue::Bool(flag) => Ok(*flag),
        _ => Err(()),
    }
}

/// Compare two values. Equality is defined for matching types; ordering is only
/// defined for integers. Any other combination is a type error.
fn compare(
    operator: Comparison,
    left: &ConditionValue,
    right: &ConditionValue,
) -> Result<bool, ()> {
    match operator {
        Comparison::Eq => equals(left, right),
        Comparison::Ne => equals(left, right).map(|equal| !equal),
        Comparison::Lt | Comparison::Le | Comparison::Gt | Comparison::Ge => {
            let (ConditionValue::Int(lhs), ConditionValue::Int(rhs)) = (left, right) else {
                return Err(());
            };
            Ok(match operator {
                Comparison::Lt => lhs < rhs,
                Comparison::Le => lhs <= rhs,
                Comparison::Gt => lhs > rhs,
                Comparison::Ge => lhs >= rhs,
                Comparison::Eq | Comparison::Ne => unreachable!(),
            })
        }
    }
}

fn equals(left: &ConditionValue, right: &ConditionValue) -> Result<bool, ()> {
    match (left, right) {
        (ConditionValue::Int(lhs), ConditionValue::Int(rhs)) => Ok(lhs == rhs),
        (ConditionValue::Str(lhs), ConditionValue::Str(rhs)) => Ok(lhs == rhs),
        (ConditionValue::Bool(lhs), ConditionValue::Bool(rhs)) => Ok(lhs == rhs),
        _ => Err(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> ConditionContext {
        let mut context = ConditionContext::new();
        context.set("channels", 6_i64);
        context.set("default", false);
        context.set("forced", true);
        context.set("lang", "eng");
        context.set("id", 2_i64);
        context
    }

    #[test]
    fn evaluates_numeric_and_boolean_conjunction() {
        assert!(evaluate("channels >= 6 and default = false", &context()));
        assert!(!evaluate("channels >= 8 and default = false", &context()));
    }

    #[test]
    fn evaluates_string_equality_case_insensitively() {
        assert!(evaluate("lang = 'ENG'", &context()));
        assert!(!evaluate("lang = 'jpn'", &context()));
    }

    #[test]
    fn evaluates_boolean_variable_and_negation() {
        assert!(evaluate("forced", &context()));
        assert!(evaluate("not default", &context()));
        assert!(!evaluate("not forced", &context()));
    }

    #[test]
    fn honors_parentheses_and_or_precedence() {
        assert!(evaluate("default or (channels = 6 and forced)", &context()));
        assert!(!evaluate(
            "default or (channels = 6 and not forced)",
            &context()
        ));
    }

    #[test]
    fn supports_inequality_and_negative_literals() {
        assert!(evaluate("id != 3", &context()));
        assert!(evaluate("id > -1", &context()));
    }

    #[test]
    fn unknown_parameter_or_garbage_yields_false() {
        assert!(!evaluate("missing = 1", &context()));
        assert!(!evaluate("channels >=", &context()));
        assert!(!evaluate("", &context()));
        assert!(!evaluate("channels >= 'eng'", &context()));
    }
}
