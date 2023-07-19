# kattis.rs
A simple automated problem tester for problems on the [Kattis Problem Archive](open.kattis.com).

![crates.io](https://img.shields.io/crates/v/kattis-rs.svg)
![crates.io](https://img.shields.io/crates/d/kattis-rs.svg)

## Usage
<pre><code><b><u>Usage:</u> kattis</b> [OPTIONS] [PROBLEM]...

<b><u>Arguments</u></b>:
  [PROBLEM]...  Paths of files to test or no arguments.
                Filenames should be of the format {problem}.{ext} where {problem} can be found from the url of the kattis problem at open.kattis.com/problems/{problem}.
                If left empty, the problem to run will be inferred by looking for the latest edited valid source file in the working directory.

Options:
  <b>-s, --submit</b>           If flag is set, all successful problems will be submitted.
  <b>-f, --force</b>            Force submission even if submitted problems don't pass local tests.
  <b>-r, --recurse</b> &lt;DEPTH&gt;  Number of directory levels to recurse into when searching for problem solutions. [default: 1]
  <b>-h, --help</b>             Print help
  <b>-V, --version</b>          Print version
</code></pre>

The simplest use case for the CLI is when you want to test and submit the latest edited file in a directory:
```sh
$ kattis -s
```
This command will find the latest edited valid file in the current directory and test it using input and output from www.open.kattis.com, then submit it if it passes.

## Installation
### Using Cargo
First install the Rust toolchain using [rustup](https://rustup.rs/).
This will also install Cargo.

```sh
$ cargo install kattis-rs
```

The installation path will show up after installation.
Add it to your `PATH` by adding the cargo binary directory to `PATH` (usually `$HOME/.cargo/bin`).

`kattis-rs` is only tested on macOS and Linux, but should work on Windows.
