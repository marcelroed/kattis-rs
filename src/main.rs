#![feature(async_closure)]

use clap::{App, Arg};

use crate::checker::Problem;
use std::collections::HashSet;
use std::error;
use tokio::io;

mod checker;
mod compare;
mod fetch;
mod submit;

type Result<T> = std::result::Result<T, Box<dyn error::Error + Send + Sync + 'static>>;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
pub async fn main() -> io::Result<()> {
    // Create folder in tmp if it doesn't already exist
    if let Err(e) = fetch::initialize_temp_dir() {
        eprintln!("{}", e);
    }

    let mut app = App::new("Kattis Tester")
        .version(VERSION)
        .author("Marcel Rød")
        .about("Tests Kattis competitive programming problems.")
        .arg(
            Arg::new("problems")
                .help(
                    "Names of the problems to test.\
                    The format needs to be {problem} in open.kattis.com/problems/{problem}. \
                    If left empty, the problem name will be the name of the last edited source file. \
                    Make sure that source files use the file name stem {problem}.",
                )
                .allow_invalid_utf8(true)
                .required(false)
                .min_values(0)
                .multiple_occurrences(true)
                .value_name("PROBLEM"))
        .arg(
            Arg::new("submit")
                .help("Problems after this flag are submitted if successful.\
                           If no problems are listed, use problems from regular args.")
                .allow_invalid_utf8(true)
                .multiple_values(true)
                .required(false)
                .min_values(0)
                .short('s')
                .long("submit")
                .value_name("SUBMIT_PROBLEM"))
        .arg(
            Arg::new("force")
                .help("Force submission even if submitted problems don't pass local tests.")
                .short('f')
                .requires("submit")
                .takes_value(false)
                .long("force")
        );
    let matches = app.get_matches_mut();
    let force = matches.is_present("force");

    let problem_names: Vec<_> = {
        let mut problems = matches.values_of_lossy("problems").unwrap_or_default();

        if let Some(subs) = matches.values_of_lossy("submit") {
            problems.append(&mut subs.into_iter().filter(|s| !problems.contains(s)).collect());
        }

        if problems.is_empty() {
            match checker::find_newest_source() {
                Ok(pname) => vec![pname],
                Err(e) => {
                    eprintln!(
                        "Although kattis can be used without arguments, \
                        this requires the latest edited file in this directory to be a kattis file.\
                        \nEncountered error: {}\n\
                        Perhaps you wanted the regular usage?",
                        e
                    );
                    eprintln!("{}", app.render_usage());
                    std::process::exit(2);
                }
            }
        } else {
            problems
        }
    };

    let to_submit: HashSet<String> = match (
        matches.is_present("submit"),
        matches.values_of_lossy("submit"),
    ) {
        (false, _) => vec![],
        (true, Some(sub)) => {
            if sub.is_empty() {
                problem_names.clone()
            } else {
                sub
            }
        }
        (true, None) => problem_names.clone(),
    }
    .into_iter()
    .collect();

    let problems: Vec<Problem> = problem_names
        .into_iter()
        .filter_map(|problem_name| checker::Problem::new(&problem_name).ok())
        .map(|problem| {
            let should_submit = to_submit.contains(&problem.problem_name);
            problem.submit(should_submit)
        })
        .collect();

    let _problem_results = checker::check_problems(problems, force).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!("2".parse::<usize>().unwrap() + 2, 4);
    }
}
