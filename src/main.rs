mod app;
mod error;
mod exec;
mod exit_codes;
mod filesystem;
mod filetypes;
mod filter;
mod options;
mod output;
mod regex_helper;
mod walk;

use std::env;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;
use std::time;

use anyhow::{anyhow, Context, Result};
use atty::Stream;
use globset::GlobBuilder;
use lscolors::LsColors;
use regex::bytes::{RegexBuilder, RegexSetBuilder};

use crate::error::print_error;
use crate::exec::CommandTemplate;
use crate::exit_codes::ExitCode;
use crate::filetypes::FileTypes;
#[cfg(unix)]
use crate::filter::OwnerFilter;
use crate::filter::{SizeFilter, TimeFilter};
use crate::options::Options;
use crate::regex_helper::{pattern_has_uppercase_char, pattern_matches_strings_with_leading_dot};

// We use jemalloc for performance reasons, see https://github.com/sharkdp/fd/pull/481
// FIXME: re-enable jemalloc on macOS, see comment in Cargo.toml file for more infos
#[cfg(all(
    not(windows),
    not(target_os = "android"),
    not(target_os = "macos"),
    not(target_env = "musl")
))]
#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

// vivid --color-mode 8-bit generate molokai
const DEFAULT_LS_COLORS: &str = "
ow=0:or=0;38;5;16;48;5;203:no=0:ex=1;38;5;203:cd=0;38;5;203;48;5;236:mi=0;38;5;16;48;5;203:*~=0;38;5;243:st=0:pi=0;38;5;16;48;5;81:fi=0:di=0;38;5;81:so=0;38;5;16;48;5;203:bd=0;38;5;81;48;5;236:tw=0:ln=0;38;5;203:*.m=0;38;5;48:*.o=0;38;5;243:*.z=4;38;5;203:*.a=1;38;5;203:*.r=0;38;5;48:*.c=0;38;5;48:*.d=0;38;5;48:*.t=0;38;5;48:*.h=0;38;5;48:*.p=0;38;5;48:*.cc=0;38;5;48:*.ll=0;38;5;48:*.jl=0;38;5;48:*css=0;38;5;48:*.md=0;38;5;185:*.gz=4;38;5;203:*.nb=0;38;5;48:*.mn=0;38;5;48:*.go=0;38;5;48:*.xz=4;38;5;203:*.so=1;38;5;203:*.rb=0;38;5;48:*.pm=0;38;5;48:*.bc=0;38;5;243:*.py=0;38;5;48:*.as=0;38;5;48:*.pl=0;38;5;48:*.rs=0;38;5;48:*.sh=0;38;5;48:*.7z=4;38;5;203:*.ps=0;38;5;186:*.cs=0;38;5;48:*.el=0;38;5;48:*.rm=0;38;5;208:*.hs=0;38;5;48:*.td=0;38;5;48:*.ui=0;38;5;149:*.ex=0;38;5;48:*.js=0;38;5;48:*.cp=0;38;5;48:*.cr=0;38;5;48:*.la=0;38;5;243:*.kt=0;38;5;48:*.ml=0;38;5;48:*.vb=0;38;5;48:*.gv=0;38;5;48:*.lo=0;38;5;243:*.hi=0;38;5;243:*.ts=0;38;5;48:*.ko=1;38;5;203:*.hh=0;38;5;48:*.pp=0;38;5;48:*.di=0;38;5;48:*.bz=4;38;5;203:*.fs=0;38;5;48:*.png=0;38;5;208:*.zsh=0;38;5;48:*.mpg=0;38;5;208:*.pid=0;38;5;243:*.xmp=0;38;5;149:*.iso=4;38;5;203:*.m4v=0;38;5;208:*.dot=0;38;5;48:*.ods=0;38;5;186:*.inc=0;38;5;48:*.sxw=0;38;5;186:*.aif=0;38;5;208:*.git=0;38;5;243:*.gvy=0;38;5;48:*.tbz=4;38;5;203:*.log=0;38;5;243:*.txt=0;38;5;185:*.ico=0;38;5;208:*.csx=0;38;5;48:*.vob=0;38;5;208:*.pgm=0;38;5;208:*.pps=0;38;5;186:*.ics=0;38;5;186:*.img=4;38;5;203:*.fon=0;38;5;208:*.hpp=0;38;5;48:*.bsh=0;38;5;48:*.sql=0;38;5;48:*TODO=1:*.php=0;38;5;48:*.pkg=4;38;5;203:*.ps1=0;38;5;48:*.csv=0;38;5;185:*.ilg=0;38;5;243:*.ini=0;38;5;149:*.pyc=0;38;5;243:*.psd=0;38;5;208:*.htc=0;38;5;48:*.swp=0;38;5;243:*.mli=0;38;5;48:*hgrc=0;38;5;149:*.bst=0;38;5;149:*.ipp=0;38;5;48:*.fsi=0;38;5;48:*.tcl=0;38;5;48:*.exs=0;38;5;48:*.out=0;38;5;243:*.jar=4;38;5;203:*.xls=0;38;5;186:*.ppm=0;38;5;208:*.apk=4;38;5;203:*.aux=0;38;5;243:*.rpm=4;38;5;203:*.dll=1;38;5;203:*.eps=0;38;5;208:*.exe=1;38;5;203:*.doc=0;38;5;186:*.wma=0;38;5;208:*.deb=4;38;5;203:*.pod=0;38;5;48:*.ind=0;38;5;243:*.nix=0;38;5;149:*.lua=0;38;5;48:*.epp=0;38;5;48:*.dpr=0;38;5;48:*.htm=0;38;5;185:*.ogg=0;38;5;208:*.bin=4;38;5;203:*.otf=0;38;5;208:*.yml=0;38;5;149:*.pro=0;38;5;149:*.cxx=0;38;5;48:*.tex=0;38;5;48:*.fnt=0;38;5;208:*.erl=0;38;5;48:*.sty=0;38;5;243:*.bag=4;38;5;203:*.rst=0;38;5;185:*.pdf=0;38;5;186:*.pbm=0;38;5;208:*.xcf=0;38;5;208:*.clj=0;38;5;48:*.gif=0;38;5;208:*.rar=4;38;5;203:*.elm=0;38;5;48:*.bib=0;38;5;149:*.tsx=0;38;5;48:*.dmg=4;38;5;203:*.tmp=0;38;5;243:*.bcf=0;38;5;243:*.mkv=0;38;5;208:*.svg=0;38;5;208:*.cpp=0;38;5;48:*.vim=0;38;5;48:*.bmp=0;38;5;208:*.ltx=0;38;5;48:*.fls=0;38;5;243:*.flv=0;38;5;208:*.wav=0;38;5;208:*.m4a=0;38;5;208:*.mid=0;38;5;208:*.hxx=0;38;5;48:*.pas=0;38;5;48:*.wmv=0;38;5;208:*.tif=0;38;5;208:*.kex=0;38;5;186:*.mp4=0;38;5;208:*.bak=0;38;5;243:*.xlr=0;38;5;186:*.dox=0;38;5;149:*.swf=0;38;5;208:*.tar=4;38;5;203:*.tgz=4;38;5;203:*.cfg=0;38;5;149:*.xml=0;
38;5;185:*.jpg=0;38;5;208:*.mir=0;38;5;48:*.sxi=0;38;5;186:*.bz2=4;38;5;203:*.odt=0;38;5;186:*.mov=0;38;5;208:*.toc=0;38;5;243:*.bat=1;38;5;203:*.asa=0;38;5;48:*.awk=0;38;5;48:*.sbt=0;38;5;48:*.vcd=4;38;5;203:*.kts=0;38;5;48:*.arj=4;38;5;203:*.blg=0;38;5;243:*.c++=0;38;5;48:*.odp=0;38;5;186:*.bbl=0;38;5;243:*.idx=0;38;5;243:*.com=1;38;5;203:*.mp3=0;38;5;208:*.avi=0;38;5;208:*.def=0;38;5;48:*.cgi=0;38;5;48:*.zip=4;38;5;203:*.ttf=0;38;5;208:*.ppt=0;38;5;186:*.tml=0;38;5;149:*.fsx=0;38;5;48:*.h++=0;38;5;48:*.rtf=0;38;5;186:*.inl=0;38;5;48:*.yaml=0;38;5;149:*.html=0;38;5;185:*.mpeg=0;38;5;208:*.java=0;38;5;48:*.hgrc=0;38;5;149:*.orig=0;38;5;243:*.conf=0;38;5;149:*.dart=0;38;5;48:*.psm1=0;38;5;48:*.rlib=0;38;5;243:*.fish=0;38;5;48:*.bash=0;38;5;48:*.make=0;38;5;149:*.docx=0;38;5;186:*.json=0;38;5;149:*.psd1=0;38;5;48:*.lisp=0;38;5;48:*.tbz2=4;38;5;203:*.diff=0;38;5;48:*.epub=0;38;5;186:*.xlsx=0;38;5;186:*.pptx=0;38;5;186:*.toml=0;38;5;149:*.h264=0;38;5;208:*.purs=0;38;5;48:*.flac=0;38;5;208:*.tiff=0;38;5;208:*.jpeg=0;38;5;208:*.lock=0;38;5;243:*.less=0;38;5;48:*.dyn_o=0;38;5;243:*.scala=0;38;5;48:*.mdown=0;38;5;185:*.shtml=0;38;5;185:*.class=0;38;5;243:*.cache=0;38;5;243:*.cmake=0;38;5;149:*passwd=0;38;5;149:*.swift=0;38;5;48:*shadow=0;38;5;149:*.xhtml=0;38;5;185:*.patch=0;38;5;48:*.cabal=0;38;5;48:*README=0;38;5;16;48;5;186:*.toast=4;38;5;203:*.ipynb=0;38;5;48:*COPYING=0;38;5;249:*.gradle=0;38;5;48:*.matlab=0;38;5;48:*.config=0;38;5;149:*LICENSE=0;38;5;249:*.dyn_hi=0;38;5;243:*.flake8=0;38;5;149:*.groovy=0;38;5;48:*INSTALL=0;38;5;16;48;5;186:*TODO.md=1:*.ignore=0;38;5;149:*Doxyfile=0;38;5;149:*TODO.txt=1:*setup.py=0;38;5;149:*Makefile=0;38;5;149:*.gemspec=0;38;5;149:*.desktop=0;38;5;149:*.rgignore=0;38;5;149:*.markdown=0;38;5;185:*COPYRIGHT=0;38;5;249:*configure=0;38;5;149:*.DS_Store=0;38;5;243:*.kdevelop=0;38;5;149:*.fdignore=0;38;5;149:*README.md=0;38;5;16;48;5;186:*.cmake.in=0;38;5;149:*SConscript=0;38;5;149:*CODEOWNERS=0;38;5;149:*.localized=0;38;5;243:*.gitignore=0;38;5;149:*Dockerfile=0;38;5;149:*.gitconfig=0;38;5;149:*INSTALL.md=0;38;5;16;48;5;186:*README.txt=0;38;5;16;48;5;186:*SConstruct=0;38;5;149:*.scons_opt=0;38;5;243:*.travis.yml=0;38;5;186:*.gitmodules=0;38;5;149:*.synctex.gz=0;38;5;243:*LICENSE-MIT=0;38;5;249:*MANIFEST.in=0;38;5;149:*Makefile.in=0;38;5;243:*Makefile.am=0;38;5;149:*INSTALL.txt=0;38;5;16;48;5;186:*configure.ac=0;38;5;149:*.applescript=0;38;5;48:*appveyor.yml=0;38;5;186:*.fdb_latexmk=0;38;5;243:*CONTRIBUTORS=0;38;5;16;48;5;186:*.clang-format=0;38;5;149:*LICENSE-APACHE=0;38;5;249:*CMakeLists.txt=0;38;5;149:*CMakeCache.txt=0;38;5;243:*.gitattributes=0;38;5;149:*CONTRIBUTORS.md=0;38;5;16;48;5;186:*.sconsign.dblite=0;38;5;243:*requirements.txt=0;38;5;149:*CONTRIBUTORS.txt=0;38;5;16;48;5;186:*package-lock.json=0;38;5;243:*.CFUserTextEncoding=0;38;5;243
";

