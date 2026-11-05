//! `etoon hook claude`: a Claude Code `PreToolUse` hook that appends `| etoon` to Bash commands.
//!
//! Claude Code applies only one `updatedInput` per tool call, and that object replaces the
//! whole `tool_input`. So this one hook merges every rewrite: first the `--pre` rewriter,
//! then `rtk hook claude` when rtk is on `PATH`, then the etoon suffix.
//!
//! The hook fails open. A parse error, a failing rewriter or a panic prints nothing (or only
//! the rewrites that already succeeded) and exits 0, so Claude Code runs the command.

use crate::skip::skip_etoon;
use sonic_rs::{JsonValueTrait, Value};
use std::ffi::OsString;
use std::io::{self, ErrorKind, Read, Write};
use std::panic::{self, AssertUnwindSafe};
use std::process::{Child, Command, ExitCode, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

pub const USAGE: &str = "usage: etoon hook claude [--pre CMD]";

/// Restores the exit status of the pipeline element before etoon (bash `PIPESTATUS`).
const PIPE_SUFFIX: &str = "; __etoon_rc=${PIPESTATUS[${#PIPESTATUS[@]}-2]}; (exit $__etoon_rc)";
/// How long the `--pre` rewriter may run before the hook kills it and keeps the command.
const PRE_TIMEOUT: Duration = Duration::from_secs(2);
/// How long `rtk hook claude` may run (it takes about 25 ms) before the hook kills it.
const RTK_TIMEOUT: Duration = Duration::from_secs(5);

/// What the hook prints and the exit code it returns.
#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    stdout: String,
    code: u8,
}

impl Outcome {
    fn silent() -> Self {
        Self {
            stdout: String::new(),
            code: 0,
        }
    }

    fn decision(decision: &RawObject) -> Self {
        Self {
            stdout: decision.to_json() + "\n",
            code: 0,
        }
    }

    /// A decision that only replaces the tool input.
    fn updated_input(tool_input: &RawObject) -> Self {
        Self {
            stdout: format!(
                r#"{{"hookSpecificOutput":{{"hookEventName":"PreToolUse","updatedInput":{}}}}}"#,
                tool_input.to_json()
            ) + "\n",
            code: 0,
        }
    }
}

/// A JSON object as ordered `(key, raw JSON value)` pairs.
///
/// Untouched values keep their source text, and an edit keeps the key order. (An edit of a
/// sonic-rs `Object` turns it into a hash map, so its key order can change between runs.)
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct RawObject(Vec<(String, String)>);

impl RawObject {
    /// Parses a JSON object. Returns `None` for invalid JSON, any other JSON type, or a
    /// duplicate key: an edit of the first copy would lose to a reader that keeps the last.
    fn parse(json: &str) -> Option<Self> {
        // The lazy iterator does not check the values, so the whole text is checked first.
        if !sonic_rs::from_str::<Value>(json).ok()?.is_object() {
            return None;
        }
        let pairs = sonic_rs::to_object_iter(json)
            .map(|pair| {
                let (key, value) = pair.ok()?;
                Some((key.into_owned(), value.as_raw_str().to_owned()))
            })
            .collect::<Option<Vec<_>>>()?;
        let duplicate = pairs
            .iter()
            .enumerate()
            .any(|(i, (key, _))| pairs[..i].iter().any(|(k, _)| k == key));
        (!duplicate).then_some(Self(pairs))
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// The value under `key` when it is an object.
    fn object(&self, key: &str) -> Option<Self> {
        Self::parse(self.get(key)?)
    }

    /// The value under `key` when it is a string.
    fn string(&self, key: &str) -> Option<String> {
        sonic_rs::from_str(self.get(key)?).ok()
    }

    /// Replaces the value under `key` in place, or appends the pair.
    fn set(&mut self, key: &str, raw: String) {
        match self.0.iter_mut().find(|(k, _)| k == key) {
            Some((_, value)) => *value = raw,
            None => self.0.push((key.to_owned(), raw)),
        }
    }

    fn set_string(&mut self, key: &str, value: &str) {
        self.set(key, json_string(value));
    }

    fn to_json(&self) -> String {
        let mut out = String::from("{");
        for (i, (key, value)) in self.0.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&json_string(key));
            out.push(':');
            out.push_str(value);
        }
        out.push('}');
        out
    }
}

