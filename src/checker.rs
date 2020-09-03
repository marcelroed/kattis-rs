use futures::future::join;
use futures::future::join_all;

use std::str::from_utf8;
use tokio::process::{Child, Command};
use tokio::spawn;

use crate::fetch::{fetch_problem, ProblemIO};
use crate::Result;
use colored::Colorize;
use futures::prelude::stream::*;
use futures::stream::TryStreamExt;

use std::fmt;
use std::fmt::Formatter;
use std::io;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Output, Stdio};

use crate::compare::compare;
use enum_iterator::IntoEnumIterator;
use itertools::any;
use tokio::io::AsyncWriteExt;

pub async fn check_problems(problems: Vec<String>) {
    join_all(
        problems
            .into_iter()
            .map(|prob| spawn(async move { check_problem(&prob).await.unwrap() })),
    )
    .await;
}

struct Program {
    lang: Lang,
    source: PathBuf,
    binary: Option<PathBuf>,
}

struct RuntimeError(&'static str);

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Program failed during runtime {}", self.0)
    }
}

impl fmt::Debug for RuntimeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Program failed during runtime")
    }
}

impl Drop for Program {
    fn drop(&mut self) {
        if let (true, Some(path)) = (&self.lang.compiled(), &self.binary) {
            std::fs::remove_file(path).ok();
        }
    }
}

impl Program {
    pub fn new(path: PathBuf) -> Result<Self> {
        Ok(Program {
            lang: match path.extension() {
                Some(ext) => match ext.to_str() {
                    Some("cpp") => Lang::Cpp,
                    Some("py") => Lang::Python,
                    Some(e) => {
                        return Err(format!("Filetype {} is not supported", e).into());
                    }
                    _ => {
                        return Err("Filetype could not be read".into());
                    }
                },
                _ => {
                    return Err("Filetype not supported".into());
                }
            },
            source: path,
            binary: None,
        })
    }

    pub async fn compile(&mut self) -> Result<()> {
        match self.lang {
            Lang::Cpp => {
                let output_file: &OsStr = self.source.file_stem().unwrap();

                let output = Command::new("g++")
                    .arg(self.source.as_os_str())
                    .arg(format!("-o {}", output_file.to_str().unwrap()))
                    .output()
                    .await?;

                io::stderr().write_all(&output.stderr)?;
                // io::stdout().write_all(&output.stderr)?;
                self.binary = Some(PathBuf::from(output_file));
                Ok(())
            }
            Lang::Python => {
                self.binary = Some(self.source.clone());
                Ok(())
            }
        }
    }

    fn spawn_process(&self) -> Result<Child> {
        if let Some(bin) = &self.binary {
            match self.lang {
                Lang::Cpp => Command::new(bin)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .spawn()
                    .map_err(|_| "Failed to spawn C++ program".into()),
                Lang::Python => Command::new("python")
                    .arg(bin)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .spawn()
                    .map_err(|_| "Failed to spawn Python program".into()),
            }
        } else {
            Err("Program not compiled".into())
        }
    }

    async fn run_problem(&'a self, pio: &'a ProblemIO) -> Result<(&'a ProblemIO, Output)> {
        match self.spawn_process() {
            Ok(mut child) => {
                child
                    .stdin
                    .as_mut()
                    .unwrap()
                    .write(pio.input.as_bytes())
                    .await?;
                Ok((&pio, child.wait_with_output().await.unwrap()))
            }
            Err(e) => Err(e),
        }
    }

    pub fn run_problems(
        &'a self,
        ios: &'a [ProblemIO],
    ) -> Result<impl Stream<Item = Result<(&ProblemIO, Output)>> + 'a> {
        let tasks = FuturesUnordered::new();
        for (_i, pio) in ios.iter().enumerate() {
            let task = self.run_problem(pio);
            tasks.push(task);
        }
        Ok(tasks)
    }
}

enum Lang {
    Cpp,
    Python,
}

impl Lang {
    pub fn compiled(&self) -> bool {
        match self {
            Lang::Cpp => true,
            Lang::Python => false,
        }
    }
    pub fn extension(&self) -> String {
        match self {
            Lang::Cpp => "cpp",
            Lang::Python => "py",
        }
        .to_string()
    }

    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "cpp" => Some(Lang::Cpp),
            "py" => Some(Lang::Python),
            _ => None,
        }
    }
}

pub fn find_source(problem_name: &str) -> Result<Vec<PathBuf>> {
    let result = walkdir::WalkDir::new(".")
        .max_depth(3)
        .into_iter()
        .filter_map(|f| {
            if let Ok(de) = f {
                if let Some(s) = de.file_name().to_str() {
                    if s.starts_with(problem_name) {
                        return Some(de.path().to_path_buf());
                    }
                }
            }
            None
        })
        .collect();
    Ok(result)
}

/// Compiles, fetches, runs and compares problem
async fn check_problem(problem_name: &str) -> Result<()> {
    //Ok(())
    // Fetch problem IO
    let future_io = fetch_problem(problem_name);

    // Find source paths
    let source = find_source(problem_name)?;
    if source.is_empty() {
        println!("{}", problem_name.bright_white().bold());
        println!(
            "{}",
            format!(
                "{}{}{}",
                "Found no source code for problem ".red(),
                problem_name.red().bold(),
                ". Make sure that the file exists with one of the supported extensions.\n".red()
            )
            .red()
        );
        return Ok(());
    }

    // Create programs
    let mut programs: Vec<Program> = source
        .into_iter()
        .map(|s| Program::new(s).unwrap())
        .collect();

    // Compile programs and fetch io
    let (_, io) = join(
        join_all(programs.iter_mut().map(|p| p.compile())),
        future_io,
    )
    .await;

    let io = &io?;

    // Run
    let _run_results = join(
        join_all(programs.iter().map(async move |program| -> Result<()> {
            let mut result_stream = program.run_problems(io)?;
            while let Some((pio, out)) = result_stream.try_next().await? {
                let output_string = from_utf8(out.stdout.as_slice())?;
                println!(
                    "{}\n{}",
                    &pio.name.yellow().bold(),
                    compare(&output_string.to_string(), &pio.output)
                );
            }
            Ok(())
        })),
        async move {
            println!("{}", problem_name.bold().bright_white());
        },
    )
    .await;

    println!();

    // for ProblemIO { input, output } in io {}
    Ok(())
}
