use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    UnclosedSingleQuote,
    UnclosedDoubleQuote,
    TrailingEscape,
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnclosedSingleQuote => "unclosed single quote",
            Self::UnclosedDoubleQuote => "unclosed double quote",
            Self::TrailingEscape => "trailing backslash",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Quote {
    None,
    Single,
    Double,
}

pub fn parse_line(line: &str) -> Result<Vec<String>, ParseError> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = Quote::None;
    let mut word_started = false;
    let mut characters = line.chars();

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
}
