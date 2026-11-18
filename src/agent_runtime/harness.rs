//! How each harness runs one message headless, and where its final answer is.

use std::path::Path;

use crate::config::Harness;

pub struct Invocation {
    pub program: &'static str,
    pub args: Vec<String>,
}

/// The command for one headless session. `answer_file` is where Codex writes
/// its last message.
pub fn invocation(harness: Harness, model: &str, prompt: &str, answer_file: &Path) -> Invocation {
    let s = |v: &str| v.to_string();
    let (program, args) = match harness {
        Harness::ClaudeCode => (
            "claude",
            vec![
                s("-p"),
                s(prompt),
                s("--output-format"),
                s("json"),
                s("--model"),
                s(model),
                s("--dangerously-skip-permissions"),
            ],
        ),
        Harness::Codex => (
            "codex",
            vec![
                s("exec"),
                s("--dangerously-bypass-approvals-and-sandbox"),
                s("--skip-git-repo-check"),
                s("--model"),
                s(model),
                s("--output-last-message"),
                answer_file.display().to_string(),
                s(prompt),
            ],
        ),
        Harness::Pi => ("pi", vec![s("--model"), s(model), s("-p"), s(prompt)]),
        Harness::Antigravity => (
            "agy",
            vec![
                s("-p"),
                s(prompt),
                s("--model"),
                s(model),
                s("--dangerously-skip-permissions"),
            ],
        ),
    };
    Invocation { program, args }
}

/// The final answer from a session that exited successfully, or why there is none.
pub fn answer(harness: Harness, stdout: &str, answer_file: Option<&str>) -> Result<String, String> {
    match harness {
        Harness::ClaudeCode => {
            // `--output-format json` prints one result object.
            let result: serde_json::Value = stdout
                .lines()
                .rev()
                .find_map(|line| serde_json::from_str(line).ok())
                .ok_or_else(|| "claude printed no JSON result".to_string())?;
            let text = result.get("result").and_then(serde_json::Value::as_str);
            if result
                .get("is_error")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
            {
                let errors = result.get("errors").map(ToString::to_string);
                return Err(text
                    .map(str::to_string)
                    .or(errors)
                    .unwrap_or_else(|| result.to_string()));
            }
            text.map(str::to_string)
                .ok_or_else(|| format!("claude returned no result: {result}"))
        }
        Harness::Codex => match answer_file.map(str::trim) {
            Some(text) if !text.is_empty() => Ok(text.to_string()),
            _ => Err("codex wrote no final message".to_string()),
        },
        Harness::Pi | Harness::Antigravity => match stdout.trim() {
            "" => Err(format!("{} printed no answer", harness.as_str())),
            text => Ok(text.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_result_and_errors() {
        let ok = r#"{"type":"result","subtype":"success","is_error":false,"result":"pong","modelUsage":{}}"#;
        assert_eq!(answer(Harness::ClaudeCode, ok, None), Ok("pong".into()));
        let bad = r#"{"type":"result","subtype":"success","is_error":true,"result":"API Error: model not found"}"#;
        assert_eq!(
            answer(Harness::ClaudeCode, bad, None),
            Err("API Error: model not found".into())
        );
        let max = r#"{"type":"result","subtype":"error_max_turns","is_error":true,"errors":["max turns"]}"#;
        assert_eq!(
            answer(Harness::ClaudeCode, max, None),
            Err(r#"["max turns"]"#.into())
        );
        assert!(answer(Harness::ClaudeCode, "garbage", None).is_err());
    }

    #[test]
    fn codex_reads_its_answer_file() {
        assert_eq!(
            answer(Harness::Codex, "noise", Some("pong\n")),
            Ok("pong".into())
        );
        assert!(answer(Harness::Codex, "", Some(" ")).is_err());
        assert!(answer(Harness::Codex, "", None).is_err());
    }

    #[test]
    fn stdout_harnesses_need_an_answer() {
        for harness in [Harness::Pi, Harness::Antigravity] {
            assert_eq!(answer(harness, " pong\n", None), Ok("pong".into()));
            assert!(answer(harness, " \n", None).is_err());
        }
    }

    #[test]
    fn every_run_gets_the_model() {
        for harness in Harness::ALL {
            let inv = invocation(harness, "m-1", "[Rin] hi", Path::new("/tmp/a"));
            let at = inv
                .args
                .iter()
                .position(|a| a == "--model")
                .expect("--model");
            assert_eq!(inv.args[at + 1], "m-1");
            assert!(inv.args.iter().any(|a| a == "[Rin] hi"));
        }
    }
}
