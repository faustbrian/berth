//! ANSI escape stripping helpers.

pub(in crate::output) fn strip_ansi_codes(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut characters = value.chars().peekable();

    while let Some(character) = characters.next() {
        if character == '\u{1b}' && characters.peek() == Some(&'[') {
            characters.next();
            for code in characters.by_ref() {
                if code.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        output.push(character);
    }

    output
}

#[cfg(test)]
mod tests {
    use super::strip_ansi_codes;

    #[test]
    fn strip_ansi_codes_removes_escape_sequences() {
        assert_eq!(
            strip_ansi_codes("\u{1b}[31mred\u{1b}[0m"),
            "red".to_string()
        );
    }

    #[test]
    fn strip_ansi_codes_keeps_plain_text() {
        assert_eq!(strip_ansi_codes("plain"), "plain");
    }

    #[test]
    fn strip_ansi_codes_keeps_non_ascii_text() {
        assert_eq!(
            strip_ansi_codes("\u{1b}[31m/Users/jörg/app ✓\u{1b}[0m"),
            "/Users/jörg/app ✓"
        );
    }
}
