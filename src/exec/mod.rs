mod command;
mod input;
mod job;
mod token;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use lazy_static::lazy_static;
use regex::Regex;

use crate::exit_codes::ExitCode;
use crate::filesystem::strip_current_dir;

use self::command::execute_command;
use self::input::{basename, dirname, remove_extension};
pub use self::job::{batch, job};
use self::token::Token;

/// Execution mode of the command
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExecutionMode {
    /// Command is executed for each search result
    OneByOne,
    /// Command is run for a batch of results at once
    Batch,
}

/// Represents a template that is utilized to generate command strings.
///
/// The template is meant to be coupled with an input in order to generate a command. The
/// `generate_and_execute()` method will be used to generate a command and execute it.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandTemplate {
    args: Vec<ArgumentTemplate>,
    mode: ExecutionMode,
}

impl CommandTemplate {
    pub fn new<I, S>(input: I) -> CommandTemplate
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self::build(input, ExecutionMode::OneByOne)
    }

    pub fn new_batch<I, S>(input: I) -> Result<CommandTemplate>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let cmd = Self::build(input, ExecutionMode::Batch);
        if cmd.number_of_tokens() > 1 {
            return Err(anyhow!("Only one placeholder allowed for batch commands"));
        }
        if cmd.args[0].has_tokens() {
            return Err(anyhow!(
                "First argument of exec-batch is expected to be a fixed executable"
            ));
        }
        Ok(cmd)
    }

    fn build<I, S>(input: I, mode: ExecutionMode) -> CommandTemplate
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        lazy_static! {
            static ref PLACEHOLDER_PATTERN: Regex = Regex::new(r"\{(/?\.?|//)\}").unwrap();
        }

        let mut args = Vec::new();
        let mut has_placeholder = false;

        for arg in input {
            let arg = arg.as_ref();

            let mut tokens = Vec::new();
            let mut start = 0;

            for placeholder in PLACEHOLDER_PATTERN.find_iter(arg) {
                // Leading text before the placeholder.
                if placeholder.start() > start {
                    tokens.push(Token::Text(arg[start..placeholder.start()].to_owned()));
                }

                start = placeholder.end();

                match placeholder.as_str() {
                    "{}" => tokens.push(Token::Placeholder),
                    "{.}" => tokens.push(Token::NoExt),
                    "{/}" => tokens.push(Token::Basename),
                    "{//}" => tokens.push(Token::Parent),
                    "{/.}" => tokens.push(Token::BasenameNoExt),
                    _ => unreachable!("Unhandled placeholder"),
                }

                has_placeholder = true;
            }

            // Without a placeholder, the argument is just fixed text.
            if tokens.is_empty() {
                args.push(ArgumentTemplate::Text(arg.to_owned()));
                continue;
            }

            if start < arg.len() {
                // Trailing text after last placeholder.
                tokens.push(Token::Text(arg[start..].to_owned()));
            }

            args.push(ArgumentTemplate::Tokens(tokens));
        }

        // If a placeholder token was not supplied, append one at the end of the command.
        if !has_placeholder {
            args.push(ArgumentTemplate::Tokens(vec![Token::Placeholder]));
        }

        CommandTemplate { args, mode }
    }

    fn number_of_tokens(&self) -> usize {
        self.args.iter().filter(|arg| arg.has_tokens()).count()
    }

    /// Generates and executes a command.
    ///
    /// Using the internal `args` field, and a supplied `input` variable, a `Command` will be
    /// build. Once all arguments have been processed, the command is executed.
    pub fn generate_and_execute(&self, input: &Path, out_perm: Arc<Mutex<()>>, is_multithread: bool) -> ExitCode {
        let input = strip_current_dir(input);

        let mut cmd = Command::new(self.args[0].generate(&input));
        for arg in &self.args[1..] {
            cmd.arg(arg.generate(&input));
        }

        execute_command(cmd, &out_perm, is_multithread)
    }

    pub fn in_batch_mode(&self) -> bool {
        self.mode == ExecutionMode::Batch
    }

    pub fn generate_and_execute_batch<I>(&self, paths: I, is_multithread: bool) -> ExitCode
    where
        I: Iterator<Item = PathBuf>,
    {
        let mut cmd = Command::new(self.args[0].generate(""));
        cmd.stdin(Stdio::inherit());
        cmd.stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());

        let mut paths: Vec<_> = paths.collect();
        let mut has_path = false;

        for arg in &self.args[1..] {
            if arg.has_tokens() {
                paths.sort();

                // A single `Tokens` is expected
                // So we can directly consume the iterator once and for all
                for path in &mut paths {
                    cmd.arg(arg.generate(strip_current_dir(path)));
                    has_path = true;
                }
            } else {
                cmd.arg(arg.generate(""));
            }
        }

        if has_path {
            execute_command(cmd, &Mutex::new(()), is_multithread)
        } else {
            ExitCode::Success
        }
    }
}

