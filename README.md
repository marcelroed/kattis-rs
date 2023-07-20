# kattis.rs
A simple automated problem tester for problems on the [Kattis Problem Archive](open.kattis.com).

![crates.io](https://img.shields.io/crates/v/kattis-rs.svg)
![crates.io](https://img.shields.io/crates/d/kattis-rs.svg)

## Usage
<pre><code>Tests and submits Kattis competitive programming problems.

<span style="font-weight:bold;"></span><span style="text-decoration:underline;font-weight:bold;">Usage:</span> <span style="font-weight:bold;">kattis</span> [OPTIONS] [PROBLEM]...

<span style="font-weight:bold;"></span><span style="text-decoration:underline;font-weight:bold;">Arguments:</span>
  [PROBLEM]...
          Paths of files to test or no arguments.
          Filenames should be of the format {problem}.{ext} where {problem} can be found from the url of the kattis problem at open.kattis.com/problems/{problem}.
          If left empty, the problem to run will be inferred by looking for the latest edited valid source file in the working directory.

<span style="font-weight:bold;"></span><span style="text-decoration:underline;font-weight:bold;">Options:</span>
  <span style="font-weight:bold;">-s</span>, <span style="font-weight:bold;">--submit</span>
          If flag is set, all successful problems will be submitted.

  <span style="font-weight:bold;">-f</span>, <span style="font-weight:bold;">--force</span>
          Force submission even if submitted problems don't pass local tests.

  <span style="font-weight:bold;">-r</span>, <span style="font-weight:bold;">--recurse</span> &lt;DEPTH&gt;
          Number of directory levels to recurse into when searching for problem solutions.

          [default: 1]

      <span style="font-weight:bold;">--submission-viewer</span> &lt;submission-viewer&gt;
          Viewer to use for submission.

          [default: cli]

          Possible values:
          - <span style="font-weight:bold;">cli</span>:     Display updated results in the CLI, blocking until submission has finished
          - <span style="font-weight:bold;">browser</span>: Open a new browser window showing the submission and terminate program
          - <span style="font-weight:bold;">none</span>:    Just terminate, ignoring submission result

  <span style="font-weight:bold;">-h</span>, <span style="font-weight:bold;">--help</span>
          Print help (see a summary with '-h')

  <span style="font-weight:bold;">-V</span>, <span style="font-weight:bold;">--version</span>
          Print version
</code></pre>

### Submit
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
