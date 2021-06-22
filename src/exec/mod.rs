mod command;
mod input;
mod job;
mod token;

use std::borrow::Cow;
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf, Prefix};
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
    path_separator: Option<String>,
}

impl CommandTemplate {
    pub fn new<I, S>(input: I, path_separator: Option<String>) -> CommandTemplate
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self::build(input, ExecutionMode::OneByOne, path_separator)
    }

    pub fn new_batch<I, S>(input: I, path_separator: Option<String>) -> Result<CommandTemplate>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let cmd = Self::build(input, ExecutionMode::Batch, path_separator);
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

    fn build<I, S>(input: I, mode: ExecutionMode, path_separator: Option<String>) -> CommandTemplate
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

        CommandTemplate {
            args,
            mode,
            path_separator,
        }
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

        let mut cmd = Command::new(self.args[0].generate(&input, self.path_separator.as_deref()));
        for arg in &self.args[1..] {
            cmd.arg(arg.generate(&input, self.path_separator.as_deref()));
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
        let mut cmd = Command::new(self.args[0].generate("", None));
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
                    cmd.arg(arg.generate(strip_current_dir(path), self.path_separator.as_deref()));
                    has_path = true;
                }
            } else {
                cmd.arg(arg.generate("", None));
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

    /// Generate an argument from this template. If path_separator is Some, then it will replace
    /// the path separator in all placeholder tokens. Text arguments and tokens are not affected by
    /// path separator substitution.
    pub fn generate(&self, path: impl AsRef<Path>, path_separator: Option<&str>) -> OsString {
        use self::Token::*;
        let path = path.as_ref();

        match *self {
            ArgumentTemplate::Tokens(ref tokens) => {
                let mut s = OsString::new();
                for token in tokens {
                    match *token {
                        Basename => s.push(Self::replace_separator(basename(path), path_separator)),
                        BasenameNoExt => s.push(Self::replace_separator(
                            &remove_extension(basename(path).as_ref()),
                            path_separator,
                        )),
                        NoExt => s.push(Self::replace_separator(
                            &remove_extension(path),
                            path_separator,
                        )),
                        Parent => s.push(Self::replace_separator(&dirname(path), path_separator)),
                        Placeholder => {
                            s.push(Self::replace_separator(path.as_ref(), path_separator))
                        }
                        Text(ref string) => s.push(string),
                    }
                }
                s
            }
            ArgumentTemplate::Text(ref text) => OsString::from(text),
        }
    }

    /// Replace the path separator in the input with the custom separator string. If path_separator
    /// is None, simply return a borrowed Cow<OsStr> of the input. Otherwise, the input is
    /// interpreted as a Path and its components are iterated through and re-joined into a new
    /// OsString.
    fn replace_separator<'a>(path: &'a OsStr, path_separator: Option<&str>) -> Cow<'a, OsStr> {
        // fast-path - no replacement necessary
        if path_separator.is_none() {
            return Cow::Borrowed(path);
        }

        let path_separator = path_separator.unwrap();
        let mut out = OsString::with_capacity(path.len());
        let mut components = Path::new(path).components().peekable();

        while let Some(comp) = components.next() {
            match comp {
                // Absolute paths on Windows are tricky.  A Prefix component is usually a drive
                // letter or UNC path, and is usually followed by RootDir. There are also
                // "verbatim" prefixes beginning with "\\?\" that skip normalization. We choose to
                // ignore verbatim path prefixes here because they're very rare, might be
                // impossible to reach here, and there's no good way to deal with them. If users
                // are doing something advanced involving verbatim windows paths, they can do their
                // own output filtering with a tool like sed.
                Component::Prefix(prefix) => {
                    if let Prefix::UNC(server, share) = prefix.kind() {
                        // Prefix::UNC is a parsed version of '\\server\share'
                        out.push(path_separator);
                        out.push(path_separator);
                        out.push(server);
                        out.push(path_separator);
                        out.push(share);
                    } else {
                        // All other Windows prefix types are rendered as-is. This results in e.g. "C:" for
                        // drive letters. DeviceNS and Verbatim* prefixes won't have backslashes converted,
                        // but they're not returned by directories fd can search anyway so we don't worry
                        // about them.
                        out.push(comp.as_os_str());
                    }
                }

                // Root directory is always replaced with the custom separator.
                Component::RootDir => out.push(path_separator),

                // Everything else is joined normally, with a trailing separator if we're not last
                _ => {
                    out.push(comp.as_os_str());
                    if components.peek().is_some() {
                        out.push(path_separator);
                    }
                }
            }
        }
        Cow::Owned(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_with_placeholder() {
        assert_eq!(
            CommandTemplate::new(&[&"echo", &"${SHELL}:"], None),
            CommandTemplate {
                args: vec![
                    ArgumentTemplate::Text("echo".into()),
                    ArgumentTemplate::Text("${SHELL}:".into()),
                    ArgumentTemplate::Tokens(vec![Token::Placeholder]),
                ],
                mode: ExecutionMode::OneByOne,
                path_separator: None,
            }
        );
    }

    #[test]
    fn tokens_with_no_extension() {
        assert_eq!(
            CommandTemplate::new(&["echo", "{.}"], None),
            CommandTemplate {
                args: vec![
                    ArgumentTemplate::Text("echo".into()),
                    ArgumentTemplate::Tokens(vec![Token::NoExt]),
                ],
                mode: ExecutionMode::OneByOne,
                path_separator: None,
            }
        );
    }

    #[test]
    fn tokens_with_basename() {
        assert_eq!(
            CommandTemplate::new(&["echo", "{/}"], None),
            CommandTemplate {
                args: vec![
                    ArgumentTemplate::Text("echo".into()),
                    ArgumentTemplate::Tokens(vec![Token::Basename]),
                ],
                mode: ExecutionMode::OneByOne,
                path_separator: None,
            }
        );
    }

    #[test]
    fn tokens_with_parent() {
        assert_eq!(
            CommandTemplate::new(&["echo", "{//}"], None),
            CommandTemplate {
                args: vec![
                    ArgumentTemplate::Text("echo".into()),
                    ArgumentTemplate::Tokens(vec![Token::Parent]),
                ],
                mode: ExecutionMode::OneByOne,
                path_separator: None,
            }
        );
    }

    #[test]
    fn tokens_with_basename_no_extension() {
        assert_eq!(
            CommandTemplate::new(&["echo", "{/.}"], None),
            CommandTemplate {
                args: vec![
                    ArgumentTemplate::Text("echo".into()),
                    ArgumentTemplate::Tokens(vec![Token::BasenameNoExt]),
                ],
                mode: ExecutionMode::OneByOne,
                path_separator: None,
            }
        );
    }

    #[test]
    fn tokens_multiple() {
        assert_eq!(
            CommandTemplate::new(&["cp", "{}", "{/.}.ext"], None),
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
                path_separator: None,
            }
        );
    }

    #[test]
    fn tokens_single_batch() {
        assert_eq!(
            CommandTemplate::new_batch(&["echo", "{.}"], None).unwrap(),
            CommandTemplate {
                args: vec![
                    ArgumentTemplate::Text("echo".into()),
                    ArgumentTemplate::Tokens(vec![Token::NoExt]),
                ],
                mode: ExecutionMode::Batch,
                path_separator: None,
            }
        );
    }

    #[test]
    fn tokens_multiple_batch() {
        assert!(CommandTemplate::new_batch(&["echo", "{.}", "{}"], None).is_err());
    }

    #[test]
    fn generate_custom_path_separator() {
        let arg = ArgumentTemplate::Tokens(vec![Token::Placeholder]);
        macro_rules! check {
            ($input:expr, $expected:expr) => {
                assert_eq!(arg.generate($input, Some("#")), OsString::from($expected));
            };
        }

        check!("foo", "foo");
        check!("foo/bar", "foo#bar");
        check!("/foo/bar/baz", "#foo#bar#baz");
    }

    #[cfg(windows)]
    #[test]
    fn generate_custom_path_separator_windows() {
        let arg = ArgumentTemplate::Tokens(vec![Token::Placeholder]);
        macro_rules! check {
            ($input:expr, $expected:expr) => {
                assert_eq!(arg.generate($input, Some("#")), OsString::from($expected));
            };
        }

        // path starting with a drive letter
        check!(r"C:\foo\bar", "C:#foo#bar");
        // UNC path
        check!(r"\\server\share\path", "##server#share#path");
        // Drive Relative path - no separator after the colon omits the RootDir path component.
        // This is uncommon, but valid
        check!(r"C:foo\bar", "C:foo#bar");

        // forward slashses should get normalized and interpreted as separators
        check!("C:/foo/bar", "C:#foo#bar");
        check!("C:foo/bar", "C:foo#bar");

        // Rust does not intrepret "//server/share" as a UNC path, but rather as a normal
        // absolute path that begins with RootDir, and the two slashes get combined together as
        // a single path separator during normalization.
        //check!("//server/share/path", "##server#share#path");
    }
}
