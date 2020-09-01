use crate::Result;
use std::collections::HashMap;
use std::error;
use std::io::{Read, Write};

#[derive(Debug, Clone)]
pub struct ProblemIO {
    pub input: String,
    pub output: String,
}

impl ProblemIO {
    pub fn from_tuple(t: (Option<String>, Option<String>)) -> Result<Self> {
        match t {
            (Some(input), Some(output)) => Ok(ProblemIO { input, output }),
            _ => Err("Kattis zip missing input or output".into()),
        }
    }
}

fn remove_suffix(s: &str, p: Vec<&str>) -> String {
    for pat in p {
        if s.ends_with(pat) {
            return s[..(s.len() - pat.len())].into();
        }
    }
    s.into()
}

pub async fn fetch_problem(problem_name: &str) -> Result<Vec<ProblemIO>> {
    // Fetch from Kattis
    let mut tmpfile = tempfile::tempfile().unwrap();
    let tmp = reqwest::get(
        format!(
            "https://open.kattis.com/problems/{}/file/statement/samples.zip",
            problem_name
        )
        .as_str(),
    )
    .await?
    .bytes()
    .await?;

    tmpfile.write(&*tmp)?;

    let mut zip = zip::ZipArchive::new(tmpfile).unwrap();
    let mut file_names: Vec<_> = zip.file_names().map(String::from).collect();
    file_names.sort();

    let mut io_map = HashMap::new();

    for file_name in file_names {
        let mut s = String::new();
        zip.by_name(&file_name)
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();
        match *io_map
            .entry(remove_suffix(&file_name, vec![".in", ".ans"]).clone())
            .or_insert((None, None))
        {
            (ref mut i, ref mut o) => {
                if file_name.ends_with(".in") {
                    *i = Some(s);
                } else if file_name.ends_with(".ans") {
                    *o = Some(s);
                } else {
                    return Err("Incompatible input format".into());
                }
            }
        };
    }

    io_map
        .into_iter()
        .map(|(_name, io)| ProblemIO::from_tuple(io))
        .collect::<Result<Vec<_>>>()
}
