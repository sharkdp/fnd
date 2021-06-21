use std::env;
use std::fs;
use std::io::{self, Write};
#[cfg(unix)]
use std::os::unix;
#[cfg(windows)]
use std::os::windows;
use std::path::{Path, PathBuf};
use std::process;

use regex::Regex;
use tempdir::TempDir;

/// Environment for the integration tests.
pub struct TestEnv {
    /// Temporary working directory.
    temp_dir: TempDir,

    /// Path to the *fd* executable.
    fd_exe: PathBuf,

    /// Normalize each line by sorting the whitespace-separated words
    normalize_line: bool,
}

/// Create the working directory and the test files.
fn create_working_directory(
    directories: &[&'static str],
    files: &[&'static str],
) -> Result<TempDir, io::Error> {
    let temp_dir = TempDir::new("fd-tests")?;

    {
        let root = temp_dir.path();

        // Pretend that this is a Git repository in order for `.gitignore` files to be respected
        fs::create_dir_all(root.join(".git"))?;

        for directory in directories {
            fs::create_dir_all(root.join(directory))?;
        }

        for file in files {
            fs::File::create(root.join(file))?;
        }

        #[cfg(unix)]
        unix::fs::symlink(root.join("one/two"), root.join("symlink"))?;

        // Note: creating symlinks on Windows requires the `SeCreateSymbolicLinkPrivilege` which
        // is by default only granted for administrators.
        #[cfg(windows)]
        windows::fs::symlink_dir(root.join("one/two"), root.join("symlink"))?;

        fs::File::create(root.join(".fdignore"))?.write_all(b"fdignored.foo")?;

        fs::File::create(root.join(".gitignore"))?.write_all(b"gitignored.foo")?;
    }

    Ok(temp_dir)
}

/// Find the *fd* executable.
fn find_fd_exe() -> PathBuf {
    // Tests exe is in target/debug/deps, the *fd* exe is in target/debug
    let root = env::current_exe()
        .expect("tests executable")
        .parent()
        .expect("tests executable directory")
        .parent()
        .expect("fd executable directory")
        .to_path_buf();

    let exe_name = if cfg!(windows) { "fd.exe" } else { "fd" };

    root.join(exe_name)
}

/// Format an error message for when *fd* did not exit successfully.
fn format_exit_error(args: &[&str], output: &process::Output) -> String {
    format!(
        "`fd {}` did not exit successfully.\nstdout:\n---\n{}---\nstderr:\n---\n{}---",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Format an error message for when the output of *fd* did not match the expected output.
fn format_output_error(args: &[&str], expected: &str, actual: &str) -> String {
    // Generate diff text.
    let diff_text = diff::lines(expected, actual)
        .into_iter()
        .map(|diff| match diff {
            diff::Result::Left(l) => format!("-{}", l),
            diff::Result::Both(l, _) => format!(" {}", l),
            diff::Result::Right(r) => format!("+{}", r),
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        concat!(
            "`fd {}` did not produce the expected output.\n",
            "Showing diff between expected and actual:\n{}\n"
        ),
        args.join(" "),
        diff_text
    )
}

fn format_match_error(args: &[&str], expectedr_re: &Regex, actual: &str) -> String {
    format!(
        concat!(
            "`fd {}` did not produce the expected output.\n",
            "The Regex\n",
            "    {}\n",
            "did not match the following output:\n",
            "===== START OF OUTPUT >>>>>{}<<<<< END OF OUTPUT =====\n",
        ),
        args.join(" "),
        expectedr_re.as_str(),
        actual,
    )
}

/// Normalize the output for comparison.
fn normalize_output(s: &str, trim_start: bool, normalize_line: bool) -> String {
    // Split into lines and normalize separators.
    let mut lines = s
        .replace('\0', "NULL\n")
        .lines()
        .map(|line| {
            let line = if trim_start { line.trim_start() } else { line };
            let line = line.replace('/', &std::path::MAIN_SEPARATOR.to_string());
            if normalize_line {
                let mut words: Vec<_> = line.split_whitespace().collect();
                words.sort();
                return words.join(" ");
            }
            line
        })
        .collect::<Vec<_>>();

    lines.sort();
    lines.join("\n")
}

impl TestEnv {
    pub fn new(directories: &[&'static str], files: &[&'static str]) -> TestEnv {
        let temp_dir = create_working_directory(directories, files).expect("working directory");
        let fd_exe = find_fd_exe();

        TestEnv {
            temp_dir,
            fd_exe,
            normalize_line: false,
        }
    }

    pub fn normalize_line(self, normalize: bool) -> TestEnv {
        TestEnv {
            temp_dir: self.temp_dir,
            fd_exe: self.fd_exe,
            normalize_line: normalize,
        }
    }

    /// Create a broken symlink at the given path in the temp_dir.
    pub fn create_broken_symlink<P: AsRef<Path>>(
        &mut self,
        link_path: P,
    ) -> Result<PathBuf, io::Error> {
        let root = self.test_root();
        let broken_symlink_link = root.join(link_path);
        {
            let temp_target_dir = TempDir::new("fd-tests-broken-symlink")?;
            let broken_symlink_target = temp_target_dir.path().join("broken_symlink_target");
            fs::File::create(&broken_symlink_target)?;
            #[cfg(unix)]
            unix::fs::symlink(&broken_symlink_target, &broken_symlink_link)?;
            #[cfg(windows)]
            windows::fs::symlink_file(&broken_symlink_target, &broken_symlink_link)?;
        }
        Ok(broken_symlink_link)
    }

    /// Get the root directory for the tests.
    pub fn test_root(&self) -> PathBuf {
        self.temp_dir.path().to_path_buf()
    }

    /// Get the root directory of the file system.
    pub fn system_root(&self) -> PathBuf {
        let mut components = self.temp_dir.path().components();
        PathBuf::from(components.next().expect("root directory").as_os_str())
    }

    /// Assert that calling *fd* in the specified path under the root working directory,
    /// and with the specified arguments produces the expected output.
    pub fn assert_success_and_get_output<P: AsRef<Path>>(
        &self,
        path: P,
        args: &[&str],
    ) -> process::Output {
        // Setup *fd* command.
        let mut cmd = process::Command::new(&self.fd_exe);
        cmd.current_dir(self.temp_dir.path().join(path));
        cmd.arg("--no-global-ignore-file").args(args);

        // Run *fd*.
        let output = cmd.output().expect("fd output");

        // Check for exit status.
        if !output.status.success() {
            panic!("{}", format_exit_error(args, &output));
        }

        output
    }

    /// Assert that calling *fd* with the specified arguments produces the expected output.
    pub fn assert_output(&self, args: &[&str], expected: &str) {
        self.assert_output_subdirectory(".", args, expected)
    }

    /// Similar to assert_output, but able to handle non-utf8 output
    #[cfg(all(unix, not(target_os = "macos")))]
    pub fn assert_output_raw(&self, args: &[&str], expected: &[u8]) {
        let output = self.assert_success_and_get_output(".", args);

        assert_eq!(expected, &output.stdout[..]);
    }

    /// Similar to assert_output but uses `regex::Regex` to match output.
    pub fn assert_output_matches_re(&self, args: &[&str], expected_re: &Regex) {
        let output = self.assert_success_and_get_output(".", args);

        let actual = String::from_utf8_lossy(&output.stdout);

        if !expected_re.is_match(&actual) {
            panic!("{}", format_match_error(args, &expected_re, &actual));
        }
    }

    /// Assert that calling *fd* in the specified path under the root working directory,
    /// and with the specified arguments produces the expected output.
    pub fn assert_output_subdirectory<P: AsRef<Path>>(
        &self,
        path: P,
        args: &[&str],
        expected: &str,
    ) {
        let output = self.assert_success_and_get_output(path, args);

        // Normalize both expected and actual output.
        let expected = normalize_output(expected, true, self.normalize_line);
        let actual = normalize_output(
            &String::from_utf8_lossy(&output.stdout),
            false,
            self.normalize_line,
        );

        // Compare actual output to expected output.
        if expected != actual {
            panic!("{}", format_output_error(args, &expected, &actual));
        }
    }

    /// Assert that calling *fd* with the specified arguments produces the expected error,
    /// and does not succeed.
    pub fn assert_failure_with_error(&self, args: &[&str], expected: &str) {
        let status = self.assert_error_subdirectory(".", args, Some(expected));
        if status.success() {
            panic!("error '{}' did not occur.", expected);
        }
    }

    /// Assert that calling *fd* with the specified arguments does not succeed.
    pub fn assert_failure(&self, args: &[&str]) {
        let status = self.assert_error_subdirectory(".", args, None);
        if status.success() {
            panic!("Failure did not occur as expected.");
        }
    }

    /// Assert that calling *fd* with the specified arguments produces the expected error.
    pub fn assert_error(&self, args: &[&str], expected: &str) -> process::ExitStatus {
        self.assert_error_subdirectory(".", args, Some(expected))
    }

    /// Assert that calling *fd* in the specified path under the root working directory,
    /// and with the specified arguments produces an error with the expected message.
    fn assert_error_subdirectory<P: AsRef<Path>>(
        &self,
        path: P,
        args: &[&str],
        expected: Option<&str>,
    ) -> process::ExitStatus {
        // Setup *fd* command.
        let mut cmd = process::Command::new(&self.fd_exe);
        cmd.current_dir(self.temp_dir.path().join(path));
        cmd.arg("--no-global-ignore-file").args(args);

        // Run *fd*.
        let output = cmd.output().expect("fd output");

        if let Some(expected) = expected {
            // Normalize both expected and actual output.
            let expected_error = normalize_output(expected, true, self.normalize_line);
            let actual_err = normalize_output(
                &String::from_utf8_lossy(&output.stderr),
                false,
                self.normalize_line,
            );

            // Compare actual output to expected output.
            if !actual_err.trim_start().starts_with(&expected_error) {
                panic!("{}", format_output_error(args, &expected_error, &actual_err));
            }
        }

        return output.status;
    }
}
