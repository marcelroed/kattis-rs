#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use crate::checker::{find_source_from_path, Problem, ProblemSource};
use anyhow::{Context, Result};
use clap::builder::NonEmptyStringValueParser;
use clap::parser::ValueSource;
use clap::{arg, crate_version, ArgAction, Command, ValueHint};
use colored::Colorize;
use log::{info, warn};
use std::path::Path;
use std::sync::OnceLock;
use submit::viewer;

mod checker;
mod compare;
mod fetch;
mod submit;

pub static RECURSE_DEPTH: OnceLock<usize> = OnceLock::new();

#[allow(clippy::cognitive_complexity)]
fn build_cli() -> Command {
    Command::new("Kattis Tester")
        .version(crate_version!())
        .author("Marcel Rød")
        .about("Tests and submits Kattis competitive programming problems.")
        .arg(
            arg!([problems] ...)
                .help(
                    "Paths of files to test or no arguments.\nFilenames should be of the format {problem}.{ext} \
                    where {problem} can be found from the url of the kattis problem \
                    at open.kattis.com/problems/{problem}.\n\
                    If left empty, the problem to run will be inferred by looking for \
                    the latest edited valid source file in the working directory.",
                )
                .required(false)
                .value_parser(NonEmptyStringValueParser::new())
                .value_hint(ValueHint::FilePath)
                .value_name("PROBLEM"))
        .arg(
            arg!(--submit)
                .short('s')
                .help("If flag is set, all successful problems will be submitted.")
                .required(false)
                .default_value("false")
                .action(ArgAction::SetTrue))
        .arg(
            arg!(--force)
                .short('f')
                .help("Force submission even if submitted problems don't pass local tests.")
                .required(false)
                .default_value("false")
                .requires("submit")
                .action(ArgAction::SetTrue)
        )
        .arg(
            arg!(--recurse <DEPTH>)
                .short('r')
                .help("Number of directory levels to recurse into when searching for problem solutions.")
                .required(false)
                .value_parser(|s: &str| s.parse::<usize>().or_else(|e| {
                    if s.to_lowercase() == "true" { Ok(100) } else { Err(e) }
                }))
                .default_value("1")
                .action(ArgAction::Set)
        )
        .arg(
            arg!(--"submission-viewer")
                .help("Viewer to use for submission.")
                .required(false)
                .default_value("cli")
                // .requires("submit")  // Warn instead of disallowing
                .action(ArgAction::Set)
                // .value_hint(ValueHint)
                .value_parser(viewer::SubmissionViewerParser)
        )
}

/// # Panics
/// Panics if something goes wrong.
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
pub async fn main() {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "warn");
    }
    pretty_env_logger::init();
    // Create folder in tmp if it doesn't already exist
    if let Err(e) = fetch::initialize_temp_dir() {
        eprintln!("{e}");
    }

    let mut app = build_cli();

    let matches = app.get_matches_mut();
    let force_flag: bool = matches.get_one("force").copied().unwrap_or(false);
    let submit_flag: bool = matches.get_one("submit").copied().unwrap_or(false);
    let recurse_depth: usize = matches.get_one("recurse").copied().unwrap_or(0);
    let submission_viewer: viewer::SubmissionViewerType =
        matches.get_one("submission-viewer").copied().unwrap();

    if matches!(
        matches.value_source("submission-viewer"),
        Some(ValueSource::CommandLine)
    ) && !submit_flag
    {
        warn!(
            "{0} flag is set but {1} is not. Ignoring {0}.",
            "--submission-viewer".bold(),
            "--submit".bold()
        );
    }

    RECURSE_DEPTH.set(recurse_depth).unwrap();
    info!("Recursing {} levels into directories.", recurse_depth);

    let problem_args: Vec<&str> = matches
        .get_many::<String>("problems")
        .unwrap_or_default()
        .map(String::as_str)
        .collect();

    let problem_sources: Vec<ProblemSource> = {
        if problem_args.is_empty() {
            // Look for newest source file
            match checker::find_newest_source() {
                Ok(problem_source) => vec![problem_source],
                Err(e) => {
                    eprintln!(
                        "Although kattis can be used without problem name arguments, \
                        this requires the latest edited file in this directory to be a kattis source code file.\
                        \nEncountered error: {e}\n\
                        Perhaps you wanted the regular usage?"
                    );
                    eprintln!("{}", app.render_help());
                    std::process::exit(1);
                }
            }
        } else {
            // Use the source files specified
            problem_args
                .into_iter()
                .map(Path::new)
                .map(find_source_from_path)
                .collect::<Result<Vec<_>>>()
                .context("Failed to find source files.")
                .unwrap()
        }
    };

    let problems: Vec<Problem> = problem_sources
        .into_iter()
        .map(Problem::new)
        .map(|problem| problem.set_submit(submit_flag))
        .collect();

    let mut failed_any: bool = false;
    checker::check_problems(problems, force_flag, submission_viewer)
        .await
        .into_iter()
        .for_each(|(problem, res)| match res {
            Err(e) => eprintln!("Failed to check problem {}: {e}", problem.problem_name),
            Ok(b) if !b => failed_any = true,
            _ => {}
        });

    std::process::exit(i32::from(failed_any));
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!("2".parse::<usize>().unwrap() + 2, 4);
    }
}
