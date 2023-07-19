use anyhow::{anyhow, bail, Result};
use std::collections::HashMap;

use futures::io::SeekFrom;
use itertools::Itertools;
use log::info;
use std::env::temp_dir;
use std::fs;
use std::io::Read;
use std::path::Path;
use tempfile::TempPath;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, ErrorKind};
use std::convert::Into;
use std::ffi::OsStr;

pub fn initialize_temp_dir() -> Result<()> {
    let mut tmp_dir = std::env::temp_dir();
    tmp_dir.push("kattis/problem_files/");
    fs::create_dir_all(tmp_dir).map_err(Into::into)
}

#[derive(Debug)]
pub struct ProblemIO {
    pub name: String,
    pub input: TempPath,
    pub output: TempPath,
}

impl ProblemIO {
    pub fn new(name: String, t: (Option<TempPath>, Option<TempPath>)) -> Result<Self> {
        if let (Some(input), Some(output)) = t {
            Ok(Self {
                name,
                input,
                output,
            })
        } else {
            Err(anyhow!("Kattis zip missing input or output"))
        }
    }

    pub fn get_output_string(&self) -> Result<String> {
        let mut res = String::new();
        let mut output_file = fs::File::open(&self.output)?;
        output_file.read_to_string(&mut res)?;
        Ok(res)
    }
}

fn remove_suffix(s: &str, p: Vec<&str>) -> String {
    for pat in p {
        if let Some(stripped) = s.strip_suffix(pat) {
            return stripped.into();
        }
}
    s.into()
}

pub async fn problem(problem_name: &str) -> Result<Vec<ProblemIO>> {
    info!("Fetching problem {}", problem_name);
    // Fetch from Kattis
    let mut problem_path = temp_dir();
    problem_path.push(format!("kattis/problem_files/{problem_name}.zip"));

    let mut problem_file = match File::open(&problem_path).await {
        Ok(f) => f,
        Err(e) => match e.kind() {
            ErrorKind::NotFound => {
                log::warn!("Downloading problem files for {problem_name} from open.kattis.com...");
                let mut file = OpenOptions::new()
                    .write(true)
                    .read(true)
                    .create(true)
                    .open(&problem_path)
                    .await?;

                let tmp = reqwest::get(
                    format!("https://open.kattis.com/problems/{problem_name}/file/statement/samples.zip")
                )
                .await?
                .bytes()
                .await?;

                file.write_all(&tmp).await?;
                file.seek(SeekFrom::Start(0)).await?;
                // file.start_seek(SeekFrom::Start(0)).await?;
                file
            }
            _ => return Err(e.into()),
        },
    };

    let mut file_contents = Vec::with_capacity(problem_file.metadata().await?.len().try_into()?);
    problem_file.read_buf(&mut file_contents).await?;
    let cursor = std::io::Cursor::new(file_contents);

    let mut zip = zip::ZipArchive::new(cursor)?;
    let mut file_names: Vec<_> = zip.file_names().map(String::from).collect();
    file_names.sort();

    let mut io_map = HashMap::new();

    for file_name in file_names {
        let mut out_file = tempfile::NamedTempFile::new()?;
        let mut zipped_file_reader = zip.by_name(&file_name)?;
        std::io::copy(&mut zipped_file_reader, &mut out_file)?;
        let file_path = out_file.into_temp_path();
        let (ref mut i, ref mut o) = *io_map
            .entry(remove_suffix(&file_name, vec![".in", ".ans"]))
            .or_insert((None, None));

        let filename_path = Path::new(&file_name);
        let extension = filename_path.extension();
        if extension.map_or(false, |e| e.eq_ignore_ascii_case("in")) {
            *i = Some(file_path);
        } else if extension.map_or(false, |e| e.eq_ignore_ascii_case("ans")) {
            *o = Some(file_path);
        } else {
            bail!("Incompatible input format");
        }
    }

    info!("Problem {problem_name} fetched");
    io_map
        .into_iter()
        .map(|(name, io)| ProblemIO::new(name, io))
        .sorted_by(|a, b| {
            Ord::cmp(a.as_ref().map(|x| x.name.as_str()).unwrap_or(""),
                     b.as_ref().map(|x| x.name.as_str()).unwrap_or(""))
        })
        .collect()
}

pub async fn problem_exists(problem_name: &str) -> Result<bool> {
    use walkdir::DirEntry;
    let mut problem_path = temp_dir();
    problem_path.push("kattis/problem_files/");
    info!("Checking if problem exists locally at {problem_path:?}");

    let found_locally = walkdir::WalkDir::new(problem_path)
        .max_depth(1)
        .into_iter()
        .take(100_000)
        .any(|f| {
            // Strip the .zip off
            let pb: Option<&Path> = f.as_ref().ok().map(DirEntry::path);
            let s = pb.and_then(Path::file_stem).map(OsStr::to_string_lossy);
            s.map_or(false, |cow| cow == problem_name)
        });

    if found_locally {
        return Ok(true);
    }

    let str = reqwest::get(&format!("https://open.kattis.com/problems/{problem_name}"))
        .await?
        .text()
        .await?;

    info!("Result of problem_exists: {str}");

    Ok(!str.contains("404: Not Found"))
}
