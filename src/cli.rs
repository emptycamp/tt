use clap::{Parser, Subcommand};

use crate::config::ConfigAction;
use crate::duration::parse_duration;

#[derive(Parser)]
#[command(name = "tt", about = "Terminal timer tool")]
#[command(disable_help_subcommand = true)]
#[command(
    long_about = "Terminal timer tool. Pass a duration and an optional name \
(in either order) to start a timer, or run `tt` with no arguments to resume the last one."
)]
pub struct Cli {
    /// Run against isolated test data instead of production data.
    #[arg(long, global = true)]
    pub test: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Manage tt configuration.
    #[command(visible_alias = "conf")]
    // Bare `tt config` should error (asking for a subcommand), NOT dump help.
    // Help is shown only via `tt config --help`.
    #[command(arg_required_else_help = false, subcommand_required = true)]
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Duration and/or name for a new or resumed timer.
    #[command(external_subcommand)]
    Timer(Vec<String>),
}

pub enum CliAction {
    Resume,
    Clear,
    NewTimer(f64, String),
    DurationOnly(f64),
    NameOnly(String),
}

impl Cli {
    /// Parse the timer-related portion of the CLI into a high-level action.
    /// (The `config` subcommand is handled separately, before this is called.)
    pub fn action(&self) -> CliAction {
        match &self.command {
            // `tt clear` / `tt reset` (the word on its own) clears all data.
            Some(Command::Timer(args)) if is_clear_args(args) => CliAction::Clear,
            Some(Command::Timer(args)) => action_from_args(args),
            // No command, or the `config` subcommand (handled elsewhere).
            _ => CliAction::Resume,
        }
    }
}

/// True only for `clear`/`reset` passed as the sole argument. Anything more —
/// e.g. `tt clear something` — is left to the normal timer path (a timer named
/// "clear something"), so these words aren't globally reserved.
fn is_clear_args(args: &[String]) -> bool {
    matches!(args, [only] if only.eq_ignore_ascii_case("clear") || only.eq_ignore_ascii_case("reset"))
}

/// Parse timer CLI args into a `CliAction`.
///
/// Time can be the first or last arg. Everything else is the name.
fn action_from_args(args: &[String]) -> CliAction {
    if args.is_empty() {
        return CliAction::Resume;
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
    fn clear_word_alone_clears() {
        // `tt clear` (and the `reset` alias) — the word on its own — clears data.
        let cli = Cli::try_parse_from(["tt", "clear"]).unwrap();
        assert!(matches!(cli.action(), CliAction::Clear));
        let cli = Cli::try_parse_from(["tt", "reset"]).unwrap();
        assert!(matches!(cli.action(), CliAction::Clear));
        // Case-insensitive, matching the in-app command aliases.
        let cli = Cli::try_parse_from(["tt", "CLEAR"]).unwrap();
        assert!(matches!(cli.action(), CliAction::Clear));
    }

    #[test]
    fn clear_with_extra_args_is_a_timer_name() {
        // `tt clear something` is NOT a clear — it starts a timer named "clear something".
        let cli = Cli::try_parse_from(["tt", "clear", "something"]).unwrap();
        assert!(matches!(cli.action(), CliAction::NameOnly(ref n) if n == "clear something"));
    }

    #[test]
    fn clear_honors_global_test_flag() {
        let cli = Cli::try_parse_from(["tt", "--test", "clear"]).unwrap();
        assert!(cli.test);
        assert!(matches!(cli.action(), CliAction::Clear));
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
        if let CliAction::NewTimer(secs, name) = action_from_args(&args(&["deep", "work", "1.5h"]))
        {
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

    // --- clap wiring: timer args still flow through as before -----------------

    #[test]
    fn parses_timer_args() {
        let cli = Cli::parse_from(["tt", "5m", "meeting"]);
        assert!(!cli.test);
        assert!(matches!(cli.command, Some(Command::Timer(_))));
        assert!(
            matches!(cli.action(), CliAction::NewTimer(secs, ref name) if secs == 300.0 && name == "meeting")
        );
    }

    #[test]
    fn test_flag_is_global_and_args_still_parse() {
        let cli = Cli::parse_from(["tt", "--test", "5m", "standup"]);
        assert!(cli.test);
        assert!(
            matches!(cli.action(), CliAction::NewTimer(secs, ref name) if secs == 300.0 && name == "standup")
        );
    }

    #[test]
    fn no_command_resumes() {
        let cli = Cli::parse_from(["tt"]);
        assert!(cli.command.is_none());
        assert!(matches!(cli.action(), CliAction::Resume));
    }

    // --- config subcommand ----------------------------------------------------

    #[test]
    fn config_list_parses() {
        let cli = Cli::try_parse_from(["tt", "config", "list"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Config {
                action: ConfigAction::List
            })
        ));
    }

    #[test]
    fn conf_alias_and_ls_alias_parse() {
        let cli = Cli::try_parse_from(["tt", "conf", "ls"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Config {
                action: ConfigAction::List
            })
        ));
    }

    #[test]
    fn config_set_parses() {
        let cli =
            Cli::try_parse_from(["tt", "config", "set", "integrate_with_task=false"]).unwrap();
        match cli.command {
            Some(Command::Config {
                action: ConfigAction::Set { args },
            }) => assert_eq!(args, vec!["integrate_with_task=false".to_string()]),
            _ => panic!("expected config set"),
        }
    }

    #[test]
    fn config_without_subcommand_errors_not_help() {
        // Bare `tt config` is an error (requires a subcommand), not a help dump.
        // `.err().unwrap()` avoids needing `Cli: Debug` for `.unwrap_err()`.
        let err = Cli::try_parse_from(["tt", "config"]).err().unwrap();
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::MissingSubcommand,
            "got: {:?}",
            err.kind()
        );
    }

    #[test]
    fn config_help_flag_requests_help() {
        let err = Cli::try_parse_from(["tt", "config", "--help"])
            .err()
            .unwrap();
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
    }
}