fn json_string(text: &str) -> String {
    sonic_rs::to_string(text).expect("a str always serializes")
}

/// The command line after `etoon hook claude`.
#[derive(Debug, Default)]
struct Options {
    /// The `--pre` rewriter: a program and its arguments; the command goes last.
    pre: Option<Vec<String>>,
}

impl Options {
    fn parse(args: Vec<OsString>) -> Result<Self, String> {
        let mut options = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let arg = arg
                .into_string()
                .map_err(|a| format!("argument is not UTF-8: {a:?}"))?;
            let value = match arg.as_str() {
                "--pre" => args
                    .next()
                    .ok_or("--pre requires a command")?
                    .into_string()
                    .map_err(|a| format!("--pre value is not UTF-8: {a:?}"))?,
                _ => match arg.strip_prefix("--pre=") {
                    Some(value) => value.to_owned(),
                    None => return Err(format!("unknown argument: {arg}")),
                },
            };
            if options.pre.is_some() {
                return Err("--pre given twice".into());
            }
            let argv: Vec<String> = value.split_ascii_whitespace().map(str::to_owned).collect();
            if argv.is_empty() {
                return Err("--pre requires a command".into());
            }
            options.pre = Some(argv);
        }
        Ok(options)
    }
}

/// Runs `etoon hook claude` with the arguments after `claude`.
pub fn main(args: Vec<OsString>) -> ExitCode {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    // A usage error exits 1: Claude Code reports it and still runs the command.
    let options = match Options::parse(args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("etoon hook claude: {message}\n{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    // An unreadable stdin behaves like an empty one.
    let raw = crate::read_stdin().unwrap_or_default();
    let outcome = fail_open(|| run(&raw, &options));
    let mut stdout = io::stdout().lock();
    let _ = stdout.write_all(outcome.stdout.as_bytes());
    let _ = stdout.flush();
    ExitCode::from(outcome.code)
}

/// Turns a panic into a silent exit 0. The release profile unwinds, so the panic lands here.
fn fail_open(hook: impl FnOnce() -> Outcome) -> Outcome {
    panic::catch_unwind(AssertUnwindSafe(hook)).unwrap_or_else(|_| Outcome::silent())
}

/// What `rtk hook claude` did with the input.
enum Rtk {
    /// rtk is not on `PATH`.
    Absent,
    /// rtk exited 0 and printed nothing.
    Silent,
    /// rtk exited 0 with a JSON object.
    Decided(RawObject),
    /// A non-zero exit (rtk denies through JSON on exit 0, so exit 2 is a usage error too),
    /// a signal, a timeout, or output that is not a JSON object.
    Failed,
}

fn run(raw: &[u8], options: &Options) -> Outcome {
    let Some(mut root) = std::str::from_utf8(raw).ok().and_then(RawObject::parse) else {
        return Outcome::silent();
    };
    // Settings normally match `Bash` already; the check keeps other tools with a `command`
    // field safe when the matcher is wider. Old callers omit `tool_name`.
    if root.get("tool_name").is_some() && root.string("tool_name").as_deref() != Some("Bash") {
        return Outcome::silent();
    }
    let Some(mut tool_input) = root.object("tool_input") else {
        return Outcome::silent();
    };
    let Some(command) = tool_input.string("command") else {
        return Outcome::silent();
    };
    let original = command.trim_end_matches('\n');
    if original.trim().is_empty() {
        return Outcome::silent();
    }

    let pre_rewrite = options
        .pre
        .as_deref()
        .and_then(|pre| run_pre(pre, original))
        .filter(|rewritten| rewritten != original);
    let pre_changed = pre_rewrite.is_some();
    let rtk_input = match &pre_rewrite {
        Some(rewritten) => {
            tool_input.set_string("command", rewritten);
            root.set("tool_input", tool_input.to_json());
            root.to_json().into_bytes()
        }
        None => raw.to_vec(),
    };
    let cmd = pre_rewrite.as_deref().unwrap_or(original);
    let skip = skip_etoon(cmd);

    // Each failure keeps what already succeeded: the pre-rewritten command, or nothing.
    let fallback = |tool_input: &RawObject| {
        if pre_changed {
            Outcome::updated_input(tool_input)
        } else {
            Outcome::silent()
        }
    };
    match run_rtk(rtk_input) {
        Rtk::Failed => fallback(&tool_input),
        Rtk::Absent | Rtk::Silent if skip => fallback(&tool_input),
        Rtk::Absent | Rtk::Silent => {
            tool_input.set_string("command", &with_etoon(cmd));
            Outcome::updated_input(&tool_input)
        }
        Rtk::Decided(mut decision) => {
            match merge_decision(&mut decision, tool_input.clone(), skip, pre_changed) {
                Some(()) => Outcome::decision(&decision),
                None => fallback(&tool_input),
            }
        }
    }
}

/// Folds the tool input into rtk's decision and appends etoon to rtk's rewrite.
///
/// rtk's `updatedInput` fields win over the hook's; the other fields stay, since Claude Code
/// replaces the whole tool input with `updatedInput`. A decision with no `updatedInput`
/// (`permissionDecision: ask`, say) gets no etoon, but it still has to carry the
/// pre-rewritten command, since rtk's own input already held it.
fn merge_decision(
    decision: &mut RawObject,
    mut tool_input: RawObject,
    skip: bool,
    pre_changed: bool,
) -> Option<()> {
    let mut inner = match decision.get("hookSpecificOutput") {
        None | Some("null") => RawObject::default(),
        Some(raw) => RawObject::parse(raw)?,
    };
    // An `updatedInput` that is present but not a clean object fails the whole decision.
    let updated = match inner.get("updatedInput") {
        None | Some("null") => None,
        Some(raw) => Some(RawObject::parse(raw)?),
    };
    match updated {
        Some(updated) => {
            for (key, value) in updated.0 {
                tool_input.set(&key, value);
            }
            if !skip {
                let command = with_etoon(&tool_input.string("command")?);
                tool_input.set_string("command", &command);
            }
        }
        None if pre_changed => inner.set_string("hookEventName", "PreToolUse"),
        None => return Some(()),
    }
    inner.set("updatedInput", tool_input.to_json());
    decision.set("hookSpecificOutput", inner.to_json());
    Some(())
}

/// Appends the etoon pipe. Trailing newlines go first, and so do blank last lines: a `|` at
/// the start of a line is a syntax error. Only the blanks bash splits words on count; bash
/// reads U+3000 as part of a word. Blanks at the end of the last line stay as written.
fn with_etoon(command: &str) -> String {
    let body = command.trim_end_matches([' ', '\t', '\n']);
    let body = if command[body.len()..].contains('\n') {
        body
    } else {
        command
    };
    format!("{body} | etoon{PIPE_SUFFIX}")
}

/// Pipes the input to `rtk hook claude`.
fn run_rtk(input: Vec<u8>) -> Rtk {
    let mut command = Command::new("rtk");
    command.args(["hook", "claude"]);
    let finished = match run_bounded(&mut command, Some(input), RTK_TIMEOUT) {
        Ok(Some(finished)) if finished.status.success() => finished,
        Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::PermissionDenied) => {
            return Rtk::Absent;
        }
        _ => return Rtk::Failed,
    };
    match String::from_utf8(finished.stdout) {
        Ok(stdout) if stdout.trim().is_empty() => Rtk::Silent,
        Ok(stdout) => RawObject::parse(&stdout).map_or(Rtk::Failed, Rtk::Decided),
        Err(_) => Rtk::Failed,
    }
}

/// Runs the `--pre` rewriter: `PROGRAM [ARGS...] COMMAND`, with the rewritten command on
/// stdout. Returns `None` (keep the command) when the rewriter cannot start, exits non-zero,
/// prints nothing, only blanks or non-UTF-8, or runs past [`PRE_TIMEOUT`].
fn run_pre(pre: &[String], command: &str) -> Option<String> {
    // An argument cannot carry a NUL byte, and a command that starts with `-` would reach
    // the rewriter as an option (`--help` would print usage text as the "new command").
    if command.contains('\0') || command.trim_start().starts_with('-') {
        return None;
    }
    let (program, args) = pre.split_first()?;
    let mut rewriter = Command::new(program);
    rewriter.args(args).arg(command);
    let finished = run_bounded(&mut rewriter, None, PRE_TIMEOUT).ok()??;
    if !finished.status.success() {
        return None;
    }
    let rewritten = String::from_utf8(finished.stdout).ok()?;
    let rewritten = rewritten.trim_end_matches('\n');
    (!rewritten.trim().is_empty()).then(|| rewritten.to_owned())
}

/// A child that exited within its deadline.
struct Finished {
    status: ExitStatus,
    stdout: Vec<u8>,
}

/// Runs `command` with `input` on stdin, captures stdout and discards stderr.
///
/// Returns `Err` when the child cannot start, and `Ok(None)` when it outlives `timeout` or
/// its stdout cannot be read; the child is then killed. stderr goes nowhere: a grandchild
/// that survives the kill would otherwise hold Claude Code's stderr pipe open.
fn run_bounded(
    command: &mut Command,
    input: Option<Vec<u8>>,
    timeout: Duration,
) -> io::Result<Option<Finished>> {
    let stdin = if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    };
    let mut child = command
        .stdin(stdin)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    if let (Some(input), Some(mut pipe)) = (input, child.stdin.take()) {
        // A writer thread keeps a large input from deadlocking against the child's output.
        // A child may exit without reading its input; a broken pipe is not an error here.
        thread::spawn(move || {
            let _ = pipe.write_all(&input);
        });
    }
    let deadline = Instant::now() + timeout;
    let stdout = read_until(&mut child, deadline);
    let status = stdout
        .as_ref()
        .and_then(|_| wait_until(&mut child, deadline));
    let (Some(stdout), Some(status)) = (stdout, status) else {
        let _ = child.kill();
        let _ = child.wait();
        return Ok(None);
    };
    Ok(Some(Finished { status, stdout }))
}

