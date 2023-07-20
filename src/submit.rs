use anyhow::{anyhow, bail, Result};
use clap::builder::{PossibleValue, TypedValueParser};
use clap::error::{ContextKind, ContextValue};
use clap::{Arg, Command};
use colored::{ColoredString, Colorize};
use enum_iterator::Sequence;
use lazy_static::lazy_static;
use log::info;
use regex::Regex;
use reqwest::multipart;
use reqwest::{header, Client};
use serde::Deserialize;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fmt::{Debug, Display, Formatter};
use std::io::{Error, ErrorKind};
use std::sync::OnceLock;
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

impl KattisConfig {
    pub fn from_config(
        mut config: HashMap<String, HashMap<String, Option<String>>>,
    ) -> Result<Self> {
        let mut read_setting =
            |first, second| -> Option<String> { config.get_mut(first)?.remove(second)? };

        let mut read_setting_with_error = |first, second| -> Result<String> {
            read_setting(first, second)
                .ok_or_else(|| anyhow!("Failed to read {}.{} from .kattisrc", first, second))
        };

        Ok(Self {
            username: read_setting_with_error("user", "username")?,
            token: read_setting_with_error("user", "token")?,
            login_url: read_setting_with_error("kattis", "loginurl")?,
            submit_url: read_setting_with_error("kattis", "submissionurl")?,
            submissions_url: read_setting_with_error("kattis", "submissionsurl")?,
        })
    }
}

lazy_static! {
    static ref ID_RE: Regex = Regex::new(r"Submission ID: (\d+)").unwrap();
}

fn display_link(url: &str) -> String {
    display_link_with_name(url, url)
}

fn display_link_with_name(url: &str, name: &str) -> String {
    format!("\u{1b}]8;;{url}\u{1b}\\{name}\u{1b}]8;;\u{1b}\\")
}

fn name_with_maybe_link(name: &str, url: Option<&str>) -> String {
    url.map_or_else(
        || name.to_string(),
        |url| display_link_with_name(url, name)
    )
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
        // config_string = config_string.replace(": ", "="); // Not needed since default allows ':' for delimiter
        let config = configparser::ini::Ini::new()
            .read(config_string)
            .map_err(|e| {
                anyhow!("Failed to read .kattisrc file with error:\n{e}\nPerhaps it is corrupt?")
            })?;
        KattisConfig::from_config(config)
    } else {
        rc.pop();
        let link = display_link("https://open.kattis.com/download/kattisrc");
        bail!(
            "\
Failed to read in a config file from your home directory.
In order to submit code from the CLI, you need to be authenticated.
Please go to {link} to download
your personal config file, and place it in your home 
directory (detected to be {}) as .kattisrc

The file should look something like this:
[user]
username: yourusername
token: *********

[kattis]
loginurl: https://<kattis>/login
submissionurl: https://<kattis>/submit
        ",
            rc.to_str().unwrap_or("[Failed to detect home directory]")
        );
    }
}

pub async fn submit(
    language: String,
    problem: String,
    submission_filename: String,
    submission: String,
    submission_viewer: SubmissionViewer,
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
    sub_file = sub_file.mime_str("application/octet-stream")?;

    form = form.part("sub_file[]", sub_file);
    let submission_response = client
        .post(&config.submit_url)
        .multipart(form)
        .send()
        .await?
        .text()
        .await?;

    if let Some(submission_id) = ID_RE.captures(&submission_response) {
        use SubmissionViewer::{Browser, Cli, None};

        let submission_id = submission_id.get(1).unwrap().as_str();
        eprintln!(
            "{}",
            format!("Submitted {submission_filename}.")
                .as_str()
                .green()
        );
        match submission_viewer {
            Browser => {
                eprintln!("Opening submission in browser...");
                open::that(format!("{}/{}", config.submissions_url, submission_id))?;
            }
            Cli => {
                eprintln!();
                view_submission_in_terminal(client, submission_id).await?;
            }
            None => {}
        }
        Ok(())
    } else {
        bail!("Failed to read submission ID from submission response");
    }
}

#[derive(Deserialize, Debug)]
struct SubmissionResponse {
    #[serde(rename = "status_id")]
    status: SubmissionStatus,
    testcase_index: usize,
    // testdata_groups_html: String,
    // feedback_html: String,
    // judge_feedback_html: String,
    row_html: String,
}

