use crate::Result;
use std::collections::HashMap;

use futures::io::SeekFrom;
use itertools::Itertools;
use std::cmp::Ordering;
use std::env::temp_dir;
use std::io::Read;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt, ErrorKind};

pub fn initialize_temp_dir() -> Result<()> {
    let mut tmp_dir = std::env::temp_dir();
    tmp_dir.push("kattis/problem_files/");
    std::fs::create_dir_all(tmp_dir).map_err(|e| e.into())
}

#[derive(Debug, Clone)]
pub struct ProblemIO {
    pub name: String,
    pub input: String,
    pub output: String,
}

impl ProblemIO {
    pub fn new(name: String, t: (Option<String>, Option<String>)) -> Result<Self> {
        if let (Some(input), Some(output)) = t {
            Ok(ProblemIO {
                name,
                input,
                output,
            })
        } else {
            Err("Kattis zip missing input or output".into())
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
    let mut problem_path = std::env::temp_dir();
    problem_path.push(format!("kattis/problem_files/{}.zip", problem_name));

    let mut problem_file = match File::open(&problem_path).await {
        Ok(f) => f,
        Err(e) => match e.kind() {
            ErrorKind::NotFound => {
                let mut file = OpenOptions::new()
                    .write(true)
                    .read(true)
                    .create(true)
                    .open(&problem_path)
                    .await?;

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

                file.write_all(&*tmp).await?;
                file.seek(SeekFrom::Start(0)).await?;
                file
            }
            _ => return Err(e.into()),
        },
    };

    let mut file_contents = Vec::with_capacity(problem_file.metadata().await?.len() as usize);
    problem_file.read_buf(&mut file_contents).await?;
    let cursor = std::io::Cursor::new(file_contents);

    let mut zip = zip::ZipArchive::new(cursor)?;
    let mut file_names: Vec<_> = zip.file_names().map(String::from).collect();
    file_names.sort();

    let mut io_map = HashMap::new();

    for file_name in file_names {
        let mut s = String::new();
        zip.by_name(&file_name)
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();
        let (ref mut i, ref mut o) = *io_map
            .entry(remove_suffix(&file_name, vec![".in", ".ans"]).clone())
            .or_insert((None, None));

        if file_name.ends_with(".in") {
            *i = Some(s);
        } else if file_name.ends_with(".ans") {
            *o = Some(s);
        } else {
            return Err("Incompatible input format".into());
        }
    }

    io_map
        .into_iter()
        .map(|(name, io)| ProblemIO::new(name, io))
        .sorted_by_key(|rpio| match rpio {
            Ok(pio) => pio.name.clone(),
            Err(e) => "zzzzz".to_string(),
        })
        .collect::<Result<Vec<_>>>()
}

pub async fn problem_exists(problem_name: &str) -> Result<bool> {
    let mut problem_path = temp_dir();
    problem_path.push("kattis/problem_files/");
    let problem_names: Vec<_> = walkdir::WalkDir::new(problem_path)
        .max_depth(1)
        .into_iter()
        .map(|f| {
            let de = f.unwrap();
            let s = de.file_name().to_str().unwrap();
            s[..s.len() - 3].to_owned()
        })
        .collect();

    if problem_names.iter().any(|pn| pn == problem_name) {
        return Ok(true);
    }

    let str = reqwest::get(format!("https://open.kattis.com/problems/{}", problem_name).as_str())
        .await?
        .text()
        .await?;

    Ok(!str.contains("404: Not Found"))
}
