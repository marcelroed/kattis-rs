use crate::Result;
use colored::Colorize;
use regex::Regex;
use reqwest::header;
use reqwest::multipart;
use std::fmt::Debug;
use std::io::{Error, ErrorKind};
use tokio::fs::File;
use tokio::io::AsyncReadExt;

#[derive(Clone, Debug)]
struct KattisConfig {
    username: String,
    token: String,
    login_url: String,
    submit_url: String,
    submissions_url: String,
}

lazy_static::lazy_static! {
    static ref ID_RE: Regex = Regex::new(r"Submission ID: (\d+)").unwrap();
}

async fn get_config() -> Result<KattisConfig> {
    let mut rc = dirs::home_dir().ok_or_else(|| {
        Error::new(
            ErrorKind::NotFound,
            "Couldn't find home directory on your OS.",
        )
    })?;
    rc.push(".kattisrc");

    if rc.is_file() {
        let mut config_file = File::open(rc).await?;
        let mut config_string = String::new();
        config_file.read_to_string(&mut config_string).await?;
        config_string = config_string.replace(": ", "=");
        let config = configparser::ini::Ini::new().read(config_string)?;
        Ok(KattisConfig {
            username: config["user"]["username"]
                .as_ref()
                .expect("Failed to read .kattisrc")
                .to_owned(),
            token: config["user"]["token"]
                .as_ref()
                .expect("Failed to read .kattisrc")
                .to_owned(),
            login_url: config["kattis"]["loginurl"]
                .as_ref()
                .expect("Failed to read .kattisrc")
                .to_owned(),
            submit_url: config["kattis"]["submissionurl"]
                .as_ref()
                .expect("Failed to read .kattisrc")
                .to_owned(),
            submissions_url: config["kattis"]["submissionsurl"]
                .as_ref()
                .expect("Failed to read .kattisrc")
                .to_owned(),
        })
    } else {
        rc.pop();
        Err(format!(
            "\
Failed to read in a config file from your home directory.
In order to submit code from the CLI, you need to be authenticated.
Please go to https://open.kattis.com/download/kattisrc to download 
your personal config file, and place it in your home 
folder ({}) as .kattisrc

The file should look something like this:
[user]
username: yourusername
token: *********

[kattis]
loginurl: https://<kattis>/login
submissionurl: https://<kattis>/submit
        ",
            rc.to_str().unwrap()
        )
        .into())
    }
}

pub async fn submit(
    language: String,
    problem: String,
    submission_filename: String,
    submission: String,
) -> Result<()> {
    let config = get_config().await?;
    let mut default_headers = header::HeaderMap::new();
    default_headers.insert(
        header::USER_AGENT,
        header::HeaderValue::from_static("kattis-cli-submit"),
    );
    let client = reqwest::ClientBuilder::new()
        .default_headers(default_headers)
        .cookie_store(true)
        .build()?;

    // Login
    let login_map = serde_json::json!({
        "user": config.username.as_str(),
        "script": "true",
        "token": config.token.as_str(),
    });

    let _login_response = client
        .post(&config.login_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&login_map)
        .send()
        .await?;

    // Make a submission
    let submission_map = serde_json::json!({
        "submit": "true",
        "submit_ctr": "2",
        "language": language,
        "mainclass": problem,
        "problem": problem,
        "script": "true",
    });

    let mut form = multipart::Form::new();
    for (k, v) in submission_map.as_object().unwrap() {
        form = form.text(k.to_string(), v.as_str().unwrap().to_string());
    }

    let mut sub_file = multipart::Part::bytes(submission.as_bytes().to_owned())
        .file_name(submission_filename.clone());
    sub_file = sub_file.mime_str("application/octet-stream").unwrap();

    form = form.part("sub_file[]", sub_file);
    let submission_response = client
        .post(&config.submit_url)
        .multipart(form)
        .send()
        .await?
        .text()
        .await?;

    if let Some(submission_id) = ID_RE.captures(&submission_response) {
        let submission_id = submission_id.get(1).unwrap().as_str();
        println!(
            "{}\n",
            format!(
                "Submitted {}. Opening submission in browser...",
                &submission_filename
            )
            .as_str()
            .green()
        );
        open::that(format!("{}/{}", config.submissions_url, submission_id))?;
        Ok(())
    } else {
        Err("Failed to read submission ID from submission response".into())
    }
}