fn run() -> Result<ExitCode> {
    let mut app = app::build_app();

    // When encountering parsing errors, print error message and exit
    let matches = match app.get_matches_from_safe_borrow(env::args_os()) {
        Ok(matches) => matches,
        Err(err) => {
            eprintln!("{}", err);
            return Ok(ExitCode::GeneralError)
        }
    };

    // Print short and long help
    if matches.is_present("help") {
        let _ = app.print_help();
        // WORKAROUND: `clap` does currently not add a newline after the output
        //     of `print_help` and `print_long_help`. According to this issue
        //     (https://github.com/clap-rs/clap/issues/1536), this is a bug and
        //     it will be fixed in version 3.
        println!("");
        return Ok(ExitCode::Success);
    }
    if matches.is_present("help_long") {
        let _ = app.print_long_help();
        // WORKAROUND: `clap` does currently not add a newline after the output
        //     of `print_help` and `print_long_help`. According to this issue
        //     (https://github.com/clap-rs/clap/issues/1536), this is a bug and
        //     it will be fixed in version 3.
        println!("");
        return Ok(ExitCode::Success);
    }


    // Set the current working directory of the process
    if let Some(base_directory) = matches.value_of_os("base-directory") {
        let base_directory = Path::new(base_directory);
        if !filesystem::is_dir(base_directory) {
            return Err(anyhow!(
                "The '--base-directory' path '{}' is not a directory.",
                base_directory.to_string_lossy()
            ));
        }
        env::set_current_dir(base_directory).with_context(|| {
            format!(
                "Could not set '{}' as the current working directory",
                base_directory.to_string_lossy()
            )
        })?;
    }

    let current_directory = Path::new(".");
    if !filesystem::is_dir(current_directory) {
        return Err(anyhow!(
            "Could not retrieve current directory (has it been deleted?)."
        ));
    }

    // Get the search pattern
    let pattern = matches
        .value_of_os("pattern")
        .map(|p| {
            p.to_str()
                .ok_or_else(|| anyhow!("The search pattern includes invalid UTF-8 sequences."))
        })
        .transpose()?
        .unwrap_or("");

    // Get one or more root directories to search.
    let passed_arguments = matches
        .values_of_os("path")
        .or_else(|| matches.values_of_os("search-path"));

    let mut search_paths = if let Some(paths) = passed_arguments {
        let mut directories = vec![];
        for path in paths {
            let path_buffer = PathBuf::from(path);
            if filesystem::is_dir(&path_buffer) {
                directories.push(path_buffer);
            } else {
                print_error(format!(
                    "Search path '{}' is not a directory.",
                    path_buffer.to_string_lossy()
                ));
            }
        }

        directories
    } else {
        vec![current_directory.to_path_buf()]
    };

    // Check if we have no valid search paths.
    if search_paths.is_empty() {
        return Err(anyhow!("No valid search paths given."));
    }

    if matches.is_present("absolute-path") {
        search_paths = search_paths
            .iter()
            .map(|path_buffer| {
                path_buffer
                    .canonicalize()
                    .and_then(|pb| filesystem::absolute_path(pb.as_path()))
                    .unwrap()
            })
            .collect();
    }

    // Detect if the user accidentally supplied a path instead of a search pattern
    if !matches.is_present("full-path")
        && pattern.contains(std::path::MAIN_SEPARATOR)
        && filesystem::is_dir(Path::new(pattern))
    {
        return Err(anyhow!(
            "The search pattern '{pattern}' contains a path-separation character ('{sep}') \
             and will not lead to any search results.\n\n\
             If you want to search for all files inside the '{pattern}' directory, use a match-all pattern:\n\n  \
             fd . '{pattern}'\n\n\
             Instead, if you want your pattern to match the full file path, use:\n\n  \
             fd --full-path '{pattern}'",
            pattern = pattern,
            sep = std::path::MAIN_SEPARATOR,
        ));
    }

    let pattern_regex = if matches.is_present("glob") && !pattern.is_empty() {
        let glob = GlobBuilder::new(pattern).literal_separator(true).build()?;
        glob.regex().to_owned()
    } else if matches.is_present("fixed-strings") {
        // Treat pattern as literal string if '--fixed-strings' is used
        regex::escape(pattern)
    } else {
        String::from(pattern)
    };

    // The search will be case-sensitive if the command line flag is set or
    // if the pattern has an uppercase character (smart case).
    let case_sensitive = !matches.is_present("ignore-case")
        && (matches.is_present("case-sensitive") || pattern_has_uppercase_char(&pattern_regex));

    #[cfg(windows)]
    let ansi_colors_support =
        ansi_term::enable_ansi_support().is_ok() || std::env::var_os("TERM").is_some();

    #[cfg(not(windows))]
    let ansi_colors_support = true;

    let interactive_terminal = atty::is(Stream::Stdout);
    let colored_output = match matches.value_of("color") {
        Some("always") => true,
        Some("never") => false,
        _ => ansi_colors_support && env::var_os("NO_COLOR").is_none() && interactive_terminal,
    };

    let path_separator = matches
        .value_of("path-separator")
        .map_or_else(filesystem::default_path_separator, |s| Some(s.to_owned()));

    let ls_colors = if colored_output {
        Some(LsColors::from_env().unwrap_or_else(|| LsColors::from_string(DEFAULT_LS_COLORS)))
    } else {
        None
    };

    let command = if let Some(args) = matches.values_of("exec") {
        Some(CommandTemplate::new(args, path_separator.clone()))
    } else if let Some(args) = matches.values_of("exec-batch") {
        Some(CommandTemplate::new_batch(args, path_separator.clone())?)
    } else if matches.is_present("list-details") {
        let color = matches.value_of("color").unwrap_or("auto");
        let color_arg = ["--color=", color].concat();

        #[allow(unused)]
        let gnu_ls = |command_name| {
            vec![
                command_name,
                "-l",               // long listing format
                "--human-readable", // human readable file sizes
                "--directory",      // list directories themselves, not their contents
                &color_arg,
            ]
        };

        let cmd: Vec<&str> = if cfg!(unix) {
            if !cfg!(any(
                target_os = "macos",
                target_os = "dragonfly",
                target_os = "freebsd",
                target_os = "netbsd",
                target_os = "openbsd"
            )) {
                // Assume ls is GNU ls
                gnu_ls("ls")
            } else {
                // MacOS, DragonFlyBSD, FreeBSD
                use std::process::{Command, Stdio};

                // Use GNU ls, if available (support for --color=auto, better LS_COLORS support)
                let gnu_ls_exists = Command::new("gls")
                    .arg("--version")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .is_ok();

                if gnu_ls_exists {
                    gnu_ls("gls")
                } else {
                    let mut cmd = vec![
                        "ls", // BSD version of ls
                        "-l", // long listing format
                        "-h", // '--human-readable' is not available, '-h' is
                        "-d", // '--directory' is not available, but '-d' is
                    ];

                    if !cfg!(any(target_os = "netbsd", target_os = "openbsd")) && colored_output {
                        // -G is not available in NetBSD's and OpenBSD's ls
                        cmd.push("-G");
                    }

                    cmd
                }
            }
        } else if cfg!(windows) {
            use std::process::{Command, Stdio};

            // Use GNU ls, if available
            let gnu_ls_exists = Command::new("ls")
                .arg("--version")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok();

            if gnu_ls_exists {
                gnu_ls("ls")
            } else {
                return Err(anyhow!(
                    "'fd --list-details' is not supported on Windows unless GNU 'ls' is installed."
                ));
            }
        } else {
            return Err(anyhow!(
                "'fd --list-details' is not supported on this platform."
            ));
        };

        Some(CommandTemplate::new_batch(&cmd, path_separator.clone()).unwrap())
    } else {
        None
    };

    let size_limits = if let Some(vs) = matches.values_of("size") {
        vs.map(|sf| {
            SizeFilter::from_string(sf)
                .ok_or_else(|| anyhow!("'{}' is not a valid size constraint. See 'fd --help'.", sf))
        })
        .collect::<Result<Vec<_>>>()?
    } else {
        vec![]
    };

    let now = time::SystemTime::now();
    let mut time_constraints: Vec<TimeFilter> = Vec::new();
    if let Some(t) = matches.value_of("changed-within") {
        if let Some(f) = TimeFilter::after(&now, t) {
            time_constraints.push(f);
        } else {
            return Err(anyhow!(
                "'{}' is not a valid date or duration. See 'fd --help'.",
                t
            ));
        }
    }
    if let Some(t) = matches.value_of("changed-before") {
        if let Some(f) = TimeFilter::before(&now, t) {
            time_constraints.push(f);
        } else {
            return Err(anyhow!(
                "'{}' is not a valid date or duration. See 'fd --help'.",
                t
            ));
        }
    }

    #[cfg(unix)]
    let owner_constraint = if let Some(s) = matches.value_of("owner") {
        OwnerFilter::from_string(s)?
    } else {
        None
    };

    let config = Options {
        case_sensitive,
        search_full_path: matches.is_present("full-path"),
        ignore_hidden: !(matches.is_present("hidden")
            || matches.occurrences_of("rg-alias-hidden-ignore") >= 2),
        read_fdignore: !(matches.is_present("no-ignore")
            || matches.is_present("rg-alias-hidden-ignore")),
        read_vcsignore: !(matches.is_present("no-ignore")
            || matches.is_present("rg-alias-hidden-ignore")
            || matches.is_present("no-ignore-vcs")),
        read_global_ignore: !(matches.is_present("no-ignore")
            || matches.is_present("rg-alias-hidden-ignore")
            || matches.is_present("no-global-ignore-file")),
        follow_links: matches.is_present("follow"),
        one_file_system: matches.is_present("one-file-system"),
        null_separator: matches.is_present("null_separator"),
        max_depth: matches
            .value_of("max-depth")
            .or_else(|| matches.value_of("rg-depth"))
            .or_else(|| matches.value_of("exact-depth"))
            .map(|n| usize::from_str_radix(n, 10))
            .transpose()
            .context("Failed to parse argument to --max-depth/--exact-depth")?,
        min_depth: matches
            .value_of("min-depth")
            .or_else(|| matches.value_of("exact-depth"))
            .map(|n| usize::from_str_radix(n, 10))
            .transpose()
            .context("Failed to parse argument to --min-depth/--exact-depth")?,
        prune: matches.is_present("prune"),
        threads: std::cmp::max(
            matches
                .value_of("threads")
                .map(|n| usize::from_str_radix(n, 10))
                .transpose()
                .context("Failed to parse number of threads")?
                .map(|n| {
                    if n > 0 {
                        Ok(n)
                    } else {
                        Err(anyhow!("Number of threads must be positive."))
                    }
                })
                .transpose()?
                .unwrap_or_else(num_cpus::get),
            1,
        ),
        max_buffer_time: matches
            .value_of("max-buffer-time")
            .map(|n| u64::from_str_radix(n, 10))
            .transpose()
            .context("Failed to parse max. buffer time argument")?
            .map(time::Duration::from_millis),
        ls_colors,
        interactive_terminal,
        file_types: matches.values_of("file-type").map(|values| {
            let mut file_types = FileTypes::default();
            for value in values {
                match value {
                    "f" | "file" => file_types.files = true,
                    "d" | "directory" => file_types.directories = true,
                    "l" | "symlink" => file_types.symlinks = true,
                    "x" | "executable" => {
                        file_types.executables_only = true;
                        file_types.files = true;
                    }
                    "e" | "empty" => file_types.empty_only = true,
                    "s" | "socket" => file_types.sockets = true,
                    "p" | "pipe" => file_types.pipes = true,
                    _ => unreachable!(),
                }
            }

            // If only 'empty' was specified, search for both files and directories:
            if file_types.empty_only && !(file_types.files || file_types.directories) {
                file_types.files = true;
                file_types.directories = true;
            }

            file_types
        }),
        extensions: matches
            .values_of("extension")
            .map(|exts| {
                let patterns = exts
                    .map(|e| e.trim_start_matches('.'))
                    .map(|e| format!(r".\.{}$", regex::escape(e)));
                RegexSetBuilder::new(patterns)
                    .case_insensitive(true)
                    .build()
            })
            .transpose()?,
        command: command.map(Arc::new),
        exclude_patterns: matches
            .values_of("exclude")
            .map(|v| v.map(|p| String::from("!") + p).collect())
            .unwrap_or_else(Vec::new),
        ignore_files: matches
            .values_of("ignore-file")
            .map(|vs| vs.map(PathBuf::from).collect())
            .unwrap_or_else(Vec::new),
        size_constraints: size_limits,
        time_constraints,
        #[cfg(unix)]
        owner_constraint,
        show_filesystem_errors: matches.is_present("show-errors"),
        path_separator,
        max_results: matches
            .value_of("max-results")
            .map(|n| usize::from_str_radix(n, 10))
            .transpose()
            .context("Failed to parse --max-results argument")?
            .filter(|&n| n > 0)
            .or_else(|| {
                if matches.is_present("max-one-result") {
                    Some(1)
                } else {
                    None
                }
            }),
    };

    if cfg!(unix)
        && config.ignore_hidden
        && pattern_matches_strings_with_leading_dot(&pattern_regex)
    {
        return Err(anyhow!(
            "The pattern seems to only match files with a leading dot, but hidden files are \
            filtered by default. Consider adding -H/--hidden to search hidden files as well \
            or adjust your search pattern."
        ));
    }

    let re = RegexBuilder::new(&pattern_regex)
        .case_insensitive(!config.case_sensitive)
        .dot_matches_new_line(true)
        .build()
        .map_err(|e| {
            anyhow!(
                "{}\n\nNote: You can use the '--fixed-strings' option to search for a \
                 literal string instead of a regular expression. Alternatively, you can \
                 also use the '--glob' option to match on a glob pattern.",
                e.to_string()
            )
        })?;

    walk::scan(&search_paths, Arc::new(re), Arc::new(config))
}

fn main() {
    let result = run();
    match result {
        Ok(exit_code) => {
            process::exit(exit_code.into());
        }
        Err(err) => {
            eprintln!("[fd error]: {:#}", err);
            process::exit(ExitCode::GeneralError.into());
        }
    }
}
