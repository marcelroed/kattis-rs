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

use regex::Regex;
use std::fmt;
use std::fmt::Formatter;
use std::path::PathBuf;
use std::process::{Output, Stdio};

use crate::compare::{compare, CompareResult};
use crate::submit::submit;
use enum_iterator::IntoEnumIterator;
use futures::executor::block_on;
use itertools::any;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Debug, Clone)]
pub struct Problem {
    pub problem_name: String,
    pub submissions: Vec<Program>,
    pub submit: bool,
}

impl Problem {
    pub fn new(problem_name: &str) -> Result<Self> {
        Ok(Problem {
            problem_name: problem_name.to_string(),
            submissions: Program::from_problem_name(problem_name)?,
            submit: false,
        })
    }
    pub fn submit(mut self, submit: bool) -> Self {
        self.submit = submit;
        self
    }
}

pub async fn check_problems(problems: Vec<Problem>, force: bool) -> Vec<(Problem, Result<()>)> {
    let handles = problems.into_iter().map(|mut prob| {
        spawn(async move {
            let checked = check_problem(&mut prob, force).await;
            (prob, checked)
        })
    });

    join_all(handles)
        .await
        .into_iter()
        .map(|r| match r {
            Ok(pr) => pr,
            Err(e) => {
                eprintln!("{}", e);
                panic!();
            }
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct Program {
    lang: Lang,
    source: PathBuf,
    binary: Option<PathBuf>,
    compiled: Option<std::result::Result<(), String>>, // None if not compiled, Err if compile error
}

impl Drop for Program {
    fn drop(&mut self) {
        if let (true, Some(path)) = (&self.lang.compiled(), &self.binary) {
            std::fs::remove_file(path).ok();
        }
    }
}

impl Program {
    pub fn name(&self) -> String {
        self.source
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string()
    }

    pub fn from_problem_name(problem_name: &str) -> Result<Vec<Self>> {
        Ok(find_source(problem_name)?
            .into_iter()
            .filter_map(|path| match Program::new(path.clone()) {
                Ok(program) => Some(program),
                Err(e) => {
                    eprintln!(
                        "Failed when reading {}: {}",
                        path.file_name().unwrap().to_str().unwrap(),
                        e
                    );
                    None
                }
            })
            .collect())
    }

    pub fn new(path: PathBuf) -> Result<Self> {
        Ok(Program {
            lang: {
                if let Some(Some(Some(lang))) = path
                    .extension()
                    .map(|ext| ext.to_str().map(|x| Lang::from_extension(x)))
                {
                    lang
                } else {
                    return Err("Filetype could not be read.".into());
                }
            },
            source: path,
            binary: None,
            compiled: None,
        })
    }

    pub async fn compile(&mut self) -> Result<()> {
        if self.compiled.is_some() {
            return Err("Already compiled!".into());
        }
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
                    .arg("-fdiagnostics-color=always") // Colored output
                    .arg("-g").arg("-O2").arg("-std=gnu++17") // Kattis standards as of Sep 2020
                    .output()
                    .await
                    .expect("Couldn't compile C++ program. Make sure GNU g++ is installed and in path (this is the compiler that kattis uses).");

                self.binary = Some(output_path.to_owned());
                if output.status.success() {
                    self.compiled = Some(Ok(()));
                    Ok(())
                } else {
                    let mut err = format!("{}\n", self.name());
                    err.push_str(&String::from_utf8(output.stderr).unwrap());
                    self.compiled = Some(Err(err));
                    Err("Compile Error!".into())
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
                    self.compiled = Some(Ok(()));
                    Ok(())
                } else {
                    let mut err =
                        format!("{}\n", self.source.file_name().unwrap().to_str().unwrap());
                    err.push_str(&String::from_utf8_lossy(&output.stderr).into_owned());
                    self.compiled = Some(Err(err));
                    Err("Compile Error!".into())
                }
            }
            Lang::Python => {
                self.binary = Some(self.source.clone());
                self.compiled = Some(Ok(()));
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
        let mut tasks = FuturesOrdered::new();
        for (_i, pio) in ios.iter().enumerate() {
            let task = self.run_problem(pio);
            tasks.push(task);
        }
        Ok(tasks)
    }

    pub async fn to_string(&self) -> Result<String> {
        // Read from source
        let mut output = String::new();
        tokio::fs::File::open(&self.source)
            .await?
            .read_to_string(&mut output)
            .await?;

        Ok(output)
    }

    pub async fn submit(&self, problem_name: &str) -> Result<()> {
        submit(
            format!("{}", &self.lang),
            problem_name.to_string(),
            self.source
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string(),
            self.to_string().await.unwrap(),
        )
        .await
    }
}

#[derive(IntoEnumIterator, PartialEq, Clone, Eq, Debug)]
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

/// Used by submission system
impl fmt::Display for Lang {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Lang::Cpp => "C++",
                Lang::Python => "Python 3",
                Lang::Rust => "Rust",
            }
        )
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

struct ProblemInstance {
    program: Program,
    result: ProblemInstanceResult,
}

enum ProblemInstanceResult {
    Ran(Vec<CaseRun>),
    CompileError(String),
}

struct CaseRun {
    case_name: String,
    run_result: RunResult,
}

impl CaseRun {
    pub fn passed(&self) -> bool {
        match &self.run_result {
            RunResult::Completed(cr) => cr.failed.is_none(),
            _ => false,
        }
    }
}

pub enum RunResult {
    Completed(CompareResult),
    RuntimeError(String, String), // Output from stderr, stdout
}

// impl fmt::Display for RunResult {
//     fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
//         match self {
//             RunResult::Completed(s) => {
//                 write!(f, "{}", s)
//             },
//             RunResult::RuntimeError(stderr, stdout) => {
//                 write!(f, "")
//             },
//             RunResult::CompileError(s) => {
//                 write!(f, "{}", s)
//             }
//         }
//     }
// }

lazy_static::lazy_static! {
    static ref SEGFAULT_RE: Regex = Regex::new(r"signal: (\d+)").unwrap();
}

/// Compiles, fetches, runs and compares problem
async fn check_problem(problem: &mut Problem, force: bool) -> Result<()> {
    let should_submit = problem.submit;
    // Fetch problem IO
    let future_io = fetch_problem(&problem.problem_name);

    // Find source paths
    if problem.submissions.is_empty() {
        println!("{}", &problem.problem_name.bright_white().bold());
        println!(
            "{}",
            format!(
                "{}{}{} ({}).\n",
                "Found no source code for problem ",
                &problem.problem_name.bold(),
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

    // Compile programs and fetch io
    let (_, io) = join(
        join_all(
            problem
                .submissions
                .iter_mut()
                .map(|p| async move { p.compile().await }),
        ),
        future_io,
    )
    .await;

    // let compiled_programs = compiled_programs.into_iter().collect::<Vec<_>>();

    let io = &io?;

    let run_handles =
        problem
            .submissions
            .clone()
            .into_iter()
            .map(async move |program| -> ProblemInstance {
                let instance_results = match &program.compiled {
                    Some(Err(compile_error)) => {
                        ProblemInstanceResult::CompileError(compile_error.to_owned())
                    }
                    Some(Ok(())) => {
                        // Run program
                        let mut result_stream = program.run_problems(io).unwrap();
                        let mut results: Vec<CaseRun> = Vec::new();
                        while let Some((pio, out)) = result_stream.try_next().await.unwrap() {
                            results.push({
                                let run_result = {
                                    let segfaulted = {
                                        let status = out.status.to_string();
                                        let seg_opt = SEGFAULT_RE
                                            .captures(&status)
                                            .and_then(|cap| cap.get(1).map(|m| m.as_str() == "11"));
                                        matches!(seg_opt, Some(true))
                                    };
                                    if out.status.success() && !segfaulted {
                                        let output_string =
                                            from_utf8(out.stdout.as_slice()).unwrap().to_owned();
                                        let compare_result = compare(&output_string, &pio.output);
                                        RunResult::Completed(compare_result)
                                    } else {
                                        let runtime_error = if segfaulted {
                                            "Segmentation fault\n".red().to_string()
                                        } else {
                                            from_utf8(out.stderr.as_slice()).unwrap().to_string()
                                        };

                                        let output_before_crash =
                                            from_utf8(out.stdout.as_slice()).unwrap();
                                        RunResult::RuntimeError(
                                            runtime_error,
                                            output_before_crash.to_owned(),
                                        )
                                    }
                                };
                                CaseRun {
                                    case_name: pio.name.to_owned(),
                                    run_result,
                                }
                            });
                        }
                        ProblemInstanceResult::Ran(results)
                    }
                    None => panic!(),
                };
                ProblemInstance {
                    program,
                    result: instance_results,
                }
            });

    let run_results = join_all(run_handles).await;

    println!("{}", &problem.problem_name.bold());
    for pi in run_results {
        let program_name = pi.program.name();
        match pi.result {
            ProblemInstanceResult::Ran(cases) => {
                let mut failed_any = false;
                let mut case_print = String::new();
                for case in cases {
                    if !case.passed() {
                        failed_any = true;
                    }
                    let result_print = match case.run_result {
                        RunResult::Completed(cr) => format!("{}\n", cr),
                        RunResult::RuntimeError(stderr, stdout) => {
                            let mut out = stderr.clone();
                            if !stdout.is_empty() {
                                out.push_str(&format!(
                                    "\nBefore crashing, {} outputted:\n{}",
                                    program_name, stdout
                                ));
                            }
                            out
                        }
                    };
                    case_print.push_str(&format!("{}\n", &case.case_name.yellow().bold()));
                    case_print.push_str(&result_print);
                }
                println!("{}\n{}", program_name, case_print);

                if should_submit && (!failed_any || force) {
                    if let Err(e) = pi.program.submit(&problem.problem_name).await {
                        eprintln!("{}", e);
                    }
                }
            }
            ProblemInstanceResult::CompileError(compile_error) => {
                eprintln!("{}", compile_error);
            }
        }
    }

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