impl SubmissionResponse {
    fn total_testcases(&self) -> Option<&str> {
        // Might not have given this value yet
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| Regex::new(".*/([0-9]+)").unwrap());
        Some(re.captures(&self.row_html)?.get(1)?.as_str())
    }
    const fn solved_testcases(&self) -> usize {
        self.testcase_index
    }
    fn problem_name(&self) -> Option<&str> {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| Regex::new("data-type=\"problem\".*?><.*?>(.*?)<").unwrap());
        Some(re.captures(&self.row_html)?.get(1)?.as_str())
    }
    fn cpu_time(&self) -> Option<&str> {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| Regex::new("data-type=\"cpu\".*?>(.*?)&").unwrap());
        Some(re.captures(&self.row_html)?.get(1)?.as_str())
    }
    fn language(&self) -> Option<&str> {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| Regex::new("data-type=\"lang\".*?>(.*?)<").unwrap());
        Some(re.captures(&self.row_html)?.get(1)?.as_str())
    }
    fn problem_slug(&self) -> Option<&str> {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| Regex::new("data-type=\"problem\".*?href=\"(.*?)\"").unwrap());
        Some(re.captures(&self.row_html)?.get(1)?.as_str())
    }
    fn submission_id(&self) -> Option<&str> {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| Regex::new("data-submission-id=\"(.*?)\"").unwrap());
        Some(re.captures(&self.row_html)?.get(1)?.as_str())
    }
}

lazy_static! {
    static ref X_MARK: ColoredString = "✘".red();
    static ref CHECK_MARK: ColoredString = "✔".green();
}

impl Display for SubmissionResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.status == SubmissionStatus::New {
            write!(f, "Queuing submission...")
        } else if !self.status.is_terminal() {
            write!(
                f,
                "Test Cases: {: >3}{}{: <3}",
                self.solved_testcases().to_string().green().bold(),
                "/".bold(),
                self.total_testcases().unwrap_or("?").bold()
            )
        } else if self.status == SubmissionStatus::Accepted {
            let mut accepted_text: ColoredString = "Submission Accepted!".into();
            let submission_link = self.submission_id().map(|id| format!("https://open.kattis.com/submissions/{id}"));
            accepted_text = name_with_maybe_link(&accepted_text, submission_link.as_deref()).green().bold();

            write!(f, "{accepted_text} ")?;
            if let Some(problem_name) = self.problem_name() {
                write!(f, "{}", name_with_maybe_link(&problem_name.bold(),
                                                     self.problem_slug().map(|slug| format!("https://open.kattis.com{slug}")).as_deref()))?;
                if let Some(lang) = self.language() {
                    write!(f, " ({})", lang.bold())?;
                }
                if let Some(time) = self.cpu_time() {
                    if let Some(slug) = self.problem_slug() {
                        let url = format!("https://open.kattis.com{slug}/statistics");
                        let seconds_with_link = display_link_with_name(&url, &format!("{time}s"));
                        write!(f, " ran in {}", seconds_with_link.bold())?;
                    } else {
                        write!(f, " ran in {}s", time.bold())?;
                    }
                }
                writeln!(f)?;
            }
            Ok(())
        } else {
            write!(
                f,
                "{: >3}{}{: <3} {}",
                self.solved_testcases().to_string().red().bold(),
                "/".bold(),
                self.total_testcases().unwrap_or("?").bold(),
                self.status.to_string().red().bold(),
            )?;
            if let Some(time) = self.cpu_time() {
                write!(f, "{}",
                       format!(" after {}{}", time.bold(), "s".bold()).red())?;
            }
            Ok(())
        }
    }
}

fn reset_line() {
    eprint!("\x1B[2K\r");
}

async fn view_submission_in_terminal(client: Client, submission_id: &str) -> Result<()> {
    async {
        let mut written_first = false;
        let mut count = 0;
        loop {
            let response = client
                .get(format!(
                    "https://open.kattis.com/submissions/{submission_id}?json"
                ))
                .send()
                .await?;
            let r = response.json::<SubmissionResponse>().await?;

            if written_first { reset_line(); } else { written_first = true; } // Clear and move to start of line

            eprint!("{r}");
            if r.status.is_terminal() {
                info!("Queried Kattis {count} times");
                return Ok(());
            }
            // eprintln!("Submission still running. Checking again in 1 second...");
            // tokio::time::sleep(Duration::from_secs(1)).await;
            // view_submission_in_terminal(client, submission_id).await
            count += 1;
        }
    }.await
}

