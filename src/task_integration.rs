//! Bridge to the external `ttask` CLI tool.
//!
//! Integration is shell-out based: tt invokes the `ttask` binary (when present on
//! `PATH`) to read the active task list and to write back completions/deletions.
//! `ttask` stays a fully separate program (it stores its data in LMDB, which makes
//! concurrent access from multiple `ttask`/`tt` processes safe). All interaction
//! goes through [`TaskSource`] so the app logic is unit-testable without spawning
//! real processes.

use std::process::Command;

/// A task surfaced by the `ttask` tool, reduced to what a timer needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRow {
    pub id: u32,
    pub est_secs: i64,
    pub text: String,
}

pub trait TaskSource {
    /// The active tasks. `None` means the `ttask` tool could not be reached (no
    /// binary, or it errored) — distinct from `Some(vec![])` (it ran, no tasks).
    /// Callers must NOT reconcile/remove timers on `None`.
    fn list_active(&self) -> Option<Vec<TaskRow>>;
    /// Mark the task complete in `ttask`.
    fn complete(&mut self, id: u32) -> Result<(), String>;
    /// Soft-delete the task in `ttask`.
    fn delete(&mut self, id: u32) -> Result<(), String>;
}

/// Production [`TaskSource`]: shells out to the `ttask` binary on `PATH`.
pub struct CliTaskSource {
    test_mode: bool,
}

impl CliTaskSource {
    pub fn new(test_mode: bool) -> Self {
        Self { test_mode }
    }

    /// A `ttask` command with shared flags applied. Output is always captured (it
    /// never inherits the terminal) so it can't disturb the ratatui screen.
    fn base_command(&self) -> Command {
        let mut cmd = Command::new("ttask");
        if self.test_mode {
            cmd.arg("--test");
        }
        cmd
    }
}

impl TaskSource for CliTaskSource {
    fn list_active(&self) -> Option<Vec<TaskRow>> {
        let mut cmd = self.base_command();
        cmd.args(["--format", "md", "list"]);
        match cmd.output() {
            Ok(o) if o.status.success() => Some(parse_md_list(&String::from_utf8_lossy(&o.stdout))),
            // No binary / non-zero exit ⇒ can't reconcile this cycle.
            _ => None,
        }
    }

    fn complete(&mut self, id: u32) -> Result<(), String> {
        let mut cmd = self.base_command();
        cmd.args(["complete", &id.to_string()]);
        run_write(&mut cmd)
    }

    fn delete(&mut self, id: u32) -> Result<(), String> {
        let mut cmd = self.base_command();
        cmd.args(["delete", &id.to_string()]);
        run_write(&mut cmd)
    }
}

fn run_write(cmd: &mut Command) -> Result<(), String> {
    match cmd.output() {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_string()),
        Err(e) => Err(e.to_string()),
    }
}

/// Parse `ttask --format md list` output into rows. The format is the stable,
/// machine-targeted markdown table:
/// `| ID | Cat | Status | Ord | Description | Est |`. Header/separator rows fail
/// the numeric-id parse and are skipped; only active rows are returned.
pub fn parse_md_list(output: &str) -> Vec<TaskRow> {
    let mut rows = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }
        let mut cells = split_md_row(line);
        if cells.first().is_some_and(String::is_empty) {
            cells.remove(0);
        }
        if cells.last().is_some_and(String::is_empty) {
            cells.pop();
        }
        // Columns: ID, Cat, Status, Ord, Description, Est.
        if cells.len() < 6 {
            continue;
        }
        let Ok(id) = cells[0].parse::<u32>() else {
            continue; // header / separator / malformed row
        };
        if !cells[2].eq_ignore_ascii_case("active") {
            continue;
        }
        let est_secs = parse_est(&cells[5]).unwrap_or(0);
        rows.push(TaskRow {
            id,
            est_secs,
            text: cells[4].clone(),
        });
    }
    rows
}

/// Split a markdown table row on unescaped `|`, unescaping `\|` back to `|` in
/// the cells (the `ttask` tool escapes pipes inside descriptions).
fn split_md_row(row: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut chars = row.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                cur.push('|');
                chars.next();
            }
            '|' => {
                cells.push(cur.trim().to_string());
                cur.clear();
            }
            other => cur.push(other),
        }
    }
    cells.push(cur.trim().to_string());
    cells
}

