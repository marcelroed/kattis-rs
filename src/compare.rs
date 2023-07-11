use colored::{ColoredString, Colorize};

use itertools::{EitherOrBoth, Itertools};

use std::fmt::Formatter;

use regex::{Captures, Regex};

#[derive(Debug, Clone)]
pub enum LineStatus {
    Wrong(String, String), // Wrong, correction
    Correct(String),       // Correct
    Missing(String),       // Missing
    Overpresent(String),   // Line past output
}

pub struct CompareResult {
    pub failed: Option<Vec<LineStatus>>,
}

impl CompareResult {
    pub fn new(x: Vec<LineStatus>) -> Self {
        let failed = if x.iter().all(|x| matches!(x, LineStatus::Correct(_))) {
            None
        } else {
            Some(x)
        };

        Self { failed }
    }
}

impl std::fmt::Display for CompareResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let output = match &self.failed {
            Some(failures) => {
                // Group into error blocks
                let mut correction: Vec<ColoredString> = Vec::new();
                let it = failures.iter();

                // (error buffer, correction buffer)
                let mut error_block_buf = (Vec::new(), Vec::new());
                for inner in it {
                    match inner {
                        LineStatus::Wrong(wrong_line, correction) => {
                            if wrong_line.is_empty() {
                                error_block_buf.0.push(" ".on_red());
                            } else {
                                error_block_buf.0.push(wrong_line.red());
                            }
                            error_block_buf.1.push(correction.green());
                        }
                        LineStatus::Correct(correct_line) => {
                            correction.append(&mut error_block_buf.0);
                            correction.append(&mut error_block_buf.1);
                            error_block_buf.0.clear();
                            error_block_buf.1.clear();
                            correction.push(correct_line.white());
                        }
                        LineStatus::Missing(missing_line) => {
                            error_block_buf.0.push(" ".on_red());
                            error_block_buf.1.push(missing_line.green());
                        }
                        LineStatus::Overpresent(overpresent_line) => {
                            error_block_buf.0.push({
                                if overpresent_line.is_empty() {
                                    " ".on_red()
                                } else {
                                    overpresent_line.red()
                                }
                            });
                        }
                    }
                }
                if !error_block_buf.0.is_empty() {
                    correction.append(&mut error_block_buf.0);
                    correction.append(&mut error_block_buf.1);
                }

                correction.into_iter().map(|cs| cs.to_string()).join("\n")
            }
            None => "Success".green().bold().to_string(),
        };
        write!(f, "{}", output)
    }
}

lazy_static::lazy_static! {
    static ref RE: Regex = Regex::new(r"([-+]?[0-9]+)(\.([0-9]+))?").unwrap();
}
fn line_eq(text: &str, key: &str) -> bool {
    // Round real numbers properly
    let mut key_iter = RE.captures_iter(key);
    let rounded = RE.replace_all(text, |captures: &Captures| -> String {
        let mut in_text: String = captures.get(0).unwrap().as_str().to_string();
        if let Some(in_key_captures) = &key_iter.next() {
            if let Some(post) = in_key_captures.get(3) {
                if let Ok(as_float) = in_text.parse::<f64>() {
                    in_text = format!("{1:.0$}", post.as_str().len(), as_float);
                }
            }
        }
        in_text
    });
    rounded.eq(key)
}

fn compare_lines(text: &str, key: &str) -> LineStatus {
    const TO_STRIP: &[char] = &['\n', ' ', '\t', '\r'];
    let pat = |c| TO_STRIP.contains(&c);
    let orig = text.trim_matches(pat).trim_matches(pat);
    let other = key.trim_matches(pat).trim_matches(pat);

    if line_eq(orig, other) {
        LineStatus::Correct(orig.to_string())
    } else {
        LineStatus::Wrong(orig.to_string(), other.to_string())
    }
}

pub fn compare(output: &str, key: &str) -> CompareResult {
    
    

    let comparisons: Vec<_> = output.split('\n')
        .zip_longest(key.split('\n'))
        .map(|eob| match eob {
            EitherOrBoth::Both(l, r) => (Some(l), Some(r)),
            EitherOrBoth::Left(l) => (Some(l), None),
            EitherOrBoth::Right(r) => (None, Some(r)),
        })
        .filter_map(|out_key| match out_key {
            (Some(o), Some(k)) => Some(compare_lines(o, k)),
            (None, Some(k)) if !k.is_empty() => Some(LineStatus::Missing(k.to_string())),
            (Some(o), None) if !o.is_empty() => Some(LineStatus::Overpresent(o.to_string())),
            _ => None,
        })
        .collect();

    CompareResult::new(comparisons)
}

#[cfg(test)]
mod test {
    use crate::compare::compare;

    #[test]
    fn test_compare() {
        let output = "This is my long story about going to taco bell.\nOne day I felt like I really wanted some good stuff.\nI walked to taco bell to get 0.55512312412345 tacos.".to_string();
        let key = "This is my long story about going to cracko bell.\nOne day I felt like I really wanted some good stuff.\nI walked to cracko bell to get 0.5551231241234 crack.".to_string();
        let comparisons = compare(&output, &key);
        println!("{}", comparisons);
    }

    #[test]
    fn test_num_diff() {}
}
