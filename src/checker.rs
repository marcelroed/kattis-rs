use futures::future::join;
use futures::future::join_all;
use std::error::Error;
use std::fs::File;
use std::str::from_utf8;
use tokio::process::{Child, Command};
use tokio::spawn;

use crate::fetch::{fetch_problem, ProblemIO};
use crate::Result;
use futures::prelude::stream::*;
use futures::stream::{StreamExt, TryStreamExt};
use futures::TryFutureExt;
use std::convert::TryInto;
use std::ffi::OsStr;
use std::fmt;
use std::fmt::Formatter;
use std::io;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, ExitStatus, Output, Stdio};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use walkdir::DirEntry;

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

impl Program {
    pub fn new(path: PathBuf) -> Result<Self> {
        Ok(Program {
            lang: match path.extension() {
                Some(ext) => match ext.to_str() {
                    Some(".cpp") => Lang::Cpp,
                    Some(".py") => Lang::Python,
                    _ => {
                        return Err("Filetype not supported".into());
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
                self.binary = Some(self.source.clone().into());
                Ok(())
            }
        }
    }

    fn spawn_process(&self) -> Result<Child> {
        if let Some(bin) = &self.binary {
            match self.lang {
                Lang::Cpp => Command::new(bin)
                    .stdin(Stdio::piped())
                    .spawn()
                    .map_err(|_| "Failed to spawn C++ program".into()),
                Lang::Python => Command::new("python")
                    .arg("bin")
                    .stdin(Stdio::piped())
                    .spawn()
                    .map_err(|_| "Failed to spawn Python program".into()),
            }
        } else {
            Err("Program not compiled".into())
        }
    }

    async fn run_problem(&self, i: &String) -> Result<Output> {
        match self.spawn_process() {
            Ok(mut child) => {
                child.stdin.as_mut().unwrap().write(i.as_bytes()).await?;
                Ok(child.wait_with_output().await.unwrap())
            }
            Err(e) => Err(e),
        }
    }

    pub fn run_problems(
        &'a self,
        ios: &'a Vec<ProblemIO>,
    ) -> Result<impl Stream<Item = Result<Output>> + 'a> {
        let mut tasks = FuturesUnordered::new();
        for (i, pio) in ios.into_iter().enumerate() {
            let input = &pio.input;
            let task = self.run_problem(input);
            tasks.push(task);
        }
        Ok(tasks)
    }
}

enum Lang {
    Cpp,
    Python,
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
    println!("{}", problem_name);
    //Ok(())
    // Fetch problem IO
    let future_io = fetch_problem(problem_name);

    // Find source paths
    let source = find_source(problem_name)?;
    if source.is_empty() {
        println!(
            "Found no source code for problem {}. \
        Make sure that the file exists with one of the supported extensions.\n",
            problem_name
        );
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

    let io = io?;

    let io_borrow = &io;

    // Run
    let _run_results = join_all(programs.iter().map(async move |program| -> Result<()> {
        let mut result_stream = program.run_problems(io_borrow)?;
        while let Some(out) = result_stream.try_next().await? {
            let output_string = from_utf8(out.stdout.as_slice())?;
            println!("{}", output_string);
        }
        Ok(())
    }));
    // for ProblemIO { input, output } in io {}
    println!("Checked problem {}", problem_name);
    Ok(())
}
