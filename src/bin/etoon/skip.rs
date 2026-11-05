//! The eight rules that decide whether a command goes without the `| etoon` suffix.
//!
//! Each rule tests the command line by line, the way the `grep`/`sed`/`awk` original did.
//! `SP` is the glibc `iswspace` set of a UTF-8 locale without the newline: ASCII blanks plus
//! U+1680, U+2000-2006, U+2008-200A, U+2028, U+2029, U+205F and U+3000 (not NBSP).
//! Each matcher names the regex of the original; `tests` checks them against it.

/// A pipe opens a subshell, so a builtin from this list would lose its side effect.
const BUILTINS: [&str; 18] = [
    "cd", "export", "unset", "alias", "unalias", "source", ".", "set", "shopt", "pushd", "popd",
    "exit", "return", "trap", "exec", "read", "wait", "kill",
];

/// `SP`: one character of the blank set in the module docs.
fn is_blank(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\x0B' | '\x0C' | '\r' | '\u{1680}' | '\u{2000}'..='\u{2006}'
        | '\u{2008}'..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{205F}' | '\u{3000}')
}

fn starts_blank(s: &str) -> bool {
    s.starts_with(is_blank)
}

/// The start of each match of `pat` in `line`, overlapping ones included (`{{{` holds two
/// `{{`), as a regex would try them.
fn starts<'a>(line: &'a str, pat: &'a str) -> impl Iterator<Item = usize> + 'a {
    line.char_indices()
        .map(|(i, _)| i)
        .filter(move |&i| line[i..].starts_with(pat))
}

/// The text after each match of `pat` in `line`.
fn after<'a>(line: &'a str, pat: &'a str) -> impl Iterator<Item = &'a str> + 'a {
    starts(line, pat).map(move |i| &line[i + pat.len()..])
}

/// `true` when the byte before `i` exists and is `<`. `<` is ASCII, so no UTF-8 tail matches.
fn after_lt(line: &str, i: usize) -> bool {
    i > 0 && line.as_bytes()[i - 1] == b'<'
}

/// Rule 1, `\|SP*etoon(SP|$)`: the command already pipes into etoon.
fn pipes_etoon(line: &str) -> bool {
    after(line, "|").any(|rest| {
        rest.trim_start_matches(is_blank)
            .strip_prefix("etoon")
            .is_some_and(|r| r.is_empty() || starts_blank(r))
    })
}

/// Rule 3, `(^|[^<])>>?SP`: an output redirect.
fn redirects(segment: &str) -> bool {
    segment.match_indices('>').any(|(i, _)| {
        let rest = &segment[i + 1..];
        !after_lt(segment, i)
            && (starts_blank(rest) || rest.strip_prefix('>').is_some_and(starts_blank))
    })
}

/// Rule 5, `^(git SP+ diff|diff)\b`: byte-exact diff.
fn is_diff(line: &str) -> bool {
    let rest = match line.strip_prefix("git") {
        Some(git) if starts_blank(git) => git.trim_start_matches(is_blank).strip_prefix("diff"),
        _ => line.strip_prefix("diff"),
    };
    // `\b` after `f`: the next character is not a word character.
    rest.is_some_and(|r| !r.starts_with(is_word))
}

/// Regex `\w`, close enough: alphanumeric, connector punctuation (`Pc`) or a join control.
/// It differs on combining marks (`\w`, not alphanumeric) and on `No` digits such as `²`
/// (alphanumeric, not `\w`); no std predicate names those categories.
fn is_word(c: char) -> bool {
    c.is_alphanumeric()
        || matches!(
            c,
            '_' | '\u{203F}' | '\u{2040}' | '\u{2054}' | '\u{FE33}' | '\u{FE34}' | '\u{FE4D}'
                ..='\u{FE4F}' | '\u{FF3F}' | '\u{200C}' | '\u{200D}'
        )
}

/// Rule 6: the caller asks for JSON output.
/// `--(format|output)[= ]["']?json|-o[= ]?["']?json|-{1,2}json(SP|["'=`]|$)|\{\{ *json`
fn asks_json(line: &str) -> bool {
    let quoted_json = |r: &str| r.strip_prefix(['"', '\'']).unwrap_or(r).starts_with("json");
    let long = ["--format", "--output"]
        .into_iter()
        .any(|flag| after(line, flag).any(|r| r.strip_prefix(['=', ' ']).is_some_and(quoted_json)));
    let short = after(line, "-o").any(|r| quoted_json(r.strip_prefix(['=', ' ']).unwrap_or(r)));
    let dash = after(line, "-json")
        .any(|r| r.is_empty() || starts_blank(r) || r.starts_with(['"', '\'', '=', '`']));
    let template = after(line, "{{").any(|r| r.trim_start_matches(' ').starts_with("json"));
    long || short || dash || template
}

