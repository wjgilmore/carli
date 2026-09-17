use std::fmt;
use std::iter::Peekable;
use std::str::Chars;

#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    UnclosedSingleQuote,
    UnclosedDoubleQuote,
    TrailingEscape,
    UnclosedVariableBrace,
    InvalidVariableName,
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnclosedSingleQuote => "unclosed single quote",
            Self::UnclosedDoubleQuote => "unclosed double quote",
            Self::TrailingEscape => "trailing backslash",
            Self::UnclosedVariableBrace => "unclosed variable brace",
            Self::InvalidVariableName => "invalid variable name",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Quote {
    None,
    Single,
    Double,
}

fn is_valid_variable_name(name: &str) -> bool {
    let mut characters = name.chars();

    matches!(characters.next(), Some('_' | 'a'..='z' | 'A'..='Z'))
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn expand_variable(
    characters: &mut Peekable<Chars<'_>>,
    output: &mut String,
) -> Result<(), ParseError> {
    if characters.next_if_eq(&'{').is_some() {
        let mut name = String::new();
        let mut closed = false;

        for character in characters.by_ref() {
            if character == '}' {
                closed = true;
                break;
            }
            name.push(character);
        }

        if !closed {
            return Err(ParseError::UnclosedVariableBrace);
        }
        if !is_valid_variable_name(&name) {
            return Err(ParseError::InvalidVariableName);
        }
        if let Ok(value) = std::env::var(&name) {
            output.push_str(&value);
        }
        return Ok(());
    }

    let mut name = String::new();

    match characters.peek().copied() {
        Some(character) if character == '_' || character.is_ascii_alphabetic() => {
            name.push(character);
            characters.next();
        }
        _ => {
            output.push('$');
            return Ok(());
        }
    }

    while let Some(character) = characters.peek().copied() {
        if character == '_' || character.is_ascii_alphanumeric() {
            name.push(character);
            characters.next();
        } else {
            break;
        }
    }

    if let Ok(value) = std::env::var(&name) {
        output.push_str(&value);
    }

    Ok(())
}

pub fn parse_line(line: &str) -> Result<Vec<String>, ParseError> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = Quote::None;
    let mut word_started = false;
    let mut characters = line.chars().peekable();

    while let Some(character) = characters.next() {
        match (quote, character) {
            (Quote::None, '\'') => {
                quote = Quote::Single;
                word_started = true;
            }
            (Quote::None, '"') => {
                quote = Quote::Double;
                word_started = true;
            }
            (Quote::Single, '\'') => quote = Quote::None,
            (Quote::Double, '"') => quote = Quote::None,
            (Quote::None | Quote::Double, '\\') => {
                let escaped = characters.next().ok_or(ParseError::TrailingEscape)?;
                word.push(escaped);
                word_started = true;
            }
            (Quote::None, character) if character.is_whitespace() => {
                if word_started {
                    words.push(std::mem::take(&mut word));
                    word_started = false;
                }
            }
            (Quote::None | Quote::Double, '$') => {
                expand_variable(&mut characters, &mut word)?;
                word_started = true;
            }
            (_, character) => {
                word.push(character);
                word_started = true;
            }
        }
    }

    match quote {
        Quote::Single => Err(ParseError::UnclosedSingleQuote),
        Quote::Double => Err(ParseError::UnclosedDoubleQuote),
        Quote::None => {
            if word_started {
                words.push(word);
            }
            Ok(words)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_words_on_whitespace() {
        assert_eq!(
            parse_line("echo  hello\tworld\n").unwrap(),
            vec!["echo", "hello", "world"]
        );
    }

    #[test]
    fn does_not_expand_names_beginning_with_a_digit() {
        assert_eq!(parse_line("echo $2VALUE").unwrap(), vec!["echo", "$2VALUE"]);
    }

    #[test]
    fn preserves_quoted_text_and_empty_arguments() {
        assert_eq!(
            parse_line("printf '%s %s' \"hello world\" ''").unwrap(),
            vec!["printf", "%s %s", "hello world", ""]
        );
    }

    #[test]
    fn handles_escaped_characters() {
        assert_eq!(
            parse_line("echo one\\ two \\\"three\\\"").unwrap(),
            vec!["echo", "one two", "\"three\""]
        );
    }

    #[test]
    fn reports_unclosed_quotes() {
        assert_eq!(
            parse_line("echo 'hello"),
            Err(ParseError::UnclosedSingleQuote)
        );
        assert_eq!(
            parse_line("echo \"hello"),
            Err(ParseError::UnclosedDoubleQuote)
        );
    }

    #[test]
    fn expands_unquoted_variables() {
        unsafe {
            std::env::set_var("CARLI_TEST_HOME", "/example/home");
        }

        assert_eq!(
            parse_line("echo $CARLI_TEST_HOME/file").unwrap(),
            vec!["echo", "/example/home/file"]
        );
    }

    #[test]
    fn expands_variables_inside_double_quotes() {
        unsafe {
            std::env::set_var("CARLI_TEST_USER", "alice");
        }

        assert_eq!(
            parse_line("echo \"hello $CARLI_TEST_USER\"").unwrap(),
            vec!["echo", "hello alice"]
        );
    }

    #[test]
    fn expands_braced_variables() {
        unsafe {
            std::env::set_var("CARLI_TEST_BRACED_USER", "alice");
        }

        assert_eq!(
            parse_line("echo ${CARLI_TEST_BRACED_USER}_backup").unwrap(),
            vec!["echo", "alice_backup"]
        );
    }

    #[test]
    fn expands_braced_variables_inside_double_quotes() {
        unsafe {
            std::env::set_var("CARLI_TEST_BRACED_GREETING", "hello");
        }

        assert_eq!(
            parse_line("echo \"${CARLI_TEST_BRACED_GREETING} world\"").unwrap(),
            vec!["echo", "hello world"]
        );
    }

    #[test]
    fn reports_invalid_braced_variables() {
        assert_eq!(
            parse_line("echo ${CARLI_TEST_BRACED_USER"),
            Err(ParseError::UnclosedVariableBrace)
        );
        assert_eq!(
            parse_line("echo ${2USER}"),
            Err(ParseError::InvalidVariableName)
        );
        assert_eq!(parse_line("echo ${}"), Err(ParseError::InvalidVariableName));
    }

    #[test]
    fn does_not_expand_variables_inside_single_quotes() {
        unsafe {
            std::env::set_var("CARLI_TEST_USER", "alice");
        }

        assert_eq!(
            parse_line("echo '$CARLI_TEST_USER'").unwrap(),
            vec!["echo", "$CARLI_TEST_USER"]
        );

        assert_eq!(
            parse_line("echo '${CARLI_TEST_BRACED_USER}'").unwrap(),
            vec!["echo", "${CARLI_TEST_BRACED_USER}"]
        );
    }
}
