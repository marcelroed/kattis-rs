#![feature(async_closure)]

use clap::{arg, crate_version, Command, ArgAction};

use crate::checker::Problem;
use std::collections::HashSet;

mod checker;
mod compare;
mod fetch;
mod submit;

#[tokio::main]
pub async fn main(){
    // Create folder in tmp if it doesn't already exist
    if let Err(e) = fetch::initialize_temp_dir() {
        eprintln!("{}", e);
    }

    let mut app = Command::new("Kattis Tester")
        .version(crate_version!())
        .author("Marcel Rød")
        .about("Tests and submits Kattis competitive programming problems.")
        .arg(
            arg!([problems] ...)
                .help(
                    "Names of the problems to test. \
                    The format needs to be {problem} in open.kattis.com/problems/{problem}. \
                    If left empty, the problem name will be the name of the last edited source file. \
                    Make sure that source files use the file name stem {problem}, e.g. {problem}.py.",
                )
                // .allow_invalid_utf8(true)
                .required(false)
                // .min_values(0)
                // .multiple_occurrences(true)
                .value_name("PROBLEM"))
        .arg(
            arg!(--submit)
                .help("Problems after this flag are submitted if successful. \
                           If no problems are listed, use problems from regular args.")
                // .allow_invalid_utf8(true)
                // .multiple_values(true)
                .required(false)
                // .min_values(0)
                .num_args(0..)
                .short('s')
                .long("submit")
                .action(ArgAction::Append)
                .value_name("SUBMIT_PROBLEM"))
        .arg(
            arg!(--force)
                .help("Force submission even if submitted problems don't pass local tests.")
                .short('f')
                .default_value("false")
                .requires("submit")
                .action(ArgAction::SetTrue)
                // .takes_value(false)
                .long("force")
        );
    let matches = app.get_matches_mut();
    let force: bool = *matches.get_one("force").unwrap();

    let problem_names: Vec<_> = {
        let mut problems: Vec<String> = matches.try_get_many("problems").unwrap_or_default().unwrap_or_default().map(|s: &String| (*s).clone()).collect();

        if let Some(subs) = matches.get_many("submit") {
            problems.append(&mut subs.into_iter()
                .filter(|s| !problems.contains(s))
                .map(|s| s.to_owned())
                .collect());
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
        matches.get_many::<String>("submit").is_some(),
        matches.try_get_many::<String>("submit").unwrap_or_default(),
    ) {
        (false, _) => vec![],
        (true, Some(sub)) => {
            if sub.len() == 0 {
                problem_names.clone()
            } else {
                sub
                    .map(|v| v.to_owned()).collect::<Vec<_>>()
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
    // Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!("2".parse::<usize>().unwrap() + 2, 4);
    }
}
