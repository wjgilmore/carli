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
    MissingRedirectionTarget(&'static str),
    DuplicateInputRedirection,
    DuplicateOutputRedirection,
    MissingPipelineCommand,
    MultiplePipelineCommands,
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnclosedSingleQuote => formatter.write_str("unclosed single quote"),
            Self::UnclosedDoubleQuote => formatter.write_str("unclosed double quote"),
            Self::TrailingEscape => formatter.write_str("trailing backslash"),
            Self::UnclosedVariableBrace => formatter.write_str("unclosed variable brace"),
            Self::InvalidVariableName => formatter.write_str("invalid variable name"),
            Self::MissingRedirectionTarget(operator) => {
                write!(formatter, "missing file after `{operator}`")
            }
            Self::DuplicateInputRedirection => {
                formatter.write_str("multiple input redirections are not supported")
            }
            Self::DuplicateOutputRedirection => {
                formatter.write_str("multiple output redirections are not supported")
            }
            Self::MissingPipelineCommand => formatter.write_str("pipeline stage has no command"),
            Self::MultiplePipelineCommands => {
                formatter.write_str("expected one command, found a pipeline")
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Quote {
    None,
    Single,
    Double,
}

#[derive(Debug, PartialEq, Eq)]
pub enum OutputRedirection {
    Truncate(String),
    Append(String),
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParsedCommand {
    pub words: Vec<String>,
    pub input: Option<String>,
    pub output: Option<OutputRedirection>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParsedPipeline {
    pub commands: Vec<ParsedCommand>,
}

#[derive(Debug, PartialEq, Eq)]
enum Token {
    Word(String),
    Input,
    Output,
    Append,
    Pipe,
}

fn is_valid_variable_name(name: &str) -> bool {
    let mut characters = name.chars();

    matches!(characters.next(), Some('_' | 'a'..='z' | 'A'..='Z'))
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn expand_variable(
    characters: &mut Peekable<Chars<'_>>,
    output: &mut String,
    previous_status: i32,
) -> Result<(), ParseError> {
    if characters.next_if_eq(&'?').is_some() {
        output.push_str(&previous_status.to_string());
        return Ok(());
    }

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
    Ok(parse_command_line_with_status(line, 0)?.words)
}

pub fn parse_line_with_status(line: &str, previous_status: i32) -> Result<Vec<String>, ParseError> {
    Ok(parse_command_line_with_status(line, previous_status)?.words)
}

pub fn parse_command_line(line: &str) -> Result<ParsedCommand, ParseError> {
    parse_command_line_with_status(line, 0)
}

pub fn parse_command_line_with_status(
    line: &str,
    previous_status: i32,
) -> Result<ParsedCommand, ParseError> {
    let mut pipeline = parse_pipeline_with_status(line, previous_status)?;
    if pipeline.commands.len() > 1 {
        return Err(ParseError::MultiplePipelineCommands);
    }
    Ok(pipeline.commands.pop().unwrap_or(ParsedCommand {
        words: Vec::new(),
        input: None,
        output: None,
    }))
}

pub fn parse_pipeline(line: &str) -> Result<ParsedPipeline, ParseError> {
    parse_pipeline_with_status(line, 0)
}

pub fn parse_pipeline_with_status(
    line: &str,
    previous_status: i32,
) -> Result<ParsedPipeline, ParseError> {
    let mut tokens = Vec::new();
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
                    tokens.push(Token::Word(std::mem::take(&mut word)));
                    word_started = false;
                }
            }
            (Quote::None, '#') if !word_started => break,
            (Quote::None, '<') => {
                if word_started {
                    tokens.push(Token::Word(std::mem::take(&mut word)));
                    word_started = false;
                }
                tokens.push(Token::Input);
            }
            (Quote::None, '>') => {
                if word_started {
                    tokens.push(Token::Word(std::mem::take(&mut word)));
                    word_started = false;
                }
                if characters.next_if_eq(&'>').is_some() {
                    tokens.push(Token::Append);
                } else {
                    tokens.push(Token::Output);
                }
            }
            (Quote::None, '|') => {
                if word_started {
                    tokens.push(Token::Word(std::mem::take(&mut word)));
                    word_started = false;
                }
                tokens.push(Token::Pipe);
            }
            (Quote::None | Quote::Double, '$') => {
                expand_variable(&mut characters, &mut word, previous_status)?;
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
                tokens.push(Token::Word(word));
            }
            pipeline_from_tokens(tokens)
        }
    }
}

fn pipeline_from_tokens(tokens: Vec<Token>) -> Result<ParsedPipeline, ParseError> {
    if tokens.is_empty() {
        return Ok(ParsedPipeline {
            commands: Vec::new(),
        });
    }

    let mut commands = Vec::new();
    let mut stage = Vec::new();

    for token in tokens {
        if token == Token::Pipe {
            let command = redirections_from_tokens(std::mem::take(&mut stage))?;
            if command.words.is_empty() {
                return Err(ParseError::MissingPipelineCommand);
            }
            commands.push(command);
        } else {
            stage.push(token);
        }
    }

    let command = redirections_from_tokens(stage)?;
    if command.words.is_empty() {
        return Err(ParseError::MissingPipelineCommand);
    }
    commands.push(command);
    Ok(ParsedPipeline { commands })
}

fn redirections_from_tokens(tokens: Vec<Token>) -> Result<ParsedCommand, ParseError> {
    let mut words = Vec::new();
    let mut input = None;
    let mut output = None;
    let mut tokens = tokens.into_iter();

    while let Some(token) = tokens.next() {
        match token {
            Token::Word(word) => words.push(word),
            Token::Input => {
                if input.is_some() {
                    return Err(ParseError::DuplicateInputRedirection);
                }
                let Some(Token::Word(path)) = tokens.next() else {
                    return Err(ParseError::MissingRedirectionTarget("<"));
                };
                input = Some(path);
            }
            Token::Output | Token::Append => {
                if output.is_some() {
                    return Err(ParseError::DuplicateOutputRedirection);
                }
                let operator = if token == Token::Append { ">>" } else { ">" };
                let Some(Token::Word(path)) = tokens.next() else {
                    return Err(ParseError::MissingRedirectionTarget(operator));
                };
                output = Some(if token == Token::Append {
                    OutputRedirection::Append(path)
                } else {
                    OutputRedirection::Truncate(path)
                });
            }
            Token::Pipe => return Err(ParseError::MultiplePipelineCommands),
        }
    }

    Ok(ParsedCommand {
        words,
        input,
        output,
    })
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
    fn supports_shell_comments() {
        assert_eq!(
            parse_line("echo hello # ignored text").unwrap(),
            vec!["echo", "hello"]
        );
        assert_eq!(
            parse_line("# a complete comment").unwrap(),
            Vec::<String>::new()
        );
        assert_eq!(
            parse_line("echo value#suffix '#' \\#").unwrap(),
            vec!["echo", "value#suffix", "#", "#"]
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
            std::env::set_var("_CARLI_TEST_VALUE_2", "valid");
        }

        assert_eq!(
            parse_line("echo $CARLI_TEST_HOME/file").unwrap(),
            vec!["echo", "/example/home/file"]
        );
        assert_eq!(
            parse_line("echo $_CARLI_TEST_VALUE_2").unwrap(),
            vec!["echo", "valid"]
        );
    }

    #[test]
    fn handles_unset_empty_and_literal_dollar_variables() {
        unsafe {
            std::env::remove_var("CARLI_TEST_UNSET_VALUE");
            std::env::set_var("CARLI_TEST_EMPTY_VALUE", "");
        }

        assert_eq!(
            parse_line("echo before$CARLI_TEST_UNSET_VALUE after").unwrap(),
            vec!["echo", "before", "after"]
        );
        assert_eq!(
            parse_line("echo \"$CARLI_TEST_EMPTY_VALUE\" $ $- ").unwrap(),
            vec!["echo", "", "$", "$-"]
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

    #[test]
    fn expands_previous_command_status() {
        assert_eq!(
            parse_line_with_status("echo $? status=$?", 127).unwrap(),
            vec!["echo", "127", "status=127"]
        );
        assert_eq!(
            parse_line_with_status("echo \"status: $?\"", 2).unwrap(),
            vec!["echo", "status: 2"]
        );
    }

    #[test]
    fn preserves_literal_previous_command_status() {
        assert_eq!(
            parse_line_with_status("echo '$?' \\$?", 1).unwrap(),
            vec!["echo", "$?", "$?"]
        );
    }

    #[test]
    fn parses_input_and_output_redirection() {
        assert_eq!(
            parse_command_line("sort<input.txt>output.txt").unwrap(),
            ParsedCommand {
                words: vec!["sort".to_string()],
                input: Some("input.txt".to_string()),
                output: Some(OutputRedirection::Truncate("output.txt".to_string())),
            }
        );
        assert_eq!(
            parse_command_line("echo hello >> output.txt").unwrap(),
            ParsedCommand {
                words: vec!["echo".to_string(), "hello".to_string()],
                input: None,
                output: Some(OutputRedirection::Append("output.txt".to_string())),
            }
        );
    }

    #[test]
    fn parses_pipeline_stages_with_independent_redirections() {
        assert_eq!(
            parse_pipeline("cat < input | grep needle | sort > output").unwrap(),
            ParsedPipeline {
                commands: vec![
                    ParsedCommand {
                        words: vec!["cat".to_string()],
                        input: Some("input".to_string()),
                        output: None,
                    },
                    ParsedCommand {
                        words: vec!["grep".to_string(), "needle".to_string()],
                        input: None,
                        output: None,
                    },
                    ParsedCommand {
                        words: vec!["sort".to_string()],
                        input: None,
                        output: Some(OutputRedirection::Truncate("output".to_string())),
                    },
                ],
            }
        );
    }

    #[test]
    fn preserves_quoted_and_escaped_pipe_characters() {
        assert_eq!(
            parse_pipeline(r#"printf '%s%s%s' '|' "|" \|"#).unwrap(),
            ParsedPipeline {
                commands: vec![ParsedCommand {
                    words: vec![
                        "printf".to_string(),
                        "%s%s%s".to_string(),
                        "|".to_string(),
                        "|".to_string(),
                        "|".to_string(),
                    ],
                    input: None,
                    output: None,
                }],
            }
        );
    }

    #[test]
    fn rejects_missing_pipeline_stages() {
        for line in ["| cat", "cat |", "cat || sort", "cat | > output"] {
            assert_eq!(
                parse_pipeline(line),
                Err(ParseError::MissingPipelineCommand),
                "line: {line}"
            );
        }
    }

    #[test]
    fn preserves_quoted_and_escaped_redirection_operators() {
        assert_eq!(
            parse_command_line("echo '>' \"<\" \\>").unwrap(),
            ParsedCommand {
                words: vec![
                    "echo".to_string(),
                    ">".to_string(),
                    "<".to_string(),
                    ">".to_string(),
                ],
                input: None,
                output: None,
            }
        );
    }

    #[test]
    fn expands_variables_in_redirection_paths() {
        unsafe {
            std::env::set_var("CARLI_TEST_OUTPUT", "results.txt");
        }

        assert_eq!(
            parse_command_line("echo ok > $CARLI_TEST_OUTPUT").unwrap(),
            ParsedCommand {
                words: vec!["echo".to_string(), "ok".to_string()],
                input: None,
                output: Some(OutputRedirection::Truncate("results.txt".to_string())),
            }
        );
    }

    #[test]
    fn reports_invalid_redirection() {
        assert_eq!(
            parse_command_line("echo hello >"),
            Err(ParseError::MissingRedirectionTarget(">"))
        );
        assert_eq!(
            parse_command_line("cat < one.txt < two.txt"),
            Err(ParseError::DuplicateInputRedirection)
        );
        assert_eq!(
            parse_command_line("echo hello > one.txt >> two.txt"),
            Err(ParseError::DuplicateOutputRedirection)
        );
    }

    #[test]
    fn parses_empty_and_whitespace_only_lines() {
        assert_eq!(parse_line("").unwrap(), Vec::<String>::new());
        assert_eq!(parse_line(" \t\n\r ").unwrap(), Vec::<String>::new());
    }

    #[test]
    fn concatenates_adjacent_quoted_and_unquoted_segments() {
        assert_eq!(
            parse_line("echo pre'middle'\"post\" ''suffix prefix''").unwrap(),
            vec!["echo", "premiddlepost", "suffix", "prefix"]
        );
    }

    #[test]
    fn single_quotes_preserve_backslashes_and_special_characters() {
        assert_eq!(
            parse_line(r"echo 'a\b $HOME # < >'").unwrap(),
            vec!["echo", r"a\b $HOME # < >"]
        );
    }

    #[test]
    fn double_quote_backslash_escapes_the_next_character() {
        assert_eq!(
            parse_line(r#"echo "a\"b\$c\\d""#).unwrap(),
            vec!["echo", "a\"b$c\\d"]
        );
    }

    #[test]
    fn reports_trailing_backslash_inside_and_outside_double_quotes() {
        assert_eq!(
            parse_line("echo trailing\\"),
            Err(ParseError::TrailingEscape)
        );
        assert_eq!(
            parse_line("echo \"trailing\\"),
            Err(ParseError::TrailingEscape)
        );
    }

    #[test]
    fn dollar_without_a_valid_name_remains_literal() {
        assert_eq!(
            parse_line("echo $ $- $9 $. ${CARLI_TEST_UNSET}").unwrap(),
            vec!["echo", "$", "$-", "$9", "$.", ""]
        );
    }

    #[test]
    fn variable_values_do_not_undergo_word_splitting_or_reparsing() {
        unsafe {
            std::env::set_var("CARLI_TEST_RAW_VALUE", "two words > file # text");
        }
        assert_eq!(
            parse_command_line("echo $CARLI_TEST_RAW_VALUE").unwrap(),
            ParsedCommand {
                words: vec!["echo".to_string(), "two words > file # text".to_string()],
                input: None,
                output: None,
            }
        );
    }

    #[test]
    fn braced_names_accept_boundaries_and_reject_non_ascii_names() {
        unsafe {
            std::env::set_var("_A0", "boundary");
        }
        assert_eq!(parse_line("echo ${_A0}").unwrap(), vec!["echo", "boundary"]);
        assert_eq!(
            parse_line("echo ${NÁME}"),
            Err(ParseError::InvalidVariableName)
        );
        assert_eq!(
            parse_line("echo ${BAD-NAME}"),
            Err(ParseError::InvalidVariableName)
        );
    }

    #[test]
    fn previous_status_supports_zero_and_large_internal_values() {
        assert_eq!(
            parse_line_with_status("echo $?", 0).unwrap(),
            vec!["echo", "0"]
        );
        assert_eq!(
            parse_line_with_status("echo $?", 1024).unwrap(),
            vec!["echo", "1024"]
        );
    }

    #[test]
    fn parses_redirections_before_between_and_after_words() {
        assert_eq!(
            parse_command_line("> out command first < in second").unwrap(),
            ParsedCommand {
                words: vec![
                    "command".to_string(),
                    "first".to_string(),
                    "second".to_string()
                ],
                input: Some("in".to_string()),
                output: Some(OutputRedirection::Truncate("out".to_string())),
            }
        );
    }

    #[test]
    fn parses_empty_quoted_redirection_path() {
        assert_eq!(
            parse_command_line("echo value > ''").unwrap(),
            ParsedCommand {
                words: vec!["echo".to_string(), "value".to_string()],
                input: None,
                output: Some(OutputRedirection::Truncate(String::new())),
            }
        );
    }

    #[test]
    fn reports_each_missing_redirection_target_form() {
        for (line, operator) in [("cat <", "<"), ("echo >", ">"), ("echo >>", ">>")] {
            assert_eq!(
                parse_command_line(line),
                Err(ParseError::MissingRedirectionTarget(operator)),
                "line: {line}"
            );
        }
    }

    #[test]
    fn rejects_an_operator_where_a_redirection_path_is_required() {
        for line in ["cat < > out", "echo > < in", "echo >> > out"] {
            assert!(
                matches!(
                    parse_command_line(line),
                    Err(ParseError::MissingRedirectionTarget(_))
                ),
                "line: {line}"
            );
        }
    }

    #[test]
    fn rejects_every_duplicate_output_operator_combination() {
        for line in [
            "echo > one > two",
            "echo > one >> two",
            "echo >> one > two",
            "echo >> one >> two",
        ] {
            assert_eq!(
                parse_command_line(line),
                Err(ParseError::DuplicateOutputRedirection),
                "line: {line}"
            );
        }
    }

    #[test]
    fn parse_error_messages_are_stable_and_specific() {
        let cases = [
            (ParseError::UnclosedSingleQuote, "unclosed single quote"),
            (ParseError::UnclosedDoubleQuote, "unclosed double quote"),
            (ParseError::TrailingEscape, "trailing backslash"),
            (ParseError::UnclosedVariableBrace, "unclosed variable brace"),
            (ParseError::InvalidVariableName, "invalid variable name"),
            (
                ParseError::DuplicateInputRedirection,
                "multiple input redirections are not supported",
            ),
            (
                ParseError::DuplicateOutputRedirection,
                "multiple output redirections are not supported",
            ),
            (
                ParseError::MissingPipelineCommand,
                "pipeline stage has no command",
            ),
            (
                ParseError::MultiplePipelineCommands,
                "expected one command, found a pipeline",
            ),
        ];
        for (error, message) in cases {
            assert_eq!(error.to_string(), message);
        }
        assert_eq!(
            ParseError::MissingRedirectionTarget(">>").to_string(),
            "missing file after `>>`"
        );
    }
}
