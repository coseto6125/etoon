//! `etoon hook claude` end to end: the binary runs with stub `rtk` and `--pre` scripts.
#![cfg(unix)]

use sonic_rs::{JsonContainerTrait, JsonValueTrait, Value};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_etoon");
const SUFFIX: &str = "; __etoon_rc=${PIPESTATUS[${#PIPESTATUS[@]}-2]}; (exit $__etoon_rc)";

/// `rtk` reads `RTK_STUB`, logs its stdin to `RTK_LOG`, then acts. Its rewrite drops every
/// field but `command`, so the tests also see the hook restore the other fields.
const RTK: &str = r#"#!/bin/sh
# flood writes past the pipe buffer before it reads any input.
if [ "$RTK_STUB" = flood ]; then head -c 300000 /dev/zero | tr '\0' x; exec cat >/dev/null; fi
input=$(cat)
[ -n "$RTK_LOG" ] && printf '%s' "$input" > "$RTK_LOG"
cmd=$(printf '%s' "$input" | sed -n 's/.*"command":"\([^"]*\)".*/\1/p')
case "$RTK_STUB" in
    rewrite) printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecisionReason":"RTK auto-rewrite","updatedInput":{"command":"rtk %s"}}}\n' "$cmd" ;;
    none) ;;
    ask) echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"ask"}}' ;;
    block) echo "rtk blocked" >&2; printf 'blocked-out'; exit 2 ;;
    fail1) echo "rtk broke" >&2; exit 1 ;;
    badjson) echo '{"hookSpecificOutput":' ;;
    nonobject) echo '[1,2]' ;;
    badshape) echo '{"hookSpecificOutput":"x"}' ;;
    signal) kill -9 $$ ;;
    hang) sleep 10 ;;
    override) printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","updatedInput":{"command":"rtk %s","timeout":1}}}\n' "$cmd" ;;
    nocommand) echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","updatedInput":{"timeout":1}}}' ;;
    dupkey) echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","updatedInput":{"command":"a","command":"b"}}}' ;;
esac
"#;

