use clap::Parser;

use crate::duration::parse_duration;

#[derive(Parser)]
#[command(name = "tt", about = "Terminal timer tool")]
pub struct Cli {
    /// Run against isolated test data instead of production data.
    #[arg(long)]
    pub test: bool,

    /// All arguments: duration and name in any order.
    /// Example: `tt 5m meeting` or `tt some long name 4s`
    pub args: Vec<String>,
}

pub enum CliAction {
    Resume,
    Clear,
    NewTimer(f64, String),
    DurationOnly(f64),
    NameOnly(String),
}

impl Cli {
    /// Parse all raw CLI args into a high-level action.
    pub fn action(&self) -> CliAction {
        action_from_args(&self.args)
    }
}

/// Parse CLI args into a `CliAction`.
///
/// Time can be the first or last arg. Everything else is the name.
fn action_from_args(args: &[String]) -> CliAction {
    if args.is_empty() {
        return CliAction::Resume;
    }

    if args.len() == 1 && args[0].eq_ignore_ascii_case("clear") {
        return CliAction::Clear;
    }

    // Try first arg as duration
    if let Ok(secs) = parse_duration(&args[0]) {
        let name_parts = &args[1..];
        return if name_parts.is_empty() {
            CliAction::DurationOnly(secs)
        } else {
            CliAction::NewTimer(secs, name_parts.join(" "))
        };
    }

    // Try last arg as duration
    if let Some((last, rest)) = args.split_last() {
        if !rest.is_empty() {
            if let Ok(secs) = parse_duration(last) {
                return CliAction::NewTimer(secs, rest.join(" "));
            }
        }
    }

    // No valid duration — treat entire input as a name
    CliAction::NameOnly(args.join(" "))
}

pub fn confirm_clear(is_test_mode: bool) -> bool {
    use std::io::{self, Write};

    if is_test_mode {
        print!("Clear TEST timer data? [y/N] ");
    } else {
        print!("Clear all timer data? [y/N] ");
    }
    if io::stdout().flush().is_err() {
        return false;
    }

    let mut answer = String::new();
    if io::stdin().read_line(&mut answer).is_err() {
        return false;
    }

    answer.trim().eq_ignore_ascii_case("y")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_args_resumes() {
        assert!(matches!(action_from_args(&[]), CliAction::Resume));
    }

    #[test]
    fn clear_arg() {
        assert!(matches!(
            action_from_args(&args(&["clear"])),
            CliAction::Clear
        ));
        assert!(matches!(
            action_from_args(&args(&["CLEAR"])),
            CliAction::Clear
        ));
    }

    #[test]
    fn duration_first_with_name() {
        if let CliAction::NewTimer(secs, name) = action_from_args(&args(&["5m", "standup"])) {
            assert_eq!(secs, 300.0);
            assert_eq!(name, "standup");
        } else {
            panic!("expected NewTimer");
        }
    }

    #[test]
    fn duration_first_long_name() {
        if let CliAction::NewTimer(secs, name) =
            action_from_args(&args(&["10m", "weekly", "standup", "meeting"]))
        {
            assert_eq!(secs, 600.0);
            assert_eq!(name, "weekly standup meeting");
        } else {
            panic!("expected NewTimer");
        }
    }

    #[test]
    fn duration_last_with_name() {
        if let CliAction::NewTimer(secs, name) =
            action_from_args(&args(&["some", "long", "name", "4s"]))
        {
            assert_eq!(secs, 4.0);
            assert_eq!(name, "some long name");
        } else {
            panic!("expected NewTimer with time at end");
        }
    }

    #[test]
    fn duration_last_hours() {
        if let CliAction::NewTimer(secs, name) = action_from_args(&args(&["deep", "work", "1.5h"])) {
            assert!((secs - 5400.0).abs() < 0.01);
            assert_eq!(name, "deep work");
        } else {
            panic!("expected NewTimer");
        }
    }

    #[test]
    fn duration_only_prompts_for_name() {
        if let CliAction::DurationOnly(secs) = action_from_args(&args(&["5m"])) {
            assert_eq!(secs, 300.0);
        } else {
            panic!("expected DurationOnly");
        }
    }

    #[test]
    fn plain_number_duration() {
        if let CliAction::DurationOnly(secs) = action_from_args(&args(&["5"])) {
            assert_eq!(secs, 300.0);
        } else {
            panic!("expected DurationOnly");
        }
    }

    #[test]
    fn name_only_prompts_for_time() {
        if let CliAction::NameOnly(name) = action_from_args(&args(&["meeting"])) {
            assert_eq!(name, "meeting");
        } else {
            panic!("expected NameOnly");
        }
    }

    #[test]
    fn multi_word_name_no_duration() {
        if let CliAction::NameOnly(name) = action_from_args(&args(&["my", "cool", "task"])) {
            assert_eq!(name, "my cool task");
        } else {
            panic!("expected NameOnly");
        }
    }

    #[test]
    fn ambiguous_prefers_first_as_duration() {
        if let CliAction::NewTimer(secs, name) = action_from_args(&args(&["5", "2m"])) {
            assert_eq!(secs, 300.0);
            assert_eq!(name, "2m");
        } else {
            panic!("expected NewTimer with first arg as duration");
        }
    }

    #[test]
    fn parse_test_flag_with_args() {
        let cli = Cli::parse_from(["tt", "--test", "clear"]);
        assert!(cli.test);
        assert_eq!(cli.args, vec!["clear"]);
        assert!(matches!(cli.action(), CliAction::Clear));
    }

    #[test]
    fn parse_without_test_flag() {
        let cli = Cli::parse_from(["tt", "5m", "meeting"]);
        assert!(!cli.test);
        assert_eq!(cli.args, vec!["5m", "meeting"]);
        assert!(matches!(cli.action(), CliAction::NewTimer(_, _)));
    }
}
