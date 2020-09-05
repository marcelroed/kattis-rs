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
use std::path::PathBuf;
use std::process::{Output, Stdio};

use crate::compare::compare;
use enum_iterator::IntoEnumIterator;
use futures::executor::block_on;
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
                    Some(x) => {
                        let lang_opt = Lang::from_extension(x);
                        match lang_opt {
                            Some(l) => l,
                            None => return Err("Filetype could not be read".into()),
                        }
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

    pub async fn compile(&mut self) -> std::result::Result<(), String> {
        match self.lang {
            Lang::Cpp => {
                let mut output_path = std::env::temp_dir();
                output_path.push("kattis/");
                output_path.push(format!(
                    "cpp-{}",
                    self.source.file_stem().unwrap().to_str().unwrap()
                ));

                let output = Command::new("g++")
                    .arg(self.source.as_os_str())
                    .arg("-o")
                    .arg(&output_path)
                    .output()
                    .await
                    .expect("Couldn't compile C++ program. Make sure GNU g++ is installed and in path (this is the compiler that kattis uses).");

                self.binary = Some(output_path.to_owned());
                if output.status.success() {
                    Ok(())
                } else {
                    let mut err =
                        format!("{}\n", self.source.file_name().unwrap().to_str().unwrap());
                    err.push_str(&String::from_utf8(output.stderr).unwrap());
                    Err(err)
                }
            }
            Lang::Rust => {
                let mut output_path = std::env::temp_dir();
                output_path.push("kattis/");
                output_path.push(format!(
                    "rs-{}",
                    self.source.file_stem().unwrap().to_str().unwrap()
                ));

                let output = Command::new("rustc")
                    .arg(self.source.as_os_str())
                    .arg("-o")
                    .arg(&output_path)
                    .arg("--color=always")
                    .output()
                    .await
                    .expect("Couldn't compile Rust program. Make sure rustc is installed and in path (this is the compiler that kattis uses).");

                self.binary = Some(output_path.to_owned());
                if output.status.success() {
                    Ok(())
                } else {
                    let mut err =
                        format!("{}\n", self.source.file_name().unwrap().to_str().unwrap());
                    err.push_str(&String::from_utf8_lossy(&output.stderr).into_owned());
                    Err(err)
                }
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
                Lang::Cpp | Lang::Rust => Ok(Command::new(bin)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()?),
                Lang::Python => Ok(Command::new("python")
                    .arg(bin)
                    .stdin(Stdio::piped())
                    .stderr(Stdio::piped())
                    .stdout(Stdio::piped())
                    .spawn()?),
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
                let results = child.wait_with_output().await.unwrap();
                Ok((&pio, results))
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

#[derive(IntoEnumIterator, PartialEq, Clone, Eq)]
enum Lang {
    Cpp,
    Python,
    Rust,
}

impl Lang {
    pub fn compiled(&self) -> bool {
        match self {
            Lang::Cpp => true,
            Lang::Rust => true,
            Lang::Python => false,
        }
    }
    pub fn extension(&self) -> String {
        match self {
            Lang::Cpp => "cpp",
            Lang::Rust => "rs",
            Lang::Python => "py",
        }
        .to_string()
    }

    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "cpp" => Some(Lang::Cpp),
            "py" => Some(Lang::Python),
            "rs" => Some(Lang::Rust),
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
                    let ends_with_extension = |l: Lang| s.ends_with(&format!(".{}", l.extension()));
                    if s.starts_with(&format!("{}.", problem_name))
                        && any(Lang::into_enum_iter(), ends_with_extension)
                    {
                        return Some(de.path().to_path_buf());
                    }
                }
            }
            None
        })
        .collect();
    Ok(result)
}

pub fn find_newest_source() -> Result<String> {
    let result = walkdir::WalkDir::new(".")
        .max_depth(3)
        .into_iter()
        .filter_map(|f| {
            if let Ok(de) = f {
                if let Some(s) = de.file_name().to_str() {
                    let ends_with_extension = |l: Lang| s.ends_with(&format!(".{}", l.extension()));
                    if any(Lang::into_enum_iter(), ends_with_extension) {
                        return Some(de);
                    }
                }
            }
            None
        })
        .max_by_key(|de| de.metadata().unwrap().modified().unwrap())
        .map(|de| de.path().file_stem().unwrap().to_str().unwrap().to_string())
        .ok_or_else(|| "No source found".into());

    match result {
        Ok(pname) => {
            if block_on(crate::fetch::problem_exists(pname.as_str()))? {
                Ok(pname)
            } else {
                Err(format!(
                    "Problem name {} does not exist on open.kattis.com",
                    pname.as_str().bold()
                )
                .into())
            }
        }
        Err(e) => Err(e),
    }
}

