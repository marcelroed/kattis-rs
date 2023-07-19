use futures::future::join;
use futures::future::join_all;

use std::str::from_utf8;
use tokio::process::{Child, Command};
use tokio::spawn;

use crate::{fetch, RECURSE_DEPTH};
use crate::fetch::ProblemIO;
use anyhow::{Result, anyhow, bail};
use colored::Colorize;
use futures::prelude::stream::*;
use futures::stream::TryStreamExt;

use regex::Regex;
use std::fmt;
use std::fmt::Formatter;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};

use crate::compare::{compare, ComparisonResult};
use crate::submit::submit;
use enum_iterator::{Sequence, all};
use futures::executor::block_on;
use itertools::Itertools;
use tokio::io::AsyncReadExt;
use guard::guard;

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

use std::time::SystemTime;
use log::info;
use walkdir::DirEntry;

#[derive(Debug)]
pub struct Problem {
    pub problem_name: String,
    pub submission: Program,
    pub submit: bool,
}

impl Problem {
    pub fn new(problem_source: ProblemSource) -> Self {
        Self {
            problem_name: problem_source.problem_name.clone(),
            submission: Program::from_problem_source(problem_source),
            submit: false,
        }
    }
    pub const fn set_submit(mut self, submit: bool) -> Self {
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
                eprintln!("HERE {e}");
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
            std::fs::remove_file(path).unwrap_or_else(|_|
                eprintln!("[Warning] Failed to remove binary for {} at {:?}", self.name(), path
            ));
        }
    }
}

impl Program {
    pub fn name(&self) -> &str {
        self.source
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
    }

    #[allow(clippy::missing_const_for_fn)]
    pub fn from_problem_source(problem_source: ProblemSource) -> Self {
        Self {
            lang: problem_source.lang,
            source: problem_source.path,
            binary: None,
            compiled: None,
        }
    }

    // pub fn new(path: PathBuf) -> Result<Self> {
    //     Ok(Self {
    //         lang: {
    //             let extension = path.extension()
    //             .and_then(OsStr::to_str)
    //             .and_then(Lang::from_extension);
    //             if let Some(ext) = extension {
    //                 ext
    //             } else {
    //                 bail!("Failed to read {}, since the filetype {} is not supported.
    //                 The supported filetypes are: {}",
    //                     path.display(),
    //                     path.extension().and_then(std::ffi::OsStr::to_str).unwrap_or("<no extension>"),
    //                     all::<Lang>().map(|l| l.extension()).join(", "));
    //             }
    //         },
    //         source: path,
    //         binary: None,
    //         compiled: None,
    //     })
    // }

    pub async fn compile(&mut self) -> Result<()> {
        if self.compiled.is_some() {
            bail!("Already compiled!");
        }
        match self.lang {
            Lang::Cpp => {
                info!("Compiling {}", self.name());
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

                info!("Finished compiling {}", self.name());
                if output.status.success() {
                    self.compiled = Some(Ok(()));
                    self.binary = Some(output_path.clone());
                    Ok(())
                } else {
                    let mut err = format!("{}\n", self.name());
                    err.push_str(&String::from_utf8(output.stderr).unwrap());
                    self.compiled = Some(Err(err));
                    bail!("Compile Error!")
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
                    .expect(
                        "Couldn't compile Rust program. Make sure rustc is installed and in path.",
                    );

                if output.status.success() {
                    self.compiled = Some(Ok(()));
                    self.binary = Some(output_path.clone());
                    Ok(())
                } else {
                    let mut err =
                        format!("{}\n", self.source.file_name().unwrap().to_str().unwrap());
                    err.push_str(&String::from_utf8_lossy(&output.stderr));
                    self.compiled = Some(Err(err));
                    Err(anyhow!("Compile Error!"))
                }
            }
            Lang::Python => {
                self.binary = Some(self.source.clone());
                self.compiled = Some(Ok(()));
                Ok(())
            }
        }
    }

    fn spawn_process(&self, stdin_file: std::fs::File) -> Result<Child> {
        if let Some(bin) = &self.binary {
            match self.lang {
                Lang::Cpp | Lang::Rust => Ok(Command::new(bin)
                    .stdin(Stdio::from(stdin_file))
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()?),
                Lang::Python => Ok(Command::new("python")
                    .arg(bin)
                    .stdin(Stdio::from(stdin_file))
                    .stderr(Stdio::piped())
                    .stdout(Stdio::piped())
                    .spawn()?),
            }
        } else {
            bail!("Program not compiled");
        }
    }

    async fn run_problem<'a>(&'a self, pio: &'a ProblemIO) -> Result<(&'a ProblemIO, Output)> {
        info!("Running problem {}", self.name());
        match self.spawn_process(std::fs::File::open(&pio.input)?) {
            Ok(child) => {
                let results = child.wait_with_output().await?;
                info!("Finished running problem {}", self.name());
                Ok((pio, results))
            }
            Err(e) => Err(e),
        }
    }

