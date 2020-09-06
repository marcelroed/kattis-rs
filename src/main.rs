#![feature(in_band_lifetimes)]
#![feature(async_closure)]
use clap::{App, Arg};

use futures::executor::block_on;
use std::error;

mod checker;
mod compare;
mod fetch;

type Result<T> = std::result::Result<T, Box<dyn error::Error + Send + Sync + 'static>>;

const VERSION: &'static str = env!("CARGO_PKG_VERSION");

#[tokio::main]
pub async fn main() {
    // Create folder in tmp if it doesn't already exist
    let mut kattis_temp = std::env::temp_dir();
    kattis_temp.push("kattis/");

    std::fs::create_dir_all(kattis_temp).unwrap();
    let mut app = App::new("Kattis Tester")
        .version(VERSION)
        .author("Marcel Rød")
        .about("Tests Kattis competitive programming problems.")
        .arg(
            Arg::new("problems")
                .about(
                    "Names of the problems to test.\
                    The format needs to be {problem} in open.kattis.com/problems/{problem}. \
                    If left empty, the problem name will be the name of the last edited source file. \
                    Make sure that source files use the file name stem {problem}.",
                )
                .required(false)
                .multiple(true)
                .value_name("PROBLEMS"),
        );
    let matches = app.get_matches_mut();

    let problems: Vec<_> = match matches.values_of("problems") {
        Some(problem_names) => problem_names.map(String::from).collect(),
        None => match checker::find_newest_source() {
            Ok(pname) => vec![pname],
            Err(e) => {
                eprintln!("Although kattis can be used without arguments, this requires the latest edited submittable in this directory to be a kattis file.\nEncountered error: {}\nPerhaps you wanted the regular usage?", e.to_string());
                eprintln!("{}", app.generate_usage());
                std::process::exit(0);
            }
        },
    };

    block_on(checker::check_problems(problems));
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