/// Compiles, fetches, runs and compares problem
async fn check_problem(problem_name: &str) -> Result<()> {
    // Fetch problem IO
    let future_io = fetch_problem(problem_name);

    // Find source paths
    let source = find_source(problem_name)?;
    if source.is_empty() {
        println!("{}", problem_name.bright_white().bold());
        println!(
            "{}",
            format!(
                "{}{}{} ({}).\n",
                "Found no source code for problem ",
                problem_name.bold(),
                ". Make sure that the file exists with one of the supported extensions\n".red(),
                Lang::into_enum_iter()
                    .map(|e| e.extension())
                    .collect::<Vec<String>>()
                    .join(", ")
            )
            .red(),
        );
        return Ok(());
    }

    // Create programs
    let mut programs: Vec<Program> = source
        .into_iter()
        .map(|s| Program::new(s).unwrap())
        .collect();

    // Compile programs and fetch io
    let (compiled_programs, io) = join(
        join_all(programs.iter_mut().map(|p| async move {
            match p.compile().await {
                Ok(()) => Ok(p),
                Err(e) => Err(e),
            }
        })),
        future_io,
    )
    .await;

    let compiled_programs = compiled_programs.into_iter().collect::<Vec<_>>();

    let io = &io?;

    // Run
    let run_results = join_all(compiled_programs.iter().map(
        async move |program_result| -> std::result::Result<String, String> {
            match program_result {
                Ok(program) => {
                    let mut result_stream = program.run_problems(io).unwrap();
                    let mut to_print = format!(
                        "{}\n",
                        program.source.file_name().unwrap().to_str().unwrap()
                    );
                    while let Some((pio, out)) = result_stream.try_next().await.unwrap() {
                        if out.status.success() {
                            let output_string = from_utf8(out.stdout.as_slice()).unwrap();
                            to_print.push_str(&format!(
                                "{}\n{}\n\n",
                                &pio.name.yellow().bold(),
                                compare(&output_string.to_string(), &pio.output)
                            ));
                        } else {
                            let runtime_error = from_utf8(out.stderr.as_slice()).unwrap();
                            let output_before_crash = from_utf8(out.stdout.as_slice()).unwrap();
                            to_print.push_str(&format!(
                                "{}\n{}{}{}\n{}\n",
                                &pio.name.yellow().bold(),
                                "Runtime error in ".bright_red(),
                                program
                                    .source
                                    .file_name()
                                    .unwrap()
                                    .to_str()
                                    .unwrap()
                                    .bold()
                                    .bright_red(),
                                ":".bright_red(),
                                runtime_error
                            ));
                            if output_before_crash.is_empty() {
                                to_print.push_str(&format!(
                                    "{}{}{}\n{}\n",
                                    "Before crashing, ".bright_red(),
                                    program
                                        .source
                                        .file_name()
                                        .unwrap()
                                        .to_str()
                                        .unwrap()
                                        .bold()
                                        .bright_red(),
                                    " outputted:".bright_red(),
                                    output_before_crash
                                ));
                            } else {
                                //to_print.push_str("Nothing printed before crash.");
                            }
                        }
                    }
                    Ok(to_print)
                }
                Err(compile_error) => {
                    let compile_error = compile_error.to_owned();
                    Err(compile_error)
                }
            }
        },
    ))
    .await;
    println!("{}", problem_name.bold().bright_white());
    run_results.into_iter().for_each(|r| match r {
        Ok(comparison_result) => print!("{}", comparison_result),
        Err(compile_error) => print!("{}", compile_error),
    });

    Ok(())
}

#[cfg(test)]
mod test {
    use crate::checker::Lang;
    use enum_iterator::IntoEnumIterator;

    #[test]
    fn complete_langs() {
        let langs = Lang::into_enum_iter();
        for lang in langs {
            assert!(Lang::from_extension(&lang.extension()).unwrap() == lang);
        }
    }
}
