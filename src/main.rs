use clap::{App, Arg};

use futures::executor::block_on;
use std::error;

mod checker;
mod fetch;

type Result<T> = std::result::Result<T, Box<dyn error::Error>>;

#[tokio::main]
pub async fn main() {
    let matches = App::new("Kattis Tester")
        .version("0.1")
        .author("Marcel Rød")
        .about("Tests Kattis competitive programming problems.")
        .arg(Arg::new("problems")
            // .short('p')
            .about("Names of the problems to test. The format needs to be {problem} in open.kattis.com/problems/{problem}")
            .required(true)
            .multiple(true)
            .value_name("PROBLEMS")
        )
        .get_matches();

    let problems: Vec<_> = matches
        .values_of("problems")
        .expect("Problems not provided")
        .map(String::from)
        .collect();
    println!("{:?}", problems);

    block_on(checker::check_problems(problems));
    println!("Finished running")
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