use serde_repr::Deserialize_repr;

#[derive(Clone, Copy, Deserialize_repr, Debug, Ord, PartialOrd, PartialEq, Eq)]
#[repr(u8)]
enum SubmissionStatus {
    New = 0,
    NewInvalid, // Should never happen
    WaitingForCompile,
    Compiling,
    WaitingForRun,
    Running,
    JudgeError,
    SubmissionError,
    CompileError,
    RuntimeError,
    MemoryLimitExceeded,
    OutputLimitExceeded,
    TimeLimitExceeded,
    IllegalFunction,
    WrongAnswer,
    Invalid, // Should never happen
    Accepted = 16,
}

impl Display for SubmissionStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        #[allow(clippy::enum_glob_use)]
        use SubmissionStatus::*;
        write!(
            f,
            "{}",
            match self {
                New => "Pending",
                NewInvalid => "New (Invalid)",
                WaitingForCompile => "Waiting for compile",
                Compiling => "Compiling",
                WaitingForRun => "Waiting for run",
                Running => "Running",
                JudgeError => "Judge error",
                SubmissionError => "Submission error",
                CompileError => "Compile error",
                RuntimeError => "Runtime error",
                MemoryLimitExceeded => "Memory limit exceeded",
                OutputLimitExceeded => "Output limit exceeded",
                TimeLimitExceeded => "Time limit exceeded",
                IllegalFunction => "Illegal function",
                WrongAnswer => "Wrong answer",
                Invalid => "Invalid Response",
                Accepted => "Accepted",
            }
        )
    }
}

impl SubmissionStatus {
    pub const fn is_terminal(self) -> bool {
        #[allow(clippy::enum_glob_use)]
        use SubmissionStatus::*;
        matches!(
            self,
            JudgeError
                | SubmissionError
                | CompileError
                | RuntimeError
                | MemoryLimitExceeded
                | OutputLimitExceeded
                | TimeLimitExceeded
                | IllegalFunction
                | WrongAnswer
                | Invalid
                | Accepted
        )
    }
}

#[derive(Debug, Clone, Copy, Sequence)]
pub enum SubmissionViewer {
    /// See results in the CLI, blocks until submission has finished
    Cli,
    /// Open a new browser window and terminate
    Browser,
    /// Just terminate, ignoring submission result
    None,
}

#[derive(Clone)]
pub struct SubmissionViewerParser;

impl TypedValueParser for SubmissionViewerParser {
    type Value = SubmissionViewer;

    fn parse_ref(
        &self,
        cmd: &Command,
        arg: Option<&Arg>,
        value: &OsStr,
    ) -> std::result::Result<Self::Value, clap::Error> {
        use SubmissionViewer::{Browser, Cli, None};
        match value.to_str().unwrap().to_lowercase().as_str() {
            "cli" => Ok(Cli),
            "browser" => Ok(Browser),
            "none" => Ok(None),
            _ => Err({
                let mut err = clap::Error::new(clap::error::ErrorKind::InvalidValue).with_cmd(cmd);
                if let Some(arg) = arg {
                    err.insert(
                        ContextKind::InvalidArg,
                        ContextValue::String(arg.to_string()),
                    );
                }
                err.insert(
                    ContextKind::InvalidValue,
                    ContextValue::String(value.to_string_lossy().to_string()),
                );
                err.insert(
                    ContextKind::ValidValue,
                    ContextValue::Strings(
                        self.possible_values()
                            .unwrap()
                            .map(|pv| pv.get_name().to_string())
                            .collect(),
                    ),
                );
                err
            }),
        }
    }

    fn possible_values(&self) -> Option<Box<dyn Iterator<Item=PossibleValue> + '_>> {
        Some(Box::new(
            vec![
                PossibleValue::new("cli").help(
                    "Display updated results in the CLI, blocking until submission has finished",
                ),
                PossibleValue::new("browser")
                    .help("Open a new browser window showing the submission and terminate program"),
                PossibleValue::new("none").help("Just terminate, ignoring submission result"),
            ].into_iter(),
        ))
    }
}
