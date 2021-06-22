# fd

[![Build Status](https://travis-ci.org/sharkdp/fd.svg?branch=master)](https://travis-ci.org/sharkdp/fd)
[![Build status](https://ci.appveyor.com/api/projects/status/21c4p5fwggc5gy3j/branch/master?svg=true)](https://ci.appveyor.com/project/sharkdp/fd/branch/master)
[![Version info](https://img.shields.io/crates/v/fd-find.svg)](https://crates.io/crates/fd-find)
[[中文](https://github.com/chinanf-boy/fd-zh)]
[[한국어](https://github.com/spearkkk/fd-kor)]

`fd` is a program to find entries in your filesystem.
It is a simple, fast and user-friendly alternative to [`find`](https://www.gnu.org/software/findutils/).
While it does not aim to support all of `find`'s powerful functionality, it provides sensible
(opinionated) defaults for a majority of use cases.

Quick links:
* [How to use](#how-to-use)
* [Installation](#installation)
* [Troubleshooting](#troubleshooting)

## Features

* Intuitive syntax: `fd PATTERN` instead of `find -iname '*PATTERN*'`.
* Regular expression (default) and glob-based patterns.
* [Very fast](#benchmark) due to parallelized directory traversal.
* Uses colors to highlight different file types (same as *ls*).
* Supports [parallel command execution](#command-execution)
* Smart case: the search is case-insensitive by default. It switches to
  case-sensitive if the pattern contains an uppercase
  character[\*](http://vimdoc.sourceforge.net/htmldoc/options.html#'smartcase').
* Ignores hidden directories and files, by default.
* Ignores patterns from your `.gitignore`, by default.
* The command name is *50%* shorter[\*](https://github.com/ggreer/the_silver_searcher) than
  `find` :-).

## Demo

![Demo](doc/screencast.svg)

## How to use

First, to get an overview of all available command line options, you can either run
[`fd -h`](#command-line-options) for a concise help message or `fd --help` for a more detailed
version.

### Simple search

*fd* is designed to find entries in your filesystem. The most basic search you can perform is to
run *fd* with a single argument: the search pattern. For example, assume that you want to find an
old script of yours (the name included `netflix`):
``` bash
> fd netfl
Software/python/imdb-ratings/netflix-details.py
```
If called with just a single argument like this, *fd* searches the current directory recursively
for any entries that *contain* the pattern `netfl`.

### Regular expression search

The search pattern is treated as a regular expression. Here, we search for entries that start
with `x` and end with `rc`:
``` bash
> cd /etc
> fd '^x.*rc$'
X11/xinit/xinitrc
X11/xinit/xserverrc
```

The regular expression syntax used by `fd` is [documented here](https://docs.rs/regex/1.0.0/regex/#syntax).

### Specifying the root directory

If we want to search a specific directory, it can be given as a second argument to *fd*:
``` bash
> fd passwd /etc
/etc/default/passwd
/etc/pam.d/passwd
/etc/passwd
```

### List all files, recursively

*fd* can be called with no arguments. This is very useful to get a quick overview of all entries
in the current directory, recursively (similar to `ls -R`):
``` bash
> cd fd/tests
> fd
testenv
testenv/mod.rs
tests.rs
```

If you want to use this functionality to list all files in a given directory, you have to use
a catch-all pattern such as `.` or `^`:
``` bash
> fd . fd/tests/
testenv
testenv/mod.rs
tests.rs
```

### Searching for a particular file extension

Often, we are interested in all files of a particular type. This can be done with the `-e` (or
`--extension`) option. Here, we search for all Markdown files in the fd repository:
``` bash
> cd fd
> fd -e md
CONTRIBUTING.md
README.md
```

The `-e` option can be used in combination with a search pattern:
``` bash
> fd -e rs mod
src/fshelper/mod.rs
src/lscolors/mod.rs
tests/testenv/mod.rs
```

### Searching for a particular file name

 To find files with exactly the provided search pattern, use the `-g` (or `--glob`) option:
``` bash
> cd /bin
> fd -g sh
sh
```

### Hidden and ignored files
By default, *fd* does not search hidden directories and does not show hidden files in the
search results. To disable this behavior, we can use the `-H` (or `--hidden`) option:
``` bash
> fd pre-commit
> fd -H pre-commit
.git/hooks/pre-commit.sample
```

If we work in a directory that is a Git repository (or includes Git repositories), *fd* does not
search folders (and does not show files) that match one of the `.gitignore` patterns. To disable
this behavior, we can use the `-I` (or `--no-ignore`) option:
``` bash
> fd num_cpu
> fd -I num_cpu
target/debug/deps/libnum_cpus-f5ce7ef99006aa05.rlib
```

To really search *all* files and directories, simply combine the hidden and ignore features to show
everything (`-HI`).

### Command execution

Instead of just showing the search results, you often want to *do something* with them. `fd`
provides two ways to execute external commands for each of your search results:

* The `-x`/`--exec` option runs an external command *for each of the search results* (in parallel).
* The `-X`/`--exec-batch` option launches the external command once, with *all search results as arguments*.

#### Examples

Recursively find all zip archives and unpack them:
``` bash
fd -e zip -x unzip
```
If there are two such files, `file1.zip` and `backup/file2.zip`, this would execute
`unzip file1.zip` and `unzip backup/file2.zip`. The two `unzip` processes run in parallel
(if the files are found fast enough).

Find all `*.h` and `*.cpp` files and auto-format them inplace with `clang-format -i`:
``` bash
fd -e h -e cpp -x clang-format -i
```
Note how the `-i` option to `clang-format` can be passed as a separate argument. This is why
we put the `-x` option last.

Find all `test_*.py` files and open them in your favorite editor:
``` bash
fd -g 'test_*.py' -X vim
```
Note that we use capital `-X` here to open a single `vim` instance. If there are two such files,
`test_basic.py` and `lib/test_advanced.py`, this will run `vim test_basic.py lib/test_advanced.py`.

To see details like file permissions, owners, file sizes etc., you can tell `fd` to show them
by running `ls` for each result:
``` bash
fd … -X ls -lhd --color=always
```
This pattern is so useful that `fd` provides a shortcut. You can use the `-l`/`--list-details`
option to execute `ls` in this way: `fd … -l`.

Convert all `*.jpg` files to `*.png` files:
``` bash
fd -e jpg -x convert {} {.}.png
```
Here, `{}` is a placeholder for the search result. `{.}` is the same, without the file extension.
See below for more details on the placeholder syntax.

#### Placeholder syntax

The `-x` and `-X` options take a *command template* as a series of arguments (instead of a single string).
If you want to add additional options to `fd` after the command template, you can terminate it with a `\;`.

The syntax for generating commands is similar to that of [GNU Parallel](https://www.gnu.org/software/parallel/):

- `{}`: A placeholder token that will be replaced with the path of the search result
  (`documents/images/party.jpg`).
- `{.}`: Like `{}`, but without the file extension (`documents/images/party`).
- `{/}`: A placeholder that will be replaced by the basename of the search result (`party.jpg`).
- `{//}`: The parent of the discovered path (`documents/images`).
- `{/.}`: The basename, with the extension removed (`party`).

If you do not include a placeholder, *fd* automatically adds a `{}` at the end.

#### Parallel vs. serial execution

For `-x`/`--exec`, you can control the number of parallel jobs by using the `-j`/`--threads` option.
Use `--threads=1` for serial execution.

### Excluding specific files or directories

Sometimes we want to ignore search results from a specific subdirectory. For example, we might
want to search all hidden files and directories (`-H`) but exclude all matches from `.git`
directories. We can use the `-E` (or `--exclude`) option for this. It takes an arbitrary glob
pattern as an argument:
``` bash
> fd -H -E .git …
```

We can also use this to skip mounted directories:
``` bash
> fd -E /mnt/external-drive …
```

.. or to skip certain file types:
``` bash
> fd -E '*.bak' …
```

To make exclude-patterns like these permanent, you can create a `.fdignore` file. They work like
`.gitignore` files, but are specific to `fd`. For example:
``` bash
> cat ~/.fdignore
/mnt/external-drive
*.bak
```
Note: `fd` also supports `.ignore` files that are used by other programs such as `rg` or `ag`.

If you want `fd` to ignore these patterns globally, you can put them in `fd`'s global ignore file.
This is usually located in `~/.config/fd/ignore` in macOS or Linux, and `%APPDATA%\fd\ignore` in
Windows.

### Deleting files

You can use `fd` to remove all files and directories that are matched by your search pattern.
If you only want to remove files, you can use the `--exec-batch`/`-X` option to call `rm`. For
example, to recursively remove all `.DS_Store` files, run:
``` bash
> fd -H '^\.DS_Store$' -tf -X rm
```
If you are unsure, always call `fd` without `-X rm` first. Alternatively, use `rm`s "interactive"
option:
``` bash
> fd -H '^\.DS_Store$' -tf -X rm -i
```

If you also want to remove a certain class of directories, you can use the same technique. You will
have to use `rm`s `--recursive`/`-r` flag to remove directories.

Note: there are scenarios where using `fd … -X rm -r` can cause race conditions: if you have a
path like `…/foo/bar/foo/…` and want to remove all directories named `foo`, you can end up in a
situation where the outer `foo` directory is removed first, leading to (harmless) *"'foo/bar/foo':
No such file or directory"* errors in the `rm` call.

### Command-line options

This is the output of `fd -h`. To see the full set of command-line options, use `fd --help` which
also includes a much more detailed help text.

```
USAGE:
    fd [FLAGS/OPTIONS] [<pattern>] [<path>...]

FLAGS:
    -H, --hidden            Search hidden files and directories
    -I, --no-ignore         Do not respect .(git|fd)ignore files
    -s, --case-sensitive    Case-sensitive search (default: smart case)
    -i, --ignore-case       Case-insensitive search (default: smart case)
    -g, --glob              Glob-based search (default: regular expression)
    -a, --absolute-path     Show absolute instead of relative paths
    -l, --list-details      Use a long listing format with file metadata
    -L, --follow            Follow symbolic links
    -p, --full-path         Search full path (default: file-/dirname only)
    -0, --print0            Separate results by the null character
    -h, --help              Prints help information
    -V, --version           Prints version information

OPTIONS:
    -d, --max-depth <depth>            Set maximum search depth (default: none)
    -t, --type <filetype>...           Filter by type: file (f), directory (d), symlink (l),
                                       executable (x), empty (e), socket (s), pipe (p)
    -e, --extension <ext>...           Filter by file extension
    -x, --exec <cmd>                   Execute a command for each search result
    -X, --exec-batch <cmd>             Execute a command with all search results at once
    -E, --exclude <pattern>...         Exclude entries that match the given glob pattern
    -c, --color <when>                 When to use colors: never, *auto*, always
    -S, --size <size>...               Limit results based on the size of files.
        --changed-within <date|dur>    Filter by file modification time (newer than)
        --changed-before <date|dur>    Filter by file modification time (older than)

ARGS:
    <pattern>    the search pattern - a regular expression unless '--glob' is used (optional)
    <path>...    the root directory for the filesystem search (optional)
```

## Benchmark

Let's search my home folder for files that end in `[0-9].jpg`. It contains ~190.000
subdirectories and about a million files. For averaging and statistical analysis, I'm using
[hyperfine](https://github.com/sharkdp/hyperfine). The following benchmarks are performed
with a "warm"/pre-filled disk-cache (results for a "cold" disk-cache show the same trends).

Let's start with `find`:
```
Benchmark #1: find ~ -iregex '.*[0-9]\.jpg$'

  Time (mean ± σ):      7.236 s ±  0.090 s

  Range (min … max):    7.133 s …  7.385 s
```

`find` is much faster if it does not need to perform a regular-expression search:
```
Benchmark #2: find ~ -iname '*[0-9].jpg'

  Time (mean ± σ):      3.914 s ±  0.027 s

  Range (min … max):    3.876 s …  3.964 s
```

Now let's try the same for `fd`. Note that `fd` *always* performs a regular expression
search. The options `--hidden` and `--no-ignore` are needed for a fair comparison,
otherwise `fd` does not have to traverse hidden folders and ignored paths (see below):
```
Benchmark #3: fd -HI '.*[0-9]\.jpg$' ~

  Time (mean ± σ):     811.6 ms ±  26.9 ms

  Range (min … max):   786.0 ms … 870.7 ms
```
For this particular example, `fd` is approximately nine times faster than `find -iregex`
and about five times faster than `find -iname`. By the way, both tools found the exact
same 20880 files :smile:.

Finally, let's run `fd` without `--hidden` and `--no-ignore` (this can lead to different
search results, of course). If *fd* does not have to traverse the hidden and git-ignored
folders, it is almost an order of magnitude faster:
```
Benchmark #4: fd '[0-9]\.jpg$' ~

  Time (mean ± σ):     123.7 ms ±   6.0 ms

  Range (min … max):   118.8 ms … 140.0 ms
```

**Note**: This is *one particular* benchmark on *one particular* machine. While I have
performed quite a lot of different tests (and found consistent results), things might
be different for you! I encourage everyone to try it out on their own. See
[this repository](https://github.com/sharkdp/fd-benchmarks) for all necessary scripts.

Concerning *fd*'s speed, the main credit goes to the `regex` and `ignore` crates that are also used
in [ripgrep](https://github.com/BurntSushi/ripgrep) (check it out!).

## Troubleshooting

### Colorized output

`fd` can colorize files by extension, just like `ls`. In order for this to work, the environment
variable [`LS_COLORS`](https://linux.die.net/man/5/dir_colors) has to be set. Typically, the value
of this variable is set by the `dircolors` command which provides a convenient configuration format
to define colors for different file formats.
On most distributions, `LS_COLORS` should be set already. If you are on Windows or if you are looking
for alternative, more complete (or more colorful) variants, see [here](https://github.com/sharkdp/vivid),
[here](https://github.com/seebi/dircolors-solarized) or
[here](https://github.com/trapd00r/LS_COLORS).

`fd` also honors the [`NO_COLOR`](https://no-color.org/) environment variable.

### `fd` does not find my file!

Remember that `fd` ignores hidden directories and files by default. It also ignores patterns
from `.gitignore` files. If you want to make sure to find absolutely every possible file, always
use the options `-H` and `-I` to disable these two features:
``` bash
> fd -HI …
```

### `fd` doesn't seem to interpret my regex pattern correctly

A lot of special regex characters (like `[]`, `^`, `$`, ..) are also special characters in your
shell. If in doubt, always make sure to put single quotes around the regex pattern:

``` bash
> fd '^[A-Z][0-9]+$'
```

If your pattern starts with a dash, you have to add `--` to signal the end of command line
options. Otherwise, the pattern will be interpreted as a command-line option. Alternatively,
use a character class with a single hyphen character:

``` bash
> fd -- '-pattern'
> fd '[-]pattern'
```

## Integration with other programs

### Using fd with `fzf`

You can use *fd* to generate input for the command-line fuzzy finder [fzf](https://github.com/junegunn/fzf):
``` bash
export FZF_DEFAULT_COMMAND='fd --type file'
export FZF_CTRL_T_COMMAND="$FZF_DEFAULT_COMMAND"
```

Then, you can type `vim <Ctrl-T>` on your terminal to open fzf and search through the fd-results.

Alternatively, you might like to follow symbolic links and include hidden files (but exclude `.git` folders):
``` bash
export FZF_DEFAULT_COMMAND='fd --type file --follow --hidden --exclude .git'
```

You can even use fd's colored output inside fzf by setting:
``` bash
export FZF_DEFAULT_COMMAND="fd --type file --color=always"
export FZF_DEFAULT_OPTS="--ansi"
```

For more details, see the [Tips section](https://github.com/junegunn/fzf#tips) of the fzf README.

### Using fd with `rofi`

[*rofi*](https://github.com/davatorium/rofi) is a graphical launch menu application that is able to create menus by reading from *stdin*. Piping `fd` output into `rofi`s `-dmenu` mode creates fuzzy-searchable lists of files and directories.

#### Example

Create a case-insensitive searchable multi-select list of *PDF* files under your `$HOME` directory and open the selection with your configured PDF viewer. To list all file types, drop the `-e pdf` argument.

``` bash
fd --type f -e pdf . $HOME | rofi -keep-right -dmenu -i -p FILES -multi-select | xargs -I {} xdg-open {}
```

To modify the list that is presented by rofi, add arguments to the `fd` command. To modify the search behaviour of rofi, add arguments to the `rofi` command.

### Using fd with `emacs`

The emacs package [find-file-in-project](https://github.com/technomancy/find-file-in-project) can
use *fd* to find files.

After installing `find-file-in-project`, add the line `(setq ffip-use-rust-fd t)` to your
`~/.emacs` or `~/.emacs.d/init.el` file.

In emacs, run `M-x find-file-in-project-by-selected` to find matching files. Alternatively, run
`M-x find-file-in-project` to list all available files in the project.

### Printing the output as a tree

To format the output of `fd` similar to the `tree` command, install [`as-tree`] and pipe the output
of `fd` to `as-tree`:
```bash
fd | as-tree
```

This can be more useful than running `tree` by itself because `tree` does not ignore any files by
default, nor does it support as rich a set of options as `fd` does to control what to print:
```bash
❯ fd --extension rs | as-tree
.
├── build.rs
└── src
    ├── app.rs
    └── error.rs
```

For more information about `as-tree`, see [the `as-tree` README][`as-tree`].

[`as-tree`]: https://github.com/jez/as-tree

### Using fd with `xargs` or `parallel`

Note that `fd` has a builtin feature for [command execution](#command-execution) with
its `-x`/`--exec` and `-X`/`--exec-batch` options. If you prefer, you can still use
it in combination with `xargs`:
``` bash
> fd -0 -e rs | xargs -0 wc -l
```
Here, the `-0` option tells *fd* to separate search results by the NULL character (instead of
newlines). In the same way, the `-0` option of `xargs` tells it to read the input in this way.

## Installation

[![Packaging status](https://repology.org/badge/vertical-allrepos/fd-find.svg)](https://repology.org/project/fd-find/versions)

### On Ubuntu
*... and other Debian-based Linux distributions.*

If you run Ubuntu 19.04 (Disco Dingo) or newer, you can install the
[officially maintained package](https://packages.ubuntu.com/fd-find):
```
sudo apt install fd-find
```
Note that the binary is called `fdfind` as the binary name `fd` is already used by another package.
It is recommended that after installation, you add a link to `fd` by executing command
`ln -s $(which fdfind) ~/.local/bin/fd`, in order to use `fd` in the same way as in this documentation.

If you use an older version of Ubuntu, you can download the latest `.deb` package from the
[release page](https://github.com/sharkdp/fd/releases) and install it via:
``` bash
sudo dpkg -i fd_8.2.1_amd64.deb  # adapt version number and architecture
```

### On Debian

If you run Debian Buster or newer, you can install the
[officially maintained Debian package](https://tracker.debian.org/pkg/rust-fd-find):
```
sudo apt-get install fd-find
```
Note that the binary is called `fdfind` as the binary name `fd` is already used by another package.
It is recommended that after installation, you add a link to `fd` by executing command
`ln -s $(which fdfind) ~/.local/bin/fd`, in order to use `fd` in the same way as in this documentation.

### On Fedora

Starting with Fedora 28, you can install `fd` from the official package sources:
``` bash
dnf install fd-find
```

For older versions, you can use this [Fedora copr](https://copr.fedorainfracloud.org/coprs/keefle/fd/) to install `fd`:
``` bash
dnf copr enable keefle/fd
dnf install fd
```

### On Alpine Linux

You can install [the fd package](https://pkgs.alpinelinux.org/packages?name=fd)
from the official sources, provided you have the appropriate repository enabled:
```
apk add fd
```

### On Arch Linux

You can install [the fd package](https://www.archlinux.org/packages/community/x86_64/fd/) from the official repos:
```
pacman -S fd
```
### On Gentoo Linux

You can use [the fd ebuild](https://packages.gentoo.org/packages/sys-apps/fd) from the official repo:
```
emerge -av fd
```

### On openSUSE Linux

You can install [the fd package](https://software.opensuse.org/package/fd) from the official repo:
```
zypper in fd
```

### On Void Linux

You can install `fd` via xbps-install:
```
xbps-install -S fd
```

### On macOS

You can install `fd` with [Homebrew](https://formulae.brew.sh/formula/fd):
```
brew install fd
```

… or with MacPorts:
```
sudo port install fd
```

### On Windows

You can download pre-built binaries from the [release page](https://github.com/sharkdp/fd/releases).

Alternatively, you can install `fd` via [Scoop](http://scoop.sh):
```
scoop install fd
```

Or via [Chocolatey](https://chocolatey.org):
```
choco install fd
```

### On NixOS / via Nix

You can use the [Nix package manager](https://nixos.org/nix/) to install `fd`:
```
nix-env -i fd
```

### On FreeBSD

You can install [the fd-find package](https://www.freshports.org/sysutils/fd) from the official repo:
```
pkg install fd-find
```

### From NPM

On linux and macOS, you can install the [fd-find](https://npm.im/fd-find) package:

```
npm install -g fd-find
```

### From source

With Rust's package manager [cargo](https://github.com/rust-lang/cargo), you can install *fd* via:
```
cargo install fd-find
```
Note that rust version *1.36.0* or later is required.

### From binaries

The [release page](https://github.com/sharkdp/fd/releases) includes precompiled binaries for Linux, macOS and Windows. Statically-linked binaries are also available: look for archives with `musl` in the file name.

## Development
```bash
git clone https://github.com/sharkdp/fd

# Build
cd fd
cargo build

# Run unit tests and integration tests
cargo test

# Install
cargo install --path .
```

## Maintainers

- [sharkdp](https://github.com/sharkdp)
- [tmccombs](https://github.com/tmccombs)

## License

Copyright (c) 2017-2021 The fd developers

`fd` is distributed under the terms of both the MIT License and the Apache License 2.0.

See the [LICENSE-APACHE](LICENSE-APACHE) and [LICENSE-MIT](LICENSE-MIT) files for license details.
