use crate::duration::parse_duration;

#[derive(Debug, PartialEq)]
pub enum Command {
    Pause,
    Reset,
    Update(f64),
    Revert,
    Remove,
    NewTimer(f64, String),
    NamePrompt(f64),
    Switch,
    TimerPrompt(String),
    Quit,
    Unknown(String),
}

impl Command {
    /// Parse user input from the command bar into a `Command`.
    pub fn parse(input: &str) -> Self {
        let input = input.trim();
        if input.is_empty() {
            return Self::Unknown(String::new());
        }

        if Self::is_alias(input, &["q", "quit", "exit"]) {
            return Self::Quit;
        }
        if Self::is_alias(input, &["p", "pause", "paus"]) {
            return Self::Pause;
        }
        if Self::is_alias(input, &["r", "rst", "reset"]) {
            return Self::Reset;
        }
        if Self::is_alias(input, &["revert", "rollback"]) {
            return Self::Revert;
        }
        if Self::is_alias(input, &["stop", "remove", "rm"]) {
            return Self::Remove;
        }
        if Self::is_alias(input, &["tt"]) {
            return Self::Switch;
        }

        if let Some((head, tail)) = input.split_once(char::is_whitespace) {
            let rest = tail.trim();
            if head.eq_ignore_ascii_case("update") {
                return match parse_duration(rest) {
                    Ok(secs) => Self::Update(secs),
                    Err(_) => Self::Unknown(input.to_string()),
                };
            }
            if head.eq_ignore_ascii_case("tt") {
                return Self::parse_tt_args(rest);
            }
        }

        Self::Unknown(input.to_string())
    }

    fn is_alias(input: &str, aliases: &[&str]) -> bool {
        aliases
            .iter()
            .any(|alias| input.eq_ignore_ascii_case(alias))
    }

    /// Parse arguments after "tt": time can be first or last token, rest is name.
    fn parse_tt_args(args: &str) -> Self {
        if args.is_empty() {
            return Self::Switch;
        }

        let parts: Vec<&str> = args.split_whitespace().collect();

        // Try first token as duration
        if let Ok(secs) = parse_duration(parts[0]) {
            return if parts.len() > 1 {
                Self::NewTimer(secs, parts[1..].join(" "))
            } else {
                Self::NamePrompt(secs)
            };
        }

        // Try last token as duration
        if parts.len() >= 2 {
            if let Ok(secs) = parse_duration(parts[parts.len() - 1]) {
                let name = parts[..parts.len() - 1].join(" ");
                return Self::NewTimer(secs, name);
            }
        }

        // No valid duration — treat entire input as a name, prompt for time
        Self::TimerPrompt(args.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_aliases() {
        assert_eq!(Command::parse("p"), Command::Pause);
        assert_eq!(Command::parse("pause"), Command::Pause);
        assert_eq!(Command::parse("paus"), Command::Pause);
        assert_eq!(Command::parse("PAUSE"), Command::Pause);
    }

    #[test]
    fn reset_aliases() {
        assert_eq!(Command::parse("r"), Command::Reset);
        assert_eq!(Command::parse("rst"), Command::Reset);
        assert_eq!(Command::parse("reset"), Command::Reset);
        assert_eq!(Command::parse("RESET"), Command::Reset);
    }

    #[test]
    fn revert_aliases() {
        assert_eq!(Command::parse("revert"), Command::Revert);
        assert_eq!(Command::parse("rollback"), Command::Revert);
    }

    #[test]
    fn remove_aliases() {
        assert_eq!(Command::parse("stop"), Command::Remove);
        assert_eq!(Command::parse("remove"), Command::Remove);
        assert_eq!(Command::parse("rm"), Command::Remove);
        assert_eq!(Command::parse("RM"), Command::Remove);
    }

    #[test]
    fn switch() {
        assert_eq!(Command::parse("tt"), Command::Switch);
    }

    #[test]
    fn quit_aliases() {
        assert_eq!(Command::parse("q"), Command::Quit);
        assert_eq!(Command::parse("quit"), Command::Quit);
        assert_eq!(Command::parse("exit"), Command::Quit);
        assert_eq!(Command::parse("EXIT"), Command::Quit);
    }

    #[test]
    fn update_time() {
        if let Command::Update(secs) = Command::parse("update 5m") {
            assert_eq!(secs, 300.0);
        } else {
            panic!("expected Update");
        }
    }

    #[test]
    fn update_invalid_time() {
        assert!(matches!(Command::parse("update xyz"), Command::Unknown(_)));
    }

    #[test]
    fn new_timer_time_first() {
        if let Command::NewTimer(secs, name) = Command::parse("tt 10m meeting") {
            assert_eq!(secs, 600.0);
            assert_eq!(name, "meeting");
        } else {
            panic!("expected NewTimer");
        }
    }

    #[test]
    fn new_timer_time_first_long_name() {
        if let Command::NewTimer(secs, name) = Command::parse("tt 5m weekly standup meeting") {
            assert_eq!(secs, 300.0);
            assert_eq!(name, "weekly standup meeting");
        } else {
            panic!("expected NewTimer");
        }
    }

    #[test]
    fn new_timer_time_last() {
        if let Command::NewTimer(secs, name) = Command::parse("tt some long name 4s") {
            assert_eq!(secs, 4.0);
            assert_eq!(name, "some long name");
        } else {
            panic!("expected NewTimer with time at end");
        }
    }

    #[test]
    fn new_timer_time_last_hours() {
        if let Command::NewTimer(secs, name) = Command::parse("tt deep work session 1.5h") {
            assert!((secs - 5400.0).abs() < 0.01);
            assert_eq!(name, "deep work session");
        } else {
            panic!("expected NewTimer");
        }
    }

    #[test]
    fn name_prompt() {
        if let Command::NamePrompt(secs) = Command::parse("tt 10m") {
            assert_eq!(secs, 600.0);
        } else {
            panic!("expected NamePrompt");
        }
    }

    #[test]
    fn timer_prompt_single_word() {
        if let Command::TimerPrompt(name) = Command::parse("tt meeting") {
            assert_eq!(name, "meeting");
        } else {
            panic!("expected TimerPrompt");
        }
    }

    #[test]
    fn timer_prompt_multi_word() {
        if let Command::TimerPrompt(name) = Command::parse("tt my long task name") {
            assert_eq!(name, "my long task name");
        } else {
            panic!("expected TimerPrompt");
        }
    }

    #[test]
    fn empty_input() {
        assert!(matches!(Command::parse(""), Command::Unknown(_)));
    }

    #[test]
    fn whitespace_only() {
        assert!(matches!(Command::parse("   "), Command::Unknown(_)));
    }

    #[test]
    fn unknown_command() {
        assert!(matches!(Command::parse("foobar"), Command::Unknown(_)));
    }

    #[test]
    fn tt_empty_is_switch() {
        assert_eq!(Command::parse("tt "), Command::Switch);
    }
}