/// Reads the child's stdout to its end, or gives up at the deadline.
fn read_until(child: &mut Child, deadline: Instant) -> Option<Vec<u8>> {
    let mut stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    // Detached: a hung reader dies with the process, which exits right after the hook.
    thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = tx.send(stdout.read_to_end(&mut buf).map(|_| buf));
    });
    rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .ok()?
        .ok()
}

/// Waits for the child after its stdout closed, or gives up at the deadline.
fn wait_until(child: &mut Child, deadline: Instant) -> Option<ExitStatus> {
    let mut pause = Duration::from_micros(50);
    loop {
        if let Some(status) = child.try_wait().ok()? {
            return Some(status);
        }
        if Instant::now() >= deadline {
            return None;
        }
        thread::sleep(pause);
        pause = (pause * 2).min(Duration::from_millis(10));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn test_options_parse_pre_forms_split_on_whitespace() {
        for form in [&["--pre", "a  b\tc"][..], &["--pre=a b c"]] {
            let options = Options::parse(args(form)).expect("valid");
            assert_eq!(
                options.pre.as_deref(),
                Some(&["a", "b", "c"].map(String::from)[..])
            );
        }
        assert!(Options::parse(Vec::new()).expect("valid").pre.is_none());
    }

    #[test]
    fn test_options_parse_bad_arguments_return_error() {
        for bad in [
            &["--pre"][..],
            &["--pre", "   "],
            &["--pre="],
            &["--pre", "a", "--pre", "b"],
            &["--bogus"],
            &["claude"],
        ] {
            assert!(Options::parse(args(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn test_with_etoon_trailing_newlines_dropped() {
        assert_eq!(with_etoon("ls\n\n"), format!("ls | etoon{PIPE_SUFFIX}"));
    }

    #[test]
    fn test_merge_decision_non_object_output_returns_none() {
        let mut decision = RawObject::parse(r#"{"hookSpecificOutput":"x"}"#).expect("object");
        assert!(merge_decision(&mut decision, RawObject::default(), false, true).is_none());
    }

    #[test]
    fn test_raw_object_edit_keeps_order_and_untouched_bytes() {
        let mut object =
            RawObject::parse(r#"{ "b": 1.50, "a": "\u0041", "c": [1, 2] }"#).expect("object");
        object.set_string("a", "x");
        object.set("z", "true".into());
        assert_eq!(
            object.to_json(),
            r#"{"b":1.50,"a":"x","c":[1, 2],"z":true}"#
        );
    }

    #[test]
    fn test_raw_object_parse_rejects_invalid_or_non_object() {
        for bad in ["", "[1]", "\"s\"", "{\"a\":tru}", "{\"a\":1} x", "{\"a\":"] {
            assert!(RawObject::parse(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn test_fail_open_panic_returns_silent_exit_zero() {
        assert_eq!(fail_open(|| panic!("boom")), Outcome::silent());
    }
}
