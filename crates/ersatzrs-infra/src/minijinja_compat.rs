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

    /// Pins the rendering contract of the shared environment across the
    /// `MiniJinja` 3 upgrade. Persisted XMLTV, dynamic-text, and graphics
    /// templates render through this environment, so a future dependency
    /// update that changes any of these spellings must fail here first and be
    /// recorded in the changelog instead of silently altering output.
    #[test]
    fn pins_rendered_output_contract_for_persisted_templates() {
        let environment = environment();
        let render = |source: &str| {
            environment
                .render_str(
                    source,
                    context! {
                        title => "Tom & Jerry <3>",
                        year => 2026,
                        rating => 7.5,
                        whole => 2.0,
                        names => vec!["A", "B"],
                        enabled => true,
                        empty => (),
                    },
                )
                .unwrap()
        };

        // Unchanged from MiniJinja 2: scalars, numbers, joins, escaping-free
        // `render_str` output (auto-escape is decided by template name), and
        // positive integer arithmetic used by bundled templates.
        assert_eq!(render("{{ title }}"), "Tom & Jerry <3>");
        assert_eq!(
            render("{{ year }}|{{ rating }}|{{ whole }}"),
            "2026|7.5|2.0"
        );
        assert_eq!(render("{{ names | join(', ') }}"), "A, B");
        assert_eq!(render("{{ enabled }}|{{ empty }}"), "true|none");
        assert_eq!(
            render("{{ 1080 // 20 }}|{{ 1080 / 20 }}|{{ 7 % 3 }}"),
            "54|54.0|1"
        );
        assert_eq!(render("{{ '%02d' | format(4) }}"), "04");
        assert_eq!(render("{{ 1234567.5 }}"), "1234567.5");

        // Accepted MiniJinja 3 changes that align custom templates with Jinja2.
        assert_eq!(render("{{ names }}"), "['A', 'B']");
        assert_eq!(render("{{ 2.5 | round }}|{{ 3.5 | round }}"), "2.0|4.0");
        assert_eq!(render("{{ 7 // -2 }}|{{ 7 % -2 }}"), "-4|-1");
        assert_eq!(render("{{ \"don't stop\" | title }}"), "Don't Stop");
        assert_eq!(render("{{ ['b', 'P', 'n'] | max }}"), "P");
        assert!(environment.render_str("{{ 1 / 0 }}", ()).is_err());
    }
}