/// Rule 7, `(^|[^<])<<([^<]|$)`: a heredoc, `<<` that is not `<<<`.
fn has_heredoc(line: &str) -> bool {
    starts(line, "<<").any(|i| !after_lt(line, i) && !line[i + 2..].starts_with('<'))
}

/// Rule 4: the pipe attaches to the last command of the last line, after its last `&&`,
/// `||`, `;` or `&`. A builtin there would run in the pipeline's subshell and lose its
/// effect (`true && cd /`). A command that already holds a `|` runs in a subshell anyway.
/// Quotes are not parsed, so `echo "a; cd b"` skips too: a skip only costs the compression.
fn attaches_to_builtin(trimmed: &str) -> bool {
    let line = trimmed.rsplit('\n').next().unwrap_or(trimmed);
    let cut = [
        line.rfind(['&', ';']).map(|i| i + 1),
        line.rfind("||").map(|i| i + 2),
    ]
    .into_iter()
    .flatten()
    .max()
    .unwrap_or(0);
    let last = &line[cut..];
    !last.contains('|')
        && last
            .split([' ', '\t'])
            .find(|word| !word.is_empty())
            .is_some_and(|word| BUILTINS.contains(&word))
}

fn any_line(rule: fn(&str) -> bool, text: &str) -> bool {
    text.split('\n').any(rule)
}

/// Returns `true` when the command must not get the `| etoon` suffix.
pub fn skip_etoon(cmd: &str) -> bool {
    // 1. already piped into etoon
    if any_line(pipes_etoon, cmd) {
        return true;
    }

    // 2. a trailing `&` (the pipe would pull the job into the foreground), `;` (`; |` is a
    // syntax error) or `\` (a line continuation), after trailing blanks and blank lines
    let trimmed = cmd.trim_end_matches(|c| c == '\n' || is_blank(c));
    if trimmed.ends_with(['&', ';', '\\']) {
        return true;
    }

    // 3. the output already goes to a file
    if cmd.split('\n').any(|line| {
        let line = line.trim_end_matches(is_blank);
        redirects(line.rsplit('|').next().unwrap_or(line))
    }) {
        return true;
    }

    // 4. the command the pipe attaches to is a builtin
    if attaches_to_builtin(trimmed) {
        return true;
    }

    // 5-7. byte-exact diff, JSON output, heredoc (`| etoon` on the last line would turn the
    // closing delimiter into `EOF | etoon`)
    if any_line(is_diff, cmd) || any_line(asks_json, cmd) || any_line(has_heredoc, cmd) {
        return true;
    }

    // 8. pipefail: the status suffix reads one pipeline element, so an earlier failure is lost
    cmd.contains("pipefail")
}

#[cfg(test)]
mod tests {
    use super::{asks_json, has_heredoc, is_diff, pipes_etoon, redirects, skip_etoon};
    use regex::Regex;

    const SP: &str = r"[ \t\x0B\x0C\r\x{1680}\x{2000}-\x{2006}\x{2008}-\x{200A}\x{2028}\x{2029}\x{205F}\x{3000}]";