/// Represents a template for a single command argument.
///
/// The argument is either a collection of `Token`s including at least one placeholder variant, or
/// a fixed text.
#[derive(Clone, Debug, PartialEq)]
enum ArgumentTemplate {
    Tokens(Vec<Token>),
    Text(String),
}

impl ArgumentTemplate {
    pub fn has_tokens(&self) -> bool {
        match self {
            ArgumentTemplate::Tokens(_) => true,
            _ => false,
        }
    }

    pub fn generate(&self, path: impl AsRef<Path>) -> OsString {
        use self::Token::*;

        match *self {
            ArgumentTemplate::Tokens(ref tokens) => {
                let mut s = OsString::new();
                for token in tokens {
                    match *token {
                        Basename => s.push(basename(path.as_ref())),
                        BasenameNoExt => {
                            s.push(remove_extension(&PathBuf::from(basename(path.as_ref()))))
                        }
                        NoExt => s.push(remove_extension(path.as_ref())),
                        Parent => s.push(dirname(path.as_ref())),
                        Placeholder => s.push(path.as_ref()),
                        Text(ref string) => s.push(string),
                    }
                }
                s
            }
            ArgumentTemplate::Text(ref text) => OsString::from(text),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_with_placeholder() {
        assert_eq!(
            CommandTemplate::new(&[&"echo", &"${SHELL}:"]),
            CommandTemplate {
                args: vec![
                    ArgumentTemplate::Text("echo".into()),
                    ArgumentTemplate::Text("${SHELL}:".into()),
                    ArgumentTemplate::Tokens(vec![Token::Placeholder]),
                ],
                mode: ExecutionMode::OneByOne,
            }
        );
    }

    #[test]
    fn tokens_with_no_extension() {
        assert_eq!(
            CommandTemplate::new(&["echo", "{.}"]),
            CommandTemplate {
                args: vec![
                    ArgumentTemplate::Text("echo".into()),
                    ArgumentTemplate::Tokens(vec![Token::NoExt]),
                ],
                mode: ExecutionMode::OneByOne,
            }
        );
    }

    #[test]
    fn tokens_with_basename() {
        assert_eq!(
            CommandTemplate::new(&["echo", "{/}"]),
            CommandTemplate {
                args: vec![
                    ArgumentTemplate::Text("echo".into()),
                    ArgumentTemplate::Tokens(vec![Token::Basename]),
                ],
                mode: ExecutionMode::OneByOne,
            }
        );
    }

    #[test]
    fn tokens_with_parent() {
        assert_eq!(
            CommandTemplate::new(&["echo", "{//}"]),
            CommandTemplate {
                args: vec![
                    ArgumentTemplate::Text("echo".into()),
                    ArgumentTemplate::Tokens(vec![Token::Parent]),
                ],
                mode: ExecutionMode::OneByOne,
            }
        );
    }

    #[test]
    fn tokens_with_basename_no_extension() {
        assert_eq!(
            CommandTemplate::new(&["echo", "{/.}"]),
            CommandTemplate {
                args: vec![
                    ArgumentTemplate::Text("echo".into()),
                    ArgumentTemplate::Tokens(vec![Token::BasenameNoExt]),
                ],
                mode: ExecutionMode::OneByOne,
            }
        );
    }

    #[test]
    fn tokens_multiple() {
        assert_eq!(
            CommandTemplate::new(&["cp", "{}", "{/.}.ext"]),
            CommandTemplate {
                args: vec![
                    ArgumentTemplate::Text("cp".into()),
                    ArgumentTemplate::Tokens(vec![Token::Placeholder]),
                    ArgumentTemplate::Tokens(vec![
                        Token::BasenameNoExt,
                        Token::Text(".ext".into())
                    ]),
                ],
                mode: ExecutionMode::OneByOne,
            }
        );
    }

    #[test]
    fn tokens_single_batch() {
        assert_eq!(
            CommandTemplate::new_batch(&["echo", "{.}"]).unwrap(),
            CommandTemplate {
                args: vec![
                    ArgumentTemplate::Text("echo".into()),
                    ArgumentTemplate::Tokens(vec![Token::NoExt]),
                ],
                mode: ExecutionMode::Batch,
            }
        );
    }

    #[test]
    fn tokens_multiple_batch() {
        assert!(CommandTemplate::new_batch(&["echo", "{.}", "{}"]).is_err());
    }
}