    pub fn run_problems<'a>(
        &'a self,
        ios: &'a [ProblemIO],
    ) -> impl Stream<Item = Result<(&ProblemIO, Output)>> + 'a {
        let mut tasks = FuturesOrdered::new();
        for (_i, pio) in ios.iter().enumerate() {
            let task = self.run_problem(pio);
            tasks.push_back(task);
        }
        tasks
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

#[derive(Sequence, PartialEq, Clone, Eq, Debug)]
pub enum Lang {
    Cpp,
    Python,
    Rust,
}

impl Lang {
    pub const fn compiled(&self) -> bool {
        match self {
            Self::Cpp | Self::Rust => true,
            Self::Python => false,
        }
    }
    pub const fn extension(&self) -> &'static str {
        match self {
            Self::Cpp => "cpp",
            Self::Rust => "rs",
            Self::Python => "py",
        }
    }

    pub fn from_extension(ext: impl AsRef<str>) -> Option<Self> {
        match ext.as_ref() {
            "cpp" => Some(Self::Cpp),
            "py" => Some(Self::Python),
            "rs" => Some(Self::Rust),
            _ => None,
        }
    }

    pub fn is_valid_extension(ext: &str) -> bool {
        Self::from_extension(ext).is_some()
    }
}

/// Used by submission system
impl fmt::Display for Lang {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Cpp => "C++",
                Self::Python => "Python 3",
                Self::Rust => "Rust",
            }
        )
    }
}

// pub fn find_source(problem_name: &str) -> Vec<PathBuf> {
//     walkdir::WalkDir::new(".")
//         .max_depth(*RECURSE_DEPTH.get().unwrap())
//         .into_iter()
//         .filter_map(|f| {
//             if let Ok(de) = f {
//                 if let Some(s) = de.file_name().to_str() {
//                     let ends_with_extension = |l: Lang| { s.ends_with(&format!(".{}", l.extension())) };
//                     if s.starts_with(&format!("{problem_name}."))
//                         && any(all::<Lang>(), ends_with_extension)
//                     {
//                         return Some(de.path().to_path_buf());
//                     }
//                 }
//             }
//             None
//         })
//         .collect()
// }

pub fn find_source_from_path(path: &Path) -> Result<ProblemSource> {
    if !path.is_file() {
        bail!("Path {path:?} is not a file");
    }
    let extension = path.extension()
        .ok_or_else(|| anyhow!("Path {path:?} has no extension"))?;
    let lang = Lang::from_extension(extension.to_string_lossy())
        .ok_or_else(|| anyhow!("Extension {extension:?} from path {path:?} is not supported. Expected one of {}",
            all::<Lang>().map(|l| l.extension()).join(", ")))?;
    let problem_name = path.file_stem()
        .ok_or_else(|| anyhow!("Problem name not found in path {path:?}"))?;

    if block_on(fetch::problem_exists(&problem_name.to_string_lossy()))? {
        Ok(ProblemSource {
            problem_name: problem_name.to_string_lossy().to_string(),
            path: path.to_path_buf(),
            lang,
        })
    } else {
        bail!("Could not find the problem {problem_name:?} at open.kattis.com/problem/{problem_name:?}");
    }
}

pub struct ProblemSource {
    pub problem_name: String,
    pub path: PathBuf,
    pub lang: Lang,
}

pub fn find_newest_source() -> Result<ProblemSource> {
    let problem_path = walkdir::WalkDir::new(".")
        .max_depth(*RECURSE_DEPTH.get().unwrap())
        .into_iter().take(100_000)  // Look through at most 100_000 files
        .filter_map(|f| -> Option<DirEntry> {  // Filter out files that don't have the right extension
            let de = f.ok()?;

            let file_path = de.path();
            if !file_path.is_file() {return None;}; // Skip directories
            let file_extension = file_path.extension()?.to_string_lossy();
            if Lang::is_valid_extension(&file_extension) {
                Some(de)
            } else {
                None
            }
        })
        .max_by_key(|de|  // Find the file modified the latest
            de.metadata()
                .map_err(|e| anyhow!("Failed to get metadata from file with error: {e}"))
                .and_then(|x| x.modified().map_err(Into::into))
                .unwrap_or(SystemTime::UNIX_EPOCH)
        ).ok_or_else(|| anyhow!("No source files found."))?
        .into_path();  // Get the path of the file


    guard!(let Some(file_stem) = problem_path.file_stem() else {
        bail!("No file stem found for file {problem_path:?}.");
    });

    let problem_name = file_stem.to_string_lossy();

    if block_on(fetch::problem_exists(&problem_name))? {
        let extension = problem_path.extension()
            .ok_or_else(|| anyhow!("Path {problem_path:?} has no extension"))?;
        Ok(ProblemSource {
            problem_name: problem_name.to_string(),
            lang: Lang::from_extension(extension.to_string_lossy()).ok_or_else(|| anyhow!("Unrecognized extension"))?,
            path: problem_path,
        })
    } else {
        bail!(
            "Problem name {} does not exist on open.kattis.com",
            problem_name.bold()
        );
    }
}

struct ProblemInstance<'a> {
    program: &'a Program,
    result: ProblemInstanceResult,
}

