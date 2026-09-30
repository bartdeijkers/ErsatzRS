/// A stored Lua schedule path followed by its positional arguments.
///
/// Quoting groups whitespace; backslashes remain literal so Windows paths survive.
/// Keep the native editor's single-quote support alongside upstream double quotes.
/// This is tokenization only: no shell, expansion or process execution is involved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptedScheduleCommand {
    /// The unquoted script filename.
    pub file: String,
    /// Positional arguments supplied to Lua as `arg[1]`, `arg[2]`, etc.
    pub arguments: Vec<String>,
}

impl ScriptedScheduleCommand {
    /// Parse a stored schedule command, rejecting empty paths and unclosed quotes.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let mut tokens = Vec::new();
        let mut token = String::new();
        let mut quote = None;
        let mut started = false;
        for ch in value.chars() {
            if let Some(delimiter) = quote {
                if ch == delimiter {
                    quote = None;
                } else {
                    token.push(ch);
                }
            } else if ch == '"' || (ch == '\'' && !started) {
                quote = Some(ch);
                started = true;
            } else if ch.is_whitespace() {
                if started {
                    tokens.push(std::mem::take(&mut token));
                    started = false;
                }
            } else {
                token.push(ch);
                started = true;
            }
        }
        if quote.is_some() {
            return None;
        }
        if started {
            tokens.push(token);
        }
        let mut tokens = tokens.into_iter();
        let file = tokens.next().filter(|file| !file.is_empty())?;
        Some(Self {
            file,
            arguments: tokens.collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ScriptedScheduleCommand;

    #[test]
    fn scripted_command_keeps_windows_paths_and_literal_arguments() {
        let command = ScriptedScheduleCommand::parse(
            r#""C:\Synthetic Scripts\schedule.lua" --title="two words" "" '$HOME' literal\path O'Brien"#,
        ).unwrap();
        assert_eq!(command.file, r"C:\Synthetic Scripts\schedule.lua");
        assert_eq!(
            command.arguments,
            ["--title=two words", "", "$HOME", r"literal\path", "O'Brien"]
        );
    }

    #[test]
    fn scripted_command_rejects_missing_path_or_incomplete_quote() {
        for value in [
            "",
            " \t",
            "\"\" ignored",
            "'' ignored",
            "\"unfinished",
            "file.lua 'unfinished",
        ] {
            assert!(ScriptedScheduleCommand::parse(value).is_none(), "{value:?}");
        }
    }
}