const PRE_STUBS: &[(&str, &str)] = &[
    ("pre-prefix", r#"printf 'PRE %s' "$1""#),
    ("pre-newline", r#"printf 'PRE %s\n\n' "$1""#),
    ("pre-same", r#"printf '%s' "$1""#),
    ("pre-args", r#"printf '%s+%s+%s' "$1" "$2" "$3""#),
    ("pre-fail", r#"printf 'PRE %s' "$1"; exit 1"#),
    ("pre-empty", "exit 0"),
    ("pre-hang", "sleep 10"),
    ("pre-close-hang", "exec >&-; sleep 10"),
    ("pre-badutf8", r"printf '\377'"),
    ("pre-blank", "printf '   '"),
];

/// The stub directory, written once before any test spawns a process.
fn stubs() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        // The "rtk absent" runs keep /usr/bin and /bin on PATH, so a host rtk there would
        // take the stub's place.
        for host_rtk in ["/usr/bin/rtk", "/bin/rtk"] {
            assert!(
                !Path::new(host_rtk).exists(),
                "{host_rtk} shadows the rtk stubs"
            );
        }
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("hook-claude-stubs-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("create stub dir");
        let mut scripts = vec![("rtk".to_owned(), RTK.to_owned())];
        scripts.extend(
            PRE_STUBS
                .iter()
                .map(|(name, body)| ((*name).to_owned(), format!("#!/bin/sh\n{body}\n"))),
        );
        for (name, body) in scripts {
            let path = dir.join(name);
            fs::write(&path, body).expect("write stub");
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod stub");
        }
        dir
    })
}

#[derive(Debug)]
struct Out {
    stdout: String,
    code: i32,
}

impl Out {
    fn json(&self) -> Value {
        sonic_rs::from_str(&self.stdout).unwrap_or_else(|e| panic!("{e}: {:?}", self.stdout))
    }

    fn updated(&self) -> Value {
        self.json()["hookSpecificOutput"]["updatedInput"].clone()
    }

    fn command(&self) -> String {
        self.updated()["command"]
            .as_str()
            .unwrap_or_else(|| panic!("no command in {:?}", self.stdout))
            .to_owned()
    }

    fn is_silent_zero(&self) -> bool {
        self.stdout.is_empty() && self.code == 0
    }
}

/// Where rtk lives for one run.
#[derive(Clone, Copy)]
enum Rtk<'a> {
    Absent,
    Stub(&'a str),
}

struct Run<'a> {
    rtk: Rtk<'a>,
    args: Vec<String>,
    log: Option<&'a Path>,
}

impl<'a> Run<'a> {
    fn new(rtk: Rtk<'a>) -> Self {
        Self {
            rtk,
            args: Vec::new(),
            log: None,
        }
    }

    fn pre(mut self, pre: &str) -> Self {
        self.args = vec!["--pre".into(), stub_command(pre)];
        self
    }

    fn args(mut self, args: &[String]) -> Self {
        self.args = args.to_vec();
        self
    }

    fn log(mut self, log: &'a Path) -> Self {
        self.log = Some(log);
        self
    }

    fn stdin(&self, stdin: &[u8]) -> Out {
        // The rtk-free directory still holds the --pre stubs; only rtk differs.
        let bin_dir = match self.rtk {
            Rtk::Stub(_) => stubs().to_path_buf(),
            Rtk::Absent => {
                let dir = stubs().join("no-rtk");
                fs::create_dir_all(&dir).expect("create no-rtk dir");
                dir
            }
        };
        let mut child = Command::new(BIN)
            .args(["hook", "claude"])
            .args(&self.args)
            .env("PATH", format!("{}:/usr/bin:/bin", bin_dir.display()))
            .env(
                "RTK_STUB",
                match self.rtk {
                    Rtk::Stub(mode) => mode,
                    Rtk::Absent => "",
                },
            )
            .env("RTK_LOG", self.log.map(Path::as_os_str).unwrap_or_default())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn etoon");
        let mut pipe = child.stdin.take().expect("stdin");
        let stdin = stdin.to_vec();
        let writer = std::thread::spawn(move || {
            let _ = pipe.write_all(&stdin);
        });
        let output = child.wait_with_output().expect("wait etoon");
        writer.join().expect("writer");
        Out {
            stdout: String::from_utf8(output.stdout).expect("utf-8 stdout"),
            code: output.status.code().expect("exit code"),
        }
    }

    fn command(&self, cmd: &str) -> Out {
        self.stdin(input(cmd).as_bytes())
    }
}

/// `NAME [ARGS]` with NAME resolved to its stub path.
fn stub_command(pre: &str) -> String {
    let first = stubs().join(pre.split(' ').next().expect("name"));
    let rest = pre.split_once(' ').map_or("", |(_, r)| r);
    format!("{} {rest}", first.display())
}

fn input(cmd: &str) -> String {
    let cmd = sonic_rs::to_string(cmd).expect("encode");
    format!(
        r#"{{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{{"command":{cmd}}}}}"#
    )
}

fn etoon(cmd: &str) -> String {
    format!("{cmd} | etoon{SUFFIX}")
}

// --- stdin shapes -----------------------------------------------------------------------

#[test]
fn test_hook_empty_or_junk_stdin_exits_zero_silent() {
    for junk in ["", "not json", "{", "[1,2]", "\"str\"", "5", "null"] {
        let out = Run::new(Rtk::Stub("rewrite")).stdin(junk.as_bytes());
        assert!(out.is_silent_zero(), "{junk:?}: {out:?}");
    }
}

#[test]
fn test_hook_non_bash_tool_exits_zero_silent() {
    let raw = r#"{"tool_name":"Monitor","tool_input":{"command":"ls"}}"#;
    let out = Run::new(Rtk::Absent).stdin(raw.as_bytes());
    assert!(out.is_silent_zero(), "{out:?}");
}

#[test]
fn test_hook_missing_or_non_string_command_exits_zero_silent() {
    for raw in [
        r#"{}"#,
        r#"{"tool_input":null}"#,
        r#"{"tool_input":"x"}"#,
        r#"{"tool_input":[1]}"#,
        r#"{"tool_input":{}}"#,
        r#"{"tool_input":{"command":null}}"#,
        r#"{"tool_input":{"command":["ls","-l"]}}"#,
        r#"{"tool_input":{"command":5}}"#,
    ] {
        let out = Run::new(Rtk::Absent).stdin(raw.as_bytes());
        assert!(out.is_silent_zero(), "{raw}: {out:?}");
    }
}

#[test]
fn test_hook_empty_or_whitespace_command_exits_zero_silent() {
    for cmd in ["", "\n\n", "   ", " \t \n", "\u{3000}"] {
        let out = Run::new(Rtk::Absent).command(cmd);
        assert!(out.is_silent_zero(), "{cmd:?}: {out:?}");
    }
}

#[test]
fn test_hook_tool_name_absent_still_rewrites() {
    let out = Run::new(Rtk::Absent).stdin(br#"{"tool_input":{"command":"ls"}}"#);
    assert_eq!(out.command(), etoon("ls"));
}

// --- etoon suffix -----------------------------------------------------------------------

#[test]
fn test_hook_rtk_absent_appends_etoon_and_keeps_fields_in_order() {
    let raw = r#"{"tool_name":"Bash","tool_input":{"command":"ls","description":"d","timeout":5000,"run_in_background":true}}"#;
    let out = Run::new(Rtk::Absent).stdin(raw.as_bytes());
    let want = format!(
        r#"{{"command":{},"description":"d","timeout":5000,"run_in_background":true}}"#,
        sonic_rs::to_string(&etoon("ls")).expect("encode")
    );
    assert_eq!(sonic_rs::to_string(&out.updated()).expect("encode"), want);
    assert_eq!(
        out.json()["hookSpecificOutput"]["hookEventName"],
        "PreToolUse"
    );
}

#[test]
fn test_hook_already_etoon_or_heredoc_exits_zero_silent() {
    for cmd in [
        "ls | etoon",
        "ls |etoon -x",
        "cat <<EOF\nhi\nEOF",
        "git diff HEAD",
    ] {
        let out = Run::new(Rtk::Absent).command(cmd);
        assert!(out.is_silent_zero(), "{cmd:?}: {out:?}");
    }
}

#[test]
fn test_hook_multiline_command_gets_etoon_after_last_line() {
    let out = Run::new(Rtk::Absent).command("echo a\necho b\n\n");
    assert_eq!(out.command(), etoon("echo a\necho b"));
}

#[test]
fn test_hook_blank_last_line_dropped_before_pipe() {
    let out = Run::new(Rtk::Absent).command("echo hi\n   \n\t");
    assert_eq!(out.command(), etoon("echo hi"));
}

#[test]
fn test_hook_trailing_blanks_on_last_line_kept() {
    let out = Run::new(Rtk::Absent).command("echo a  ");
    assert_eq!(out.command(), etoon("echo a  "));
}

#[test]
fn test_hook_builtin_after_and_gets_no_etoon() {
    let out = Run::new(Rtk::Absent).command("true && cd /");
    assert!(out.is_silent_zero(), "{out:?}");
}

#[test]
fn test_hook_suffix_keeps_command_exit_code() {
    let dir = stubs().join("cat-etoon");
    fs::create_dir_all(&dir).expect("dir");
    let etoon_stub = dir.join("etoon");
    if !etoon_stub.exists() {
        fs::write(&etoon_stub, "#!/bin/sh\ncat\n").expect("write");
        fs::set_permissions(&etoon_stub, fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let rc = |cmd: &str| {
        Command::new("bash")
            .args(["-c", cmd])
            .env("PATH", format!("{}:/usr/bin:/bin", dir.display()))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("bash")
            .code()
    };
    for cmd in [
        "false",
        "true",
        "sh -c 'exit 3'",
        "true | false",
        "printf x; false",
        "false\n  ",
    ] {
        let rewritten = Run::new(Rtk::Absent).command(cmd).command();
        assert_eq!(rc(&rewritten), rc(cmd), "{cmd}");
    }
}

// --- rtk chaining -----------------------------------------------------------------------

#[test]
fn test_hook_rtk_rewrite_gets_etoon_and_restores_dropped_fields() {
    let raw = r#"{"tool_name":"Bash","tool_input":{"command":"git status","description":"d","timeout":5000}}"#;
    let out = Run::new(Rtk::Stub("rewrite")).stdin(raw.as_bytes());
    let updated = out.updated();
    assert_eq!(updated["command"], etoon("rtk git status"));
    assert_eq!(updated["description"], "d");
    assert_eq!(updated["timeout"], 5000);
    assert_eq!(
        out.json()["hookSpecificOutput"]["permissionDecisionReason"],
        "RTK auto-rewrite"
    );
}

#[test]
fn test_hook_rtk_rewrite_on_skip_rule_keeps_rtk_without_etoon() {
    for (cmd, want) in [
        ("git diff HEAD", "rtk git diff HEAD"),
        ("ls | etoon", "rtk ls | etoon"),
        ("cat <<EOF", "rtk cat <<EOF"),
    ] {
        assert_eq!(Run::new(Rtk::Stub("rewrite")).command(cmd).command(), want);
    }
}

#[test]
fn test_hook_rtk_fields_override_hook_fields() {
    let raw =
        r#"{"tool_name":"Bash","tool_input":{"command":"ls","timeout":5000,"description":"d"}}"#;
    let out = Run::new(Rtk::Stub("override")).stdin(raw.as_bytes());
    let updated = out.updated();
    assert_eq!(updated["command"], etoon("rtk ls"));
    assert_eq!(
        (updated["timeout"].as_u64(), updated["description"].as_str()),
        (Some(1), Some("d"))
    );
}

#[test]
fn test_hook_rtk_update_without_command_keeps_command() {
    let out = Run::new(Rtk::Stub("nocommand")).command("ls");
    let updated = out.updated();
    assert_eq!(
        (updated["command"].as_str(), updated["timeout"].as_u64()),
        (Some(etoon("ls").as_str()), Some(1))
    );
}

#[test]
fn test_hook_rtk_silent_appends_etoon() {
    let out = Run::new(Rtk::Stub("none")).command("python3 x.py");
    assert_eq!(out.command(), etoon("python3 x.py"));
}

#[test]
fn test_hook_rtk_decision_without_rewrite_passes_unchanged() {
    let out = Run::new(Rtk::Stub("ask")).command("ls");
    assert_eq!(
        out.stdout,
        "{\"hookSpecificOutput\":{\"hookEventName\":\"PreToolUse\",\"permissionDecision\":\"ask\"}}\n"
    );
}

#[test]
fn test_hook_rtk_failure_exits_zero_silent() {
    // Exit 2 included: rtk denies through JSON on exit 0, and clap exits 2 on a usage error.
    for mode in [
        "block",
        "fail1",
        "badjson",
        "nonobject",
        "badshape",
        "signal",
        "dupkey",
    ] {
        let out = Run::new(Rtk::Stub(mode)).command("ls");
        assert!(out.is_silent_zero(), "{mode}: {out:?}");
    }
}

#[test]
fn test_hook_large_command_does_not_deadlock() {
    let cmd = format!("echo {}", "x".repeat(200_000));
    let out = Run::new(Rtk::Stub("rewrite")).command(&cmd);
    assert_eq!(out.command(), etoon(&format!("rtk {cmd}")));
}

#[test]
fn test_hook_rtk_output_before_input_read_does_not_deadlock() {
    // rtk fills its stdout pipe before it reads a stdin larger than the pipe buffer. A
    // writer on the hook's main thread would block; the 5 s rtk timeout would hide that.
    let cmd = format!("echo {}", "x".repeat(200_000));
    let start = Instant::now();
    let out = Run::new(Rtk::Stub("flood")).command(&cmd);
    assert!(out.is_silent_zero(), "{:?}", out.code);
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "took {:?}",
        start.elapsed()
    );
}

#[test]
fn test_hook_rtk_hang_times_out_silent() {
    let start = Instant::now();
    let out = Run::new(Rtk::Stub("hang")).command("ls");
    assert!(out.is_silent_zero(), "{out:?}");
    assert!(
        start.elapsed() < Duration::from_secs(9),
        "took {:?}",
        start.elapsed()
    );
}

// --- --pre rewriter ---------------------------------------------------------------------

#[test]
fn test_hook_pre_rewrite_feeds_rtk_then_gets_etoon() {
    let log = stubs().join("rtk-log-pre-rewrite");
    let out = Run::new(Rtk::Stub("rewrite"))
        .pre("pre-prefix")
        .log(&log)
        .command("ls");
    assert_eq!(out.command(), etoon("rtk PRE ls"));
    let seen: Value = sonic_rs::from_slice(&fs::read(&log).expect("rtk log")).expect("json");
    assert_eq!(seen["tool_input"]["command"], "PRE ls");
}

#[test]
fn test_hook_pre_rewrite_without_rtk_gets_etoon() {
    let out = Run::new(Rtk::Absent).pre("pre-prefix").command("ls");
    assert_eq!(out.command(), etoon("PRE ls"));
}

#[test]
fn test_hook_pre_output_trailing_newlines_dropped() {
    let out = Run::new(Rtk::Absent).pre("pre-newline").command("ls");
    assert_eq!(out.command(), etoon("PRE ls"));
}

#[test]
fn test_hook_pre_extra_args_come_before_command() {
    let out = Run::new(Rtk::Absent).pre("pre-args one two").command("ls");
    assert_eq!(out.command(), etoon("one+two+ls"));
}

#[test]
fn test_hook_pre_survives_rtk_failure_without_etoon() {
    for mode in ["block", "fail1", "badjson", "signal"] {
        let out = Run::new(Rtk::Stub(mode)).pre("pre-prefix").command("ls");
        assert_eq!(
            (out.command(), out.code),
            ("PRE ls".to_owned(), 0),
            "{mode}"
        );
    }
}

#[test]
fn test_hook_pre_survives_skip_rule() {
    let out = Run::new(Rtk::Absent)
        .pre("pre-prefix")
        .command("ls > out.txt");
    assert_eq!(out.command(), "PRE ls > out.txt");
}

#[test]
fn test_hook_pre_rides_on_rtk_decision_without_rewrite() {
    let out = Run::new(Rtk::Stub("ask")).pre("pre-prefix").command("ls");
    assert_eq!(out.command(), "PRE ls");
    assert_eq!(
        out.json()["hookSpecificOutput"]["permissionDecision"],
        "ask"
    );
}

#[test]
fn test_hook_pre_equals_form_matches_separate_form() {
    let args = [format!("--pre={}", stub_command("pre-prefix"))];
    let out = Run::new(Rtk::Absent).args(&args).command("ls");
    assert_eq!(out.command(), etoon("PRE ls"));
}

#[test]
fn test_hook_pre_skipped_for_dash_command() {
    let out = Run::new(Rtk::Absent).pre("pre-prefix").command("--help");
    assert_eq!(out.command(), etoon("--help"));
}

#[test]
fn test_hook_pre_unchanged_output_acts_like_no_pre() {
    let out = Run::new(Rtk::Stub("ask")).pre("pre-same").command("ls");
    assert!(out.json()["hookSpecificOutput"]
        .get("updatedInput")
        .is_none());
}

#[test]
fn test_hook_pre_failure_keeps_original_command() {
    for pre in [
        "pre-fail",
        "pre-empty",
        "pre-badutf8",
        "pre-blank",
        "does-not-exist",
        "pre-hang",
        "pre-close-hang",
    ] {
        let start = Instant::now();
        let out = Run::new(Rtk::Absent).pre(pre).command("ls");
        assert_eq!(out.command(), etoon("ls"), "{pre}");
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "{pre} took {:?}",
            start.elapsed()
        );
    }
}

#[test]
fn test_hook_pre_skipped_for_nul_command() {
    let out = Run::new(Rtk::Absent).pre("pre-prefix").command("echo a\0b");
    assert_eq!(out.command(), etoon("echo a\0b"));
}

// --- command line -----------------------------------------------------------------------

#[test]
fn test_hook_bad_arguments_exit_one_without_output() {
    for args in [
        &["--pre"][..],
        &["--pre", " "],
        &["--nope"],
        &["--pre", "a", "--pre=b"],
    ] {
        let out = Command::new(BIN)
            .args(["hook", "claude"])
            .args(args)
            .stdin(Stdio::null())
            .output()
            .expect("run");
        assert_eq!(
            (out.stdout.len(), out.status.code()),
            (0, Some(1)),
            "{args:?}"
        );
    }
}

#[test]
fn test_hook_help_exits_zero() {
    let out = Command::new(BIN)
        .args(["hook", "claude", "--help"])
        .output()
        .expect("run");
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains("--pre"));
}

#[test]
fn test_cli_file_named_hook_still_encodes() {
    let dir = stubs().join("file-named-hook");
    fs::create_dir_all(&dir).expect("dir");
    fs::write(dir.join("hook"), r#"{"a":1}"#).expect("write");
    let out = Command::new(BIN)
        .arg("hook")
        .current_dir(&dir)
        .output()
        .expect("run");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "a: 1\n");
}

#[test]
fn test_hook_decision_is_one_json_object() {
    let out = Run::new(Rtk::Absent).command("ls");
    assert!(out.json().as_object().is_some());
    assert!(out.stdout.ends_with('\n'));
}
