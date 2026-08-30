//! Shared `MiniJinja` configuration for persisted user templates.

use minijinja::value::{Value, ValueKind};
use minijinja::{escape_formatter, Environment};

/// Build an environment that preserves scalar spellings used before
/// `MiniJinja` 2.22 while delegating every other value to its escape-aware
/// formatter.
///
/// XMLTV, dynamic-text, and graphics templates are persisted user contracts.
/// Letting a dependency update silently change `true`/`false`/`none` to
/// `True`/`False`/`None` would alter generated XML and overlay text without a
/// template edit.
pub(crate) fn environment() -> Environment<'static> {
    let mut environment = Environment::new();
    environment.set_formatter(|output, state, value| match value.kind() {
        ValueKind::Bool => {
            let spelling = Value::from(if value.is_true() { "true" } else { "false" });
            escape_formatter(output, state, &spelling)
        }
        ValueKind::None => escape_formatter(output, state, &Value::from("none")),
        _ => escape_formatter(output, state, value),
    });
    environment
}

#[cfg(test)]
mod tests {
    use minijinja::context;

    use super::*;

    #[test]
    fn preserves_pre_2_22_scalar_spelling() {
        let environment = environment();

        assert_eq!(
            environment
                .render_str(
                    "{{ enabled }}|{{ disabled }}|{{ empty }}",
                    context! {
                        enabled => true,
                        disabled => false,
                        empty => (),
                    }
                )
                .unwrap(),
            "true|false|none"
        );
    }
}
