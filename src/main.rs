#![feature(in_band_lifetimes)]
#![feature(async_closure)]
#![feature(try_trait)]

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
            Arg::with_name("problems")
                .about(
                    "Names of the problems to test.\
                    The format needs to be {problem} in open.kattis.com/problems/{problem}. \
                    If left empty, the problem name will be the name of the last edited source file. \
                    Make sure that source files use the file name stem {problem}.",
                )
                .required(false)
                .min_values(0)
                .multiple(true)
                .value_name("PROBLEM"))
        .arg(
            Arg::with_name("submit")
                .about("Problems after this flag are submitted if successful.\
                           If no problems are listed, use problems from regular args.")
                .multiple_values(true)
                .required(false)
                .min_values(0)
                .short('s')
                .long("submit")
                .value_name("SUBMIT_PROBLEM"))
        .arg(
            Arg::with_name("force")
                .about("Force submission even if submitted problems don't pass local tests.")
                .short('f')
                .long("force")
        );
    let matches = app.get_matches_mut();

    // println!(
    //     "{:?} {:?} {}",
    //     matches
    //         .values_of("problems")
    //         .unwrap()
    //         .map(String::from)
    //         .collect::<Vec<_>>(),
    //     matches
    //         .values_of("submit")
    //         .unwrap()
    //         .map(String::from)
    //         .collect::<Vec<_>>(),
    //     matches.is_present("force")
    // );

    let problem_names: Vec<_> = {
        let mut problems = match matches.values_of_lossy("problems") {
            Some(problems) => problems,
            None => vec![],
        };
        if let Some(subs) = matches.values_of_lossy("submit") {
            problems.append(&mut subs.into_iter().filter(|s| !problems.contains(s)).collect());
        }

        if problems.is_empty() {
            match checker::find_newest_source() {
                Ok(pname) => vec![pname],
                Err(e) => {
                    eprintln!("Although kattis can be used without arguments, this requires the latest edited file in this directory to be a kattis file.\nEncountered error: {}\nPerhaps you wanted the regular usage?", e.to_string());
                    eprintln!("{}", app.generate_usage());
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

    let _problem_results = checker::check_problems(problems).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