    /// Each matcher next to the regex of the original hook.
    type Oracle = (&'static str, fn(&str) -> bool, Regex);

    fn oracles() -> Vec<Oracle> {
        let re = |p: String| Regex::new(&p).expect("oracle pattern");
        vec![
            (
                "pipes_etoon",
                pipes_etoon,
                re(format!(r"\|{SP}*etoon({SP}|$)")),
            ),
            ("redirects", redirects, re(format!(r"(^|[^<])>>?{SP}"))),
            ("is_diff", is_diff, re(format!(r"^(git{SP}+diff|diff)\b"))),
            (
                "asks_json",
                asks_json,
                re(format!(
                    r#"--(format|output)[= ]["']?json|-o[= ]?["']?json|-{{1,2}}json({SP}|["'=`]|$)|\{{\{{ *json"#
                )),
            ),
            ("has_heredoc", has_heredoc, re(r"(^|[^<])<<([^<]|$)".into())),
        ]
    }

    /// Lines built from the tokens the rules look at, by a fixed xorshift sequence.
    fn generated_lines() -> Vec<String> {
        const TOKENS: [&str; 37] = [
            "|", " ", "  ", "\t", "\u{3000}", "\u{a0}", "etoon", "etoo", "e", ">", ">>", "<", "<<",
            "-", "--", "o", "-o", "json", "jso", "=", "\"", "'", "`", "{", "{{", "git", "diff",
            "f", "_", "x", "format", "output", "é", "1", "\u{203F}", "\u{200D}", "\u{FF3F}",
        ];
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        (0..50_000)
            .map(|_| {
                let len = next() % 8 + 1;
                (0..len)
                    .map(|_| TOKENS[(next() % TOKENS.len() as u64) as usize])
                    .collect()
            })
            .collect()
    }

    #[test]
    fn test_matchers_agree_with_original_regexes() {
        let corpus = GOLDEN.iter().flat_map(|(cmd, _)| cmd.split('\n'));
        let generated = generated_lines();
        let lines: Vec<&str> = corpus.chain(generated.iter().map(String::as_str)).collect();
        for (name, matcher, regex) in oracles() {
            let wrong: Vec<_> = lines
                .iter()
                .filter(|line| matcher(line) != regex.is_match(line))
                .take(5)
                .collect();
            assert!(wrong.is_empty(), "{name} differs on {wrong:?}");
        }
    }

    /// Verdicts of the bash and `cc-auto-etoon` originals over their differential corpus,
    /// except where a comment marks a deliberate change.
    const GOLDEN: &[(&str, bool)] = &[
        ("ls | etoon", true),
        ("ls |etoon", true),
        ("ls |   etoon -x", true),
        ("ls | etoonx", false),
        ("ls | etoon_x", false),
        ("echo etoon", false),
        ("ls |\tetoon", true),
        ("sleep 1 &", true),
        ("sleep 1 &  ", true),
        ("ls ;", true),
        ("ls;", true),
        ("printf %s hello\\", true),
        ("ls &&", true),
        ("a & b", false),
        ("echo ok; ", true),
        ("ls &\n   ", true),
        ("echo hi > f", true),
        ("echo hi >> f", true),
        ("echo hi > f | cat", false),
        ("cat f | tee > o", true),
        ("ls 2>&1", false),
        ("ls 2>&1 | head", false),
        ("echo hi >f", false),
        ("cat <<< x > y", true),
        ("echo a\necho b > c", true),
        ("echo a > c\necho b", true),
        ("cd /x", true),
        ("cd /x && ls", false),
        ("export A=1", true),
        ("export A=1; ls", false),
        ("source ~/.bashrc", true),
        (". ./env.sh", true),
        ("unset A", true),
        ("set -e", true),
        ("exec ls", true),
        ("read x", true),
        ("cd", true),
        ("cd /x\n  ", true),
        ("cd /x\nls", false),
        // The original missed this one: its rule 4 saw two first words, `""` and `cd`.
        ("\ncd /x", true),
        ("  cd /x", true),
        ("cdx /x", false),
        ("kill 1", true),
        ("wait", true),
        ("git diff", true),
        ("git   diff HEAD~1", true),
        ("diff a b", true),
        ("diffx a", false),
        ("git diffstat", false),
        ("echo hi\ndiff a b", true),
        ("git difftool", false),
        ("diffé a", false),
        ("cat f | diff - g", false),
        (" git diff", false),
        ("gh pr list --json number", true),
        ("tool --format=json", true),
        ("tool --format json", true),
        ("tool --output=\"json\"", true),
        ("tool -o json", true),
        ("tool -ojson", true),
        ("tool -o=json", true),
        ("tool -json", true),
        ("tool -json=a", true),
        ("tool --jsonx", false),
        ("go test -json ./...", true),
        ("tool --json", true),
        ("kubectl get x -o jsonpath={.a}", true),
        ("echo '{{json .}}'", true),
        ("echo {{ json .}}", true),
        ("echo \"--json\"", true),
        ("tool --json'x'", true),
        ("tool -ojsonl", true),
        ("cat <<EOF\nhi\nEOF", true),
        ("cat <<'EOF'\nhi\nEOF", true),
        ("cat <<< hi", false),
        ("cat <<<hi", false),
        ("a <<b", true),
        ("a<<<b <<c", true),
        ("x <<", true),
        ("set -o pipefail; false | true", true),
        ("echo pipefail", true),
        ("git status", false),
        ("ls", false),
        ("ls -la /tmp", false),
        ("python3 x.py", false),
        ("cat data.json", false),
        ("echo hello world", false),
        ("grep -rn foo .", false),
        ("make -j4", false),
        ("printf hello\n", false),
        ("printf hello\n\n\n", false),
        ("a\n\nb", false),
        ("a\n\nb\n", false),
        ("a\n\n\nb\n\n", false),
        ("echo a\r\necho b\r\n", false),
        ("ls\t", false),
        ("ls  \n", false),
        ("\n\nls", false),
        ("echo a\n\n\n   ", false),
        ("echo 你好世界", false),
        ("echo \"😀 emoji\" > out.txt", true),
        ("echo '日本語' | cat", false),
        ("grep 'it'\\''s' f", false),
        ("echo \"a\\\"b\"", false),
        ("echo a\\\\b", false),
        ("echo \u{3000}x", false),
        ("ls |\u{3000}etoon", true),
        ("echo \u{a0}> f", true),
        ("echo `date`", false),
        ("echo $(date) $HOME ${A:-b}", false),
        ("echo \u{7f}del", false),
        ("echo \u{1}ctl", false),
        ("cargo test -p x", false),
        ("cargo test -p x -- --nocapture", false),
        ("cargo test -p x 2>&1 | tail -5", false),
        ("cargo test -p a; cargo test -p b", false),
        ("cd d && cargo test", false),
        ("cargo nextest run -p x", false),
        ("cargo test -- --test-threads=2", false),
        ("RUST_TEST_THREADS=2 cargo test", false),
        ("cargo build", false),
        ("cargo test --no-run", false),
        ("echo cargo test", false),
        ("grep -rn \"cargo test\" docs", false),
        ("cargo test -p \"x", false),
        ("cargo test -p $(echo x)", false),
        ("car\\go test", false),
        ("c\"a\"rgo test", false),
        ("'cargo' test", false),
        ("bash -c \"cargo test\"", false),
        ("cargo test > out.txt", true),
        ("cargo test -p x &", true),
        ("set -o pipefail; cargo test | tail", true),
        ("cargo test <<EOF\nx\nEOF", true),
        ("cargo test --json", true),
        ("cargo test | etoon", true),
        ("cargo test\n\n", false),
        ("echo $'a\\nb'", false),
        ("bash -c $'cargo test'", false),
        ("sudo cargo test", false),
        ("cargo nextest run > o", true),
        ("cargo\ttest -p x", false),
        ("cargo test # note", false),
        ("ccargo test", false),
        ("cargoo test", false),
        ("cargo", false),
        ("exec cargo test", true),
        ("cd x && cargo test; echo done", false),
    ];

    #[test]
    fn test_skip_etoon_golden_corpus_matches_original() {
        let wrong: Vec<_> = GOLDEN
            .iter()
            .filter(|(cmd, want)| skip_etoon(cmd) != *want)
            .collect();
        assert!(wrong.is_empty(), "verdicts differ: {wrong:?}");
    }

    #[test]
    fn test_skip_etoon_builtin_receiving_pipe_returns_true() {
        for cmd in [
            "true && cd /",
            "ls; export A=1",
            "a || exit 3",
            "ls\ncd /x",
            "x & wait",
        ] {
            assert!(skip_etoon(cmd), "{cmd:?}");
        }
        for cmd in ["cd /x && ls", "true && cd / | cat", "cd /x\nls", "echo cd"] {
            assert!(!skip_etoon(cmd), "{cmd:?}");
        }
    }

    #[test]
    fn test_skip_etoon_diff_before_connector_or_joiner_returns_false() {
        for cmd in ["diff\u{203F}file", "diff\u{200D}x", "git diff_x"] {
            assert!(!skip_etoon(cmd), "{cmd:?}");
        }
    }

    #[test]
    fn test_skip_etoon_long_inputs_return_false() {
        assert!(!skip_etoon(&format!("echo {}", "a".repeat(100_000))));
        assert!(skip_etoon(&format!(
            "echo {} > out.txt",
            "é".repeat(20_000)
        )));
    }
}