/// The result of compiling and running a problem.
/// Either it ran and we have a list of results, or it failed to compile
enum ProblemInstanceResult {
    Ran(Vec<CaseRun>),
    CompileError(String),
}

struct CaseRun {
    case_name: String,
    run_result: RunResult,
}

impl CaseRun {
    pub const fn passed(&self) -> bool {
        match &self.run_result {
            RunResult::Completed(cr) => cr.failed.is_none(),
            RunResult::RuntimeError(_, _) => false,
        }
    }
}

pub enum RunResult {
    Completed(ComparisonResult),
    RuntimeError(String, String), // Output from stderr, stdout
}

lazy_static::lazy_static! {
    static ref SEGFAULT_RE: Regex = Regex::new(r"signal: (\d+)").unwrap();
}


/// Compiles, fetches, runs and compares problem
async fn check_problem(problem: &mut Problem, force: bool) -> Result<()> {
    let should_submit = problem.submit;
    // Fetch problem IO
    let future_io = fetch::problem(&problem.problem_name);

    // // Find source paths
    // if problem.submissions.is_empty() {
    //     println!("{}", &problem.problem_name.bright_white().bold());
    //     println!(
    //         "{}",
    //         format!(
    //             "{}{}{} ({}).\n",
    //             "Found no source code for problem ",
    //             &problem.problem_name.bold(),
    //             ". Make sure that the file exists with one of the supported extensions\n".red(),
    //             all::<Lang>()
    //                 .map(|e| e.extension())
    //                 .join(", ")
    //         )
    //         .red(),
    //     );
    //     return Ok(());
    // }

    // Compile programs and fetch the io for this problem
    let (compile_result, io) = join(problem.submission.compile(), future_io).await;
    compile_result?;

    // let compiled_programs = compiled_programs.into_iter().collect::<Vec<_>>();

    let io = io?;

    let problem_instance = run_problem(problem, &io).await;

    info!("Printing results");
    println!("{}", &problem.problem_name.bold());
    let program_name = problem_instance.program.name();
    match problem_instance.result {
        ProblemInstanceResult::Ran(cases) => {
            let mut failed_any = false;
            let mut case_print = String::new();
            for case in cases {
                if !case.passed() {
                    failed_any = true;
                }
                let result_print = match case.run_result {
                    RunResult::Completed(cr) => format!("{cr}\n"),
                    RunResult::RuntimeError(stderr, stdout) => {
                        let mut out = stderr.clone();
                        if !stdout.is_empty() {
                            out.push_str(&format!(
                                "\nBefore crashing, {program_name} outputted:\n{stdout}"
                            ));
                        }
                        out
                    }
                };
                case_print.push_str(&format!("{}\n", &case.case_name.yellow().bold()));
                case_print.push_str(&result_print);
            }
            println!("{program_name}\n{case_print}");

            if should_submit && (!failed_any || force) {
                if let Err(e) = problem_instance.program.submit(&problem.problem_name).await {
                    eprintln!("{}{e}", "Error:\n".bold().red());
                }
            }
        }
        ProblemInstanceResult::CompileError(compile_error) => {
            eprintln!("{compile_error}");
        }
    }
    info!("Print results");

    Ok(())
}

fn check_problem_output(pio: &ProblemIO, out: &Output) -> RunResult {
    #[cfg(unix)]
    let segfaulted = matches!(&out.status.signal(), Some(11));

    #[cfg(not(unix))]
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
        let pio_output_string: String =
            pio.get_output_string().unwrap();
        let compare_result =
            compare(&output_string, &pio_output_string);
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
}

async fn run_problem<'a>(problem: &'a Problem, ios: &'a [ProblemIO]) -> ProblemInstance<'a> {
    async fn run_submission<'b>(program: &'b Program, ios: &'b [ProblemIO]) -> ProblemInstance<'b> {
        match &program.compiled { // Guard against programs that aren't ready to run
            Some(Err(compile_error)) => return ProblemInstance {
                program,
                result: ProblemInstanceResult::CompileError(compile_error.clone())
            },
            None => panic!("Program was not attempted compiled (internal error, please report this)"),
            Some(Ok(())) => {}, // Continue to run program
        }

        // Stream of results coming from the async functions that are completing
        let mut result_stream = program.run_problems(ios);

        let mut results: Vec<CaseRun> = Vec::new();
        while let Some((pio, out)) = result_stream.try_next().await.unwrap() {
            let run_result = check_problem_output(pio, &out);
            results.push(CaseRun {
                case_name: pio.name.clone(),
                run_result,
            });
        }
        info!("Starting to run problems");

        ProblemInstance {
            program,
            result: ProblemInstanceResult::Ran(results)
        }
    }

    run_submission(&problem.submission, ios).await
}

#[cfg(test)]
mod test {
    use crate::checker::Lang;
    use enum_iterator::all;

    #[test]
    fn complete_langs() {
        let langs = all::<Lang>();
        for lang in langs {
            assert_eq!(Lang::from_extension(lang.extension()).unwrap(), lang);
        }
    }
}