/// Parse an estimate string as emitted by the `ttask` tool's `format_est`
/// (`30m`/`1h`/`2d`/`90s`) into seconds. (tt's own `parse_duration` rejects the
/// `d` suffix, so this dedicated parser is required.)
pub fn parse_est(s: &str) -> Option<i64> {
    let s = s.trim().to_ascii_lowercase();
    if s.is_empty() {
        return None;
    }
    let (num, mult): (&str, i64) = if let Some(n) = s.strip_suffix('d') {
        (n, 86_400)
    } else if let Some(n) = s.strip_suffix('h') {
        (n, 3_600)
    } else if let Some(n) = s.strip_suffix('m') {
        (n, 60)
    } else if let Some(n) = s.strip_suffix('s') {
        (n, 1)
    } else {
        // `format_est` always emits a suffix; tolerate a bare number as seconds.
        (s.as_str(), 1)
    };
    num.trim().parse::<i64>().ok().map(|v| v * mult)
}

/// Shared mutable state behind a [`FakeTaskSource`], so a test can hold a clone
/// of the source and inspect what was written after handing one to the `App`.
#[cfg(test)]
#[derive(Default)]
pub struct FakeState {
    pub rows: Vec<TaskRow>,
    pub available: bool,
    pub completed: Vec<u32>,
    pub deleted: Vec<u32>,
    pub fail_writes: bool,
}

/// In-memory [`TaskSource`] for tests — no subprocess. Cheaply cloneable; clones
/// share the same underlying state.
#[cfg(test)]
#[derive(Clone)]
pub struct FakeTaskSource {
    pub state: std::sync::Arc<std::sync::Mutex<FakeState>>,
}

#[cfg(test)]
impl FakeTaskSource {
    pub fn with_rows(rows: Vec<TaskRow>) -> Self {
        Self {
            state: std::sync::Arc::new(std::sync::Mutex::new(FakeState {
                rows,
                available: true,
                ..Default::default()
            })),
        }
    }
    pub fn set_available(&self, available: bool) {
        self.state.lock().unwrap().available = available;
    }
    pub fn set_fail_writes(&self, fail: bool) {
        self.state.lock().unwrap().fail_writes = fail;
    }
    pub fn set_rows(&self, rows: Vec<TaskRow>) {
        self.state.lock().unwrap().rows = rows;
    }
    pub fn completed(&self) -> Vec<u32> {
        self.state.lock().unwrap().completed.clone()
    }
    pub fn deleted(&self) -> Vec<u32> {
        self.state.lock().unwrap().deleted.clone()
    }
}

#[cfg(test)]
impl TaskSource for FakeTaskSource {
    fn list_active(&self) -> Option<Vec<TaskRow>> {
        let s = self.state.lock().unwrap();
        if !s.available {
            return None; // simulate `ttask` not installed / unreachable
        }
        Some(s.rows.clone())
    }
    fn complete(&mut self, id: u32) -> Result<(), String> {
        let mut s = self.state.lock().unwrap();
        if s.fail_writes {
            return Err("write failed".to_string());
        }
        s.completed.push(id);
        s.rows.retain(|r| r.id != id);
        Ok(())
    }
    fn delete(&mut self, id: u32) -> Result<(), String> {
        let mut s = self.state.lock().unwrap();
        if s.fail_writes {
            return Err("write failed".to_string());
        }
        s.deleted.push(id);
        s.rows.retain(|r| r.id != id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_est_units() {
        assert_eq!(parse_est("30m"), Some(1800));
        assert_eq!(parse_est("1h"), Some(3600));
        assert_eq!(parse_est("2d"), Some(172_800));
        assert_eq!(parse_est("90s"), Some(90));
        assert_eq!(parse_est("0m"), Some(0));
        assert_eq!(parse_est("  45m "), Some(2700));
        assert_eq!(parse_est(""), None);
        assert_eq!(parse_est("abc"), None);
    }

    #[test]
    fn parse_md_list_basic_table() {
        let out = "# Tasks (2 tasks)\n\n\
            | ID | Cat | Status | Ord | Description | Est |\n\
            |---:|:---:|:-------|---:|:------------|----:|\n\
            | 1 | A | active | 1 | Buy milk | 30m |\n\
            | 2 | B | active | 2 | Read book | 1h |\n";
        let rows = parse_md_list(out);
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0],
            TaskRow {
                id: 1,
                est_secs: 1800,
                text: "Buy milk".into()
            }
        );
        assert_eq!(
            rows[1],
            TaskRow {
                id: 2,
                est_secs: 3600,
                text: "Read book".into()
            }
        );
    }

    #[test]
    fn parse_md_list_unescapes_pipe_and_skips_non_active() {
        let out = "| ID | Cat | Status | Ord | Description | Est |\n\
            |---:|:---:|:-------|---:|:------------|----:|\n\
            | 7 | C | active | 1 | a \\| b | 5m |\n\
            | 8 | B | completed | 2 | done | 5m |\n";
        let rows = parse_md_list(out);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, "a | b");
        assert_eq!(rows[0].est_secs, 300);
    }

    #[test]
    fn parse_md_list_empty_message() {
        assert!(parse_md_list("# Tasks\n\n_No tasks._\n").is_empty());
    }
}
