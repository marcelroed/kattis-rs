use clap::builder::{PossibleValue, TypedValueParser};
use clap::{Arg, Command};
use colored::{ColoredString, Colorize};
use enum_iterator::Sequence;
use lazy_static::lazy_static;
use log::info;
use regex::Regex;
use reqwest::Client;
use serde::Deserialize;
use serde_repr::Deserialize_repr;
use std::ffi::OsStr;
use std::fmt::{Display, Formatter};
use std::io::IsTerminal;
use std::sync::OnceLock;

lazy_static! {
    static ref X_MARK: ColoredString = "✘".red();
    static ref CHECK_MARK: ColoredString = "✔".green();
}

pub fn display_link(url: &str) -> String {
    display_link_with_name(url, url)
}

/// Add link if outputting to a terminal
fn display_link_with_name(url: &str, name: &str) -> String {
    if std::io::stdout().is_terminal() {
        format!("\u{1b}]8;;{url}\u{1b}\\{name}\u{1b}]8;;\u{1b}\\")
    } else {
        name.to_string()
    }
}

/// Adds link if `maybe_url` is `Some` and outputting to a terminal
fn maybe_add_link(s: &str, maybe_url: Option<&str>) -> String {
    maybe_url.map_or_else(|| s.to_string(), |url| display_link_with_name(url, s))
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
            let submission_link = self
                .submission_id()
                .map(|id| format!("https://open.kattis.com/submissions/{id}"));
            accepted_text = maybe_add_link(&accepted_text, submission_link.as_deref())
                .green()
                .bold();

            write!(f, "{accepted_text} ")?;
            if let Some(problem_name) = self.problem_name() {
                write!(
                    f,
                    "{}",
                    maybe_add_link(
                        problem_name,
                        self.problem_slug()
                            .map(|slug| format!("https://open.kattis.com{slug}"))
                            .as_deref()
                    )
                    .bold()
                )?;
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
                maybe_add_link(
                    &self.status.to_string(),
                    self.submission_id()
                        .map(|id| format!("https://open.kattis.com/submissions/{id}"))
                        .as_deref()
                )
                .bold()
                .red()
            )?;
            if let Some(time) = self.cpu_time() {
                write!(
                    f,
                    "{}",
                    format!(" after {}{}", time.bold(), "s".bold()).red()
                )?;
            }
            writeln!(f)?;
            Ok(())
        }
    }
}

fn reset_line() {
    eprint!("\x1B[2K\r");
}

pub async fn view_submission_in_terminal(
    client: Client,
    submission_id: &str,
) -> anyhow::Result<()> {
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

            if written_first {
                reset_line();
            } else {
                written_first = true;
            } // Clear and move to start of line

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
    }
    .await
}

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
pub enum SubmissionViewerType {
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
    type Value = SubmissionViewerType;

    fn parse_ref(
        &self,
        cmd: &Command,
        arg: Option<&Arg>,
        value: &OsStr,
    ) -> std::result::Result<Self::Value, clap::Error> {
        use clap::error::{ContextKind, ContextValue};
        use SubmissionViewerType::{Browser, Cli, None};
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

    fn possible_values(&self) -> Option<Box<dyn Iterator<Item = PossibleValue> + '_>> {
        Some(Box::new(
            vec![
                PossibleValue::new("cli").help(
                    "Display updated results in the CLI, blocking until submission has finished",
                ),
                PossibleValue::new("browser")
                    .help("Open a new browser window showing the submission and terminate program"),
                PossibleValue::new("none").help("Just terminate, ignoring submission result"),
            ]
            .into_iter(),
        ))
    }
}
