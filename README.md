# kattis-rs
A problem tester for problems on www.open.kattis.com.

## Usage
```sh
USAGE:
    kattis [FLAGS] [OPTIONS] [--] [PROBLEM]...

ARGS:
    <PROBLEM>...    Names of the problems to test.The format needs to be {problem} in
                    open.kattis.com/problems/{problem}. If left empty, the problem name will be
                    the name of the last edited source file. Make sure that source files use the
                    file name stem {problem}.

FLAGS:
    -f, --force      Force submission even if submitted problems don't pass local tests.
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -s, --submit <SUBMIT_PROBLEM>...    Problems after this flag are submitted if they pass testing.
                                        If no problems are listed, use problems from regular args.
```

The simplest use case for the CLI is when you want to test the latest edited file in a directory:
```sh
kattis -s
```
This command will find the latest edited valid file in the current directory and test it using input and output from www.open.kattis.com, then submit it if it passes.

## Installation
Can be installed from www.crates.io using
```sh
cargo install kattis
```

Can be easily installed on Arch-based distros from the AUR using an AUR manager such as `yay` or `yaourt`.
```sh
yay -S kattis
```
