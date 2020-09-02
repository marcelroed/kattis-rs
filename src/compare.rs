use colored::{ColoredString, Colorize};

use itertools::Itertools;

use std::fmt::Formatter;

pub struct CompareResult<'a> {
    failed: Option<Vec<(&'a str, Option<&'a str>)>>,
}

impl<'a> CompareResult<'a> {
    pub fn new(x: Vec<(&'a str, Option<&'a str>)>) -> Self {
        let failed = if (&x).iter().all(|x| x.1.is_none()) {
            None
        } else {
            Some(x)
        };

        CompareResult { failed }
    }
}

impl<'a> std::fmt::Display for CompareResult<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let output = match &self.failed {
            Some(failures) => {
                // Want to group into error blocks
                let mut correction: Vec<ColoredString> = Vec::new();
                let it = failures.iter();

                let mut error_block_buf = (Vec::new(), Vec::new());
                for inner in it {
                    match inner {
                        (wrong_line, Some(correction)) => {
                            error_block_buf.0.push(wrong_line.red());
                            error_block_buf.1.push(correction.green());
                        }
                        (correct_line, _) => {
                            correction.append(&mut error_block_buf.0);
                            correction.append(&mut error_block_buf.1);
                            error_block_buf.0.clear();
                            error_block_buf.1.clear();
                            correction.push(correct_line.white());
                        }
                    }
                }
                if !error_block_buf.0.is_empty() {
                    correction.append(&mut error_block_buf.0);
                    correction.append(&mut error_block_buf.1);
                }

                correction.into_iter().map(|cs| cs.to_string()).join("\n")
            }
            None => "Success".green().to_string(),
        };
        write!(f, "{}", output)
    }
}

fn compare_lines(text: &'a str, key: &'a str) -> (&'a str, Option<&'a str>) {
    const TO_STRIP: &[char] = &['\n', ' ', '\t', '\r'];
    let pat = |c| TO_STRIP.contains(&c);
    let orig = text.trim_matches(pat).trim_matches(pat);
    let other = key.trim_matches(pat).trim_matches(pat);

    if orig.eq(other) {
        (orig, None)
    } else {
        (orig, Some(other))
    }
}

pub fn compare(output: &'a str, key: &'a str) -> CompareResult<'a> {
    let output: Vec<&str> = output.split('\n').collect();
    let key: Vec<&str> = key.split('\n').collect();

    let comparisons: Vec<_> = output
        .into_iter()
        .zip(key.into_iter())
        .map(|(o, k)| compare_lines(o, k))
        .collect();

    CompareResult::new(comparisons)
}

#[cfg(test)]
mod test {
    use crate::compare::compare;

    #[test]
    fn test_compare() {
        let output = "This is my long story about going to taco bell.\nOne day I felt like I really wanted some good stuff.\nI walked to taco bell to get some 0.55512312412345 tacos.".to_string();
        let key = "This is my long story about going to cracko bell.\nOne day I felt like I really wanted some good stuff.\nI walked to cracko bell to get some 0.5551231241234 crack.".to_string();
        let comparisons = compare(&output, &key);
        println!("{}", comparisons);
    }

    #[test]
    fn test_num_diff() {}
}
