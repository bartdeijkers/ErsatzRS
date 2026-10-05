//! Values passed directly in `FFmpeg` argv cross the option and graph parsers.

pub(crate) fn escape_filtergraph_option(value: &str) -> String {
    // No shell escaping: Command passes argv directly. Each parser must receive
    // its own escapes, including whitespace trimmed at token boundaries.
    escape_chars(&escape_chars(value, "\\':"), "\\'[],;")
}

fn escape_chars(value: &str, special: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for (index, character) in value.char_indices() {
        if special.contains(character)
            || (character.is_ascii_whitespace()
                && (index == 0 || index + character.len_utf8() == value.len()))
        {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

/// Normalize Windows separators while preserving literal backslashes in Unix paths.
pub(crate) fn escape_filtergraph_path(path: &std::path::Path) -> String {
    let value = path.to_string_lossy();
    let bytes = value.as_bytes();
    let windows_absolute = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\');
    let value = if cfg!(windows) || windows_absolute || value.starts_with("\\\\") {
        value.replace('\\', "/")
    } else {
        value.into_owned()
    };
    escape_filtergraph_option(&value)
}
