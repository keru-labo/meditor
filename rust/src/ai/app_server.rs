//! The versioned Codex App Server stdio protocol for one isolated text turn.

use std::{
    env,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};

const CODEX_VERSION: &str = "0.171.0";
const CHATGPT_BASE_URL: &str = "https://chatgpt.com/backend-api/";
const DEADLINE: Duration = Duration::from_secs(60);
const CANCEL_POLL: Duration = Duration::from_millis(50);
// The server can echo the input as a userMessage item, with JSON escaping.
const LINE_LIMIT: u64 = 8 * 1024 * 1024;
const RESULT_LIMIT: usize = 128 * 1024;
const DISABLED_FEATURES: &[&str] = &[
    "apps",
    "auth_elicitation",
    "browser_use",
    "hooks",
    "in_app_browser",
    "image_generation",
    "memories",
    "multi_agent_v2",
    "plugins",
    "remote_plugin",
    "shell_snapshot",
    "shell_tool",
    "skill_mcp_dependency_install",
    "skill_search",
    "sleep_tool",
    "tool_suggest",
    "view_image",
];
const ENABLED_FEATURES: &[&str] = &["omit_skill_descriptions", "skip_host_skill_discovery"];

pub(super) fn select(
    input: &str,
    instructions: &str,
    schema: Value,
    cancelled: &AtomicBool,
) -> Result<String> {
    ensure!(
        !cancelled.load(Ordering::Relaxed),
        "Codex の処理を中断しました"
    );
    let home = codex_home()?;
    let cwd = tempfile::Builder::new()
        .prefix("selection-")
        .tempdir_in(&home)
        .context("Codex の作業フォルダーを作成できませんでした")?;
    let executable = env::var_os("PATH")
        .into_iter()
        .flat_map(|path| env::split_paths(&path).collect::<Vec<_>>())
        .filter(|path| path.is_absolute())
        .map(|path| path.join(if cfg!(windows) { "codex.exe" } else { "codex" }))
        .find(|path| path.is_file())
        .context("Codex CLI の実行ファイルが PATH に見つかりません")?;
    let mut command = Command::new(executable);
    command
        .env_clear()
        .env("CODEX_HOME", &home)
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("TMPDIR", cwd.path())
        .env("TMP", cwd.path())
        .env("TEMP", cwd.path())
        .current_dir(cwd.path());
    for name in ["SystemRoot", "WINDIR", "SystemDrive"] {
        if let Some(value) = env::var_os(name) {
            command.env(name, value);
        }
    }
    // Launch the native executable directly and keep its console out of the desktop UI.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    for flag in DISABLED_FEATURES {
        command.args(["--disable", flag]);
    }
    for flag in ENABLED_FEATURES {
        command.args(["--enable", flag]);
    }
    command.args([
        "-c",
        "web_search=\"disabled\"",
        "-c",
        "project_doc_max_bytes=0",
        "-c",
        "agents.enabled=false",
        "-c",
        "analytics.enabled=false",
        "-c",
        "notify=[]",
        "-c",
        "history.persistence=\"none\"",
        "-c",
        "otel.log_user_prompt=false",
        "-c",
        "otel.log_agent_responses=false",
        "-c",
        "otel.log_sse_deltas=false",
        "-c",
        "otel.exporter=\"none\"",
        "-c",
        "otel.trace_exporter=\"none\"",
        "-c",
        "otel.metrics_exporter=\"none\"",
        "-c",
        "cli_auth_credentials_store=\"file\"",
        "-c",
        "model_provider=\"openai\"",
        "-c",
        &format!("chatgpt_base_url=\"{CHATGPT_BASE_URL}\""),
        "-c",
        "openai_base_url=\"\"",
        "app-server",
    ]);
    let (_process, rpc) = start(command, cancelled)?;
    rpc.select(cwd.path(), input, instructions, schema)
}

fn codex_home() -> Result<PathBuf> {
    let home = env::var_os("MEDITOR_CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("LOCALAPPDATA")
                .map(|path| PathBuf::from(path).join("meditor").join("codex"))
        })
        .context("meditor 専用の Codex フォルダーを設定してください")?;
    ensure!(
        home.is_absolute(),
        "meditor 専用の Codex フォルダーには絶対パスを指定してください"
    );
    std::fs::create_dir_all(&home)
        .context("meditor 専用の Codex フォルダーを作成できませんでした")?;
    let canonical = home
        .canonicalize()
        .context("Codex の作業フォルダーを確認できませんでした")?;
    let default_home = env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(|path| PathBuf::from(path).join(".codex"));
    for shared in [env::var_os("CODEX_HOME").map(PathBuf::from), default_home]
        .into_iter()
        .flatten()
    {
        ensure!(
            shared.canonicalize().ok().as_ref() != Some(&canonical),
            "通常の Codex フォルダーと異なる専用フォルダーを指定してください"
        );
    }
    ensure!(
        matches!(home.join("config.toml").symlink_metadata(), Err(error) if error.kind() == std::io::ErrorKind::NotFound),
        "meditor 専用の Codex フォルダーに config.toml があります"
    );
    Ok(home)
}

fn same_directory(reported: &Path, expected: &Path) -> bool {
    // Windows can report the same directory using a different path prefix.
    reported == expected
        || (reported.is_absolute()
            && reported
                .canonicalize()
                .ok()
                .is_some_and(|path| expected.canonicalize().ok().as_ref() == Some(&path)))
}

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Rpc<'a> {
    send: SyncSender<Value>,
    receive: Receiver<Value>,
    deadline: Instant,
    cancelled: &'a AtomicBool,
}

fn start(mut command: Command, cancelled: &AtomicBool) -> Result<(Server, Rpc<'_>)> {
    let mut server = Server(
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| anyhow::anyhow!("Codex App Server を起動できませんでした"))?,
    );
    let mut stdin = server
        .0
        .stdin
        .take()
        .context("Codex の入力を開けませんでした")?;
    let mut stdout = BufReader::new(
        server
            .0
            .stdout
            .take()
            .context("Codex の出力を開けませんでした")?,
    );
    let (send, writing) = mpsc::sync_channel(2);
    let (reading, receive) = mpsc::sync_channel(2);
    thread::Builder::new()
        .spawn(move || {
            for request in writing {
                if serde_json::to_writer(&mut stdin, &request).is_err()
                    || stdin.write_all(b"\n").is_err()
                    || stdin.flush().is_err()
                {
                    break;
                }
            }
        })
        .context("Codex の入力スレッドを開始できませんでした")?;
    thread::Builder::new()
        .spawn(move || {
            let mut line = Vec::new();
            loop {
                line.clear();
                if stdout
                    .by_ref()
                    .take(LINE_LIMIT)
                    .read_until(b'\n', &mut line)
                    .is_err()
                    || line.last() != Some(&b'\n')
                {
                    break;
                }
                let Ok(message) = serde_json::from_slice(&line) else {
                    break;
                };
                if reading.send(message).is_err() {
                    break;
                }
            }
        })
        .context("Codex の出力スレッドを開始できませんでした")?;
    Ok((
        server,
        Rpc {
            send,
            receive,
            deadline: Instant::now() + DEADLINE,
            cancelled,
        },
    ))
}

impl Rpc<'_> {
    fn send(&self, value: Value) -> Result<()> {
        self.check_cancelled()?;
        self.send
            .try_send(value)
            .map_err(|_| anyhow::anyhow!("Codex App Server に依頼を送れませんでした"))
    }

    fn check_cancelled(&self) -> Result<()> {
        ensure!(
            !self.cancelled.load(Ordering::Relaxed),
            "Codex の処理を中断しました"
        );
        Ok(())
    }

    fn receive(&self) -> Result<Value> {
        let message = loop {
            self.check_cancelled()?;
            let remaining = self.deadline.saturating_duration_since(Instant::now());
            ensure!(!remaining.is_zero(), "Codex の応答がタイムアウトしました");
            match self.receive.recv_timeout(remaining.min(CANCEL_POLL)) {
                Ok(message) => break message,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    bail!("Codex App Server の接続が終了しました");
                }
            }
        };
        ensure!(
            message.get("method").is_none() || message.get("id").is_none(),
            "Codex が未対応の操作を要求しました"
        );
        if matches!(
            message["method"].as_str(),
            Some("item/started" | "item/completed")
        ) {
            ensure!(
                matches!(
                    message["params"]["item"]["type"].as_str(),
                    Some("userMessage" | "agentMessage" | "reasoning" | "plan")
                ),
                "Codex が未対応の操作を開始しました"
            );
        }
        Ok(message)
    }

    fn response(&self, id: u64, event: impl FnMut(&Value) -> Result<()>) -> Result<Value> {
        let mut event = event;
        loop {
            let message = self.receive()?;
            if let Some(response_id) = message.get("id") {
                ensure!(
                    response_id == id
                        && message.get("error").is_none()
                        && message["result"].is_object(),
                    "Codex App Server が依頼を受け付けませんでした"
                );
                return Ok(message["result"].clone());
            }
            event(&message)?;
        }
    }

    fn request(&self, id: u64, method: &str, params: Value) -> Result<Value> {
        self.send(json!({"id": id, "method": method, "params": params}))?;
        self.response(id, |_| Ok(()))
    }

    fn select(&self, cwd: &Path, input: &str, instructions: &str, schema: Value) -> Result<String> {
        let initialized = self.request(
            1,
            "initialize",
            json!({
                "clientInfo": {"name": "meditor", "version": env!("CARGO_PKG_VERSION")},
                "capabilities": {"experimentalApi": true}
            }),
        )?;
        ensure!(
            initialized["userAgent"]
                .as_str()
                .and_then(|agent| agent.split_whitespace().next())
                == Some(format!("meditor/{CODEX_VERSION}").as_str()),
            "この機能には Codex CLI {} が必要です",
            CODEX_VERSION
        );
        self.send(json!({"method": "initialized"}))?;
        let config = self.request(2, "config/read", json!({"includeLayers": false}))?;
        let config = &config["config"];
        ensure!(
            config["mcp_servers"]
                .as_object()
                .is_some_and(|servers| servers.is_empty())
                && config["web_search"] == "disabled"
                && config["model_provider"] == "openai"
                && config["model_providers"]
                    .as_object()
                    .is_some_and(|providers| providers.is_empty())
                && config["otel"]["headers_helper"].is_null()
                && config["chatgpt_base_url"] == CHATGPT_BASE_URL
                && config["openai_base_url"] == ""
                && config["notify"]
                    .as_array()
                    .is_some_and(|commands| commands.is_empty())
                && config["history"]["persistence"] == "none"
                && config["analytics"]["enabled"] == false
                && ["exporter", "trace_exporter", "metrics_exporter"]
                    .iter()
                    .all(|name| config["otel"][name] == "none")
                && ["log_user_prompt", "log_agent_responses", "log_sse_deltas"]
                    .iter()
                    .all(|name| config["otel"][name] == false)
                && config["agents"]["enabled"] == false
                && DISABLED_FEATURES
                    .iter()
                    .all(|name| config["features"][name] == false)
                && ENABLED_FEATURES
                    .iter()
                    .all(|name| config["features"][name] == true),
            "Codex のツールと接続先の設定を確認できませんでした"
        );
        let account = self.request(3, "account/read", json!({"refreshToken": false}))?;
        ensure!(
            account["account"]["type"] == "chatgpt",
            "meditor 専用の Codex フォルダーで codex login を実行してください"
        );
        let thread = self.request(
            4,
            "thread/start",
            json!({
                "cwd": cwd,
                "approvalPolicy": "never",
                "sandbox": "read-only",
                "ephemeral": true,
                "environments": [],
                "dynamicTools": [],
                "modelProvider": "openai",
                "baseInstructions": instructions,
                "developerInstructions": "Return only the requested selection."
            }),
        )?;
        ensure!(
            thread["approvalPolicy"] == "never"
                && thread["sandbox"] == json!({"type": "readOnly", "networkAccess": false})
                && thread["thread"]["ephemeral"] == true
                && thread["thread"]["environments"]
                    .as_array()
                    .is_some_and(|environments| environments.is_empty())
                && thread["thread"]["gitInfo"].is_null()
                && thread["runtimeWorkspaceRoots"]
                    .as_array()
                    .is_some_and(|roots| roots.is_empty())
                && thread["modelProvider"] == "openai"
                && thread["cwd"]
                    .as_str()
                    .is_some_and(|reported| same_directory(Path::new(reported), cwd))
                && thread["instructionSources"]
                    .as_array()
                    .is_some_and(|sources| sources.is_empty()),
            "Codex の実行制限を確認できませんでした"
        );
        let thread_id = thread["thread"]["id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .context("Codex のスレッドを確認できませんでした")?;
        self.send(json!({"id": 5, "method": "turn/start", "params": {
            "threadId": thread_id,
            "input": [{"type": "text", "text": input, "text_elements": []}],
            "environments": [],
            "outputSchema": schema
        }}))?;
        let mut early = Vec::new();
        let started = self.response(5, |message| {
            if message["params"]["threadId"] == thread_id
                && matches!(
                    message["method"].as_str(),
                    Some("item/completed" | "turn/completed")
                )
            {
                ensure!(early.len() < 8, "Codex の初期応答が多すぎます");
                early.push(message.clone());
            }
            Ok(())
        })?;
        let turn_id = started["turn"]["id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .context("Codex の処理を確認できませんでした")?;
        let mut early = early.into_iter();
        let mut output = None;
        loop {
            let message = match early.next() {
                Some(message) => message,
                None => self.receive()?,
            };
            ensure!(
                message.get("id").is_none(),
                "Codex から予期しない応答を受け取りました"
            );
            let params = &message["params"];
            if params["threadId"] != thread_id {
                continue;
            }
            let item = &params["item"];
            match message["method"].as_str() {
                Some("item/completed")
                    if params["turnId"] == turn_id
                        && item["type"] == "agentMessage"
                        && (item["phase"].is_null() || item["phase"] == "final_answer") =>
                {
                    let text = item["text"]
                        .as_str()
                        .filter(|text| text.len() <= RESULT_LIMIT);
                    ensure!(
                        output.is_none() && text.is_some(),
                        "Codex の区間選択結果を確認できませんでした"
                    );
                    output = text.map(str::to_owned);
                }
                Some("turn/completed") if params["turn"]["id"] == turn_id => {
                    if params["turn"]["status"] != "completed" {
                        bail!("Codex の処理が完了しませんでした");
                    }
                    return output.context("Codex の応答に区間選択結果がありません");
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(cwd: &Path) -> Vec<Value> {
        let mut features = serde_json::Map::new();
        for name in DISABLED_FEATURES {
            features.insert((*name).into(), json!(false));
        }
        for name in ENABLED_FEATURES {
            features.insert((*name).into(), json!(true));
        }
        vec![
            json!({"id": 1, "result": {"userAgent": format!("meditor/{CODEX_VERSION} (Windows)")}}),
            json!({"id": 2, "result": {"config": {
                "mcp_servers": {},
                "web_search": "disabled",
                "model_provider": "openai",
                "model_providers": {},
                "chatgpt_base_url": CHATGPT_BASE_URL,
                "openai_base_url": "",
                "notify": [],
                "history": {"persistence": "none"},
                "analytics": {"enabled": false},
                "otel": {
                    "exporter": "none", "trace_exporter": "none", "metrics_exporter": "none",
                    "log_user_prompt": false, "log_agent_responses": false, "log_sse_deltas": false
                },
                "agents": {"enabled": false},
                "features": features
            }}}),
            json!({"id": 3, "result": {"account": {"type":"chatgpt","email":"synthetic private text"}}}),
            json!({"id": 4, "result": {
                "cwd": cwd, "approvalPolicy": "never",
                "sandbox": {"type":"readOnly","networkAccess":false},
                "instructionSources": [], "modelProvider": "openai",
                "runtimeWorkspaceRoots": [],
                "thread":{"id":"thread-1","ephemeral":true,"environments":[]}
            }}),
        ]
    }

    fn run(cwd: &Path, responses: Vec<Value>) -> (Result<String>, Vec<Value>) {
        let (send, sent) = mpsc::sync_channel(8);
        let (feed, receive) = mpsc::sync_channel(responses.len());
        for response in responses {
            feed.send(response).unwrap();
        }
        drop(feed);
        let cancelled = AtomicBool::new(false);
        let rpc = Rpc {
            send,
            receive,
            deadline: Instant::now() + DEADLINE,
            cancelled: &cancelled,
        };
        let result = rpc.select(
            cwd,
            "synthetic request data",
            "Instructions",
            super::super::schema(),
        );
        (result, sent.try_iter().collect())
    }

    #[test]
    fn only_a_completed_result_for_the_requested_turn_is_applied() {
        let cwd = tempfile::tempdir().unwrap();
        let mut responses = setup(cwd.path());
        for (thread_id, phase) in [
            ("unrelated-thread", "final_answer"),
            ("thread-1", "commentary"),
            ("thread-1", "final_answer"),
        ] {
            responses.push(json!({"method":"item/completed","params":{
                "threadId":thread_id,"turnId":"turn-1",
                "item":{"type":"agentMessage","phase":phase,"text":"{\"clip\":null}"}
            }}));
        }
        responses.push(json!({"id": 5, "result": {"turn": {"id":"turn-1"}}}));
        responses.push(json!({"method":"turn/completed","params":{
            "threadId":"thread-1","turn":{"id":"turn-1","status":"completed"}
        }}));
        let (result, sent) = run(cwd.path(), responses);
        assert_eq!(result.unwrap(), r#"{"clip":null}"#);
        assert_eq!(
            sent.iter()
                .map(|value| value["method"].as_str().unwrap())
                .collect::<Vec<_>>(),
            [
                "initialize",
                "initialized",
                "config/read",
                "account/read",
                "thread/start",
                "turn/start"
            ]
        );
        assert!(
            sent[..5]
                .iter()
                .all(|message| !message.to_string().contains("synthetic request data"))
        );
        assert_eq!(sent[4]["params"]["ephemeral"], true);
        assert_eq!(sent[4]["params"]["environments"], json!([]));
        assert_eq!(sent[5]["params"]["environments"], json!([]));
        assert_eq!(
            sent[5]["params"]["input"],
            json!([{
                "type": "text", "text": "synthetic request data", "text_elements": []
            }])
        );
        assert_eq!(sent[5]["params"]["outputSchema"], super::super::schema());
    }

    #[test]
    fn refuses_to_send_project_data_if_startup_restrictions_are_unsatisfied() {
        let cwd = tempfile::tempdir().unwrap();
        for (response, pointer, replacement) in [
            (0, "/result/userAgent", json!("unexpected-version")),
            (
                1,
                "/result/config/mcp_servers",
                json!({"synthetic-private":"configured"}),
            ),
            (1, "/result/config/features/shell_tool", json!(true)),
            (
                1,
                "/result/config/chatgpt_base_url",
                json!("synthetic-private"),
            ),
            (2, "/result/account/type", json!("apiKey")),
            (3, "/result/sandbox/networkAccess", json!(true)),
            (
                3,
                "/result/instructionSources",
                json!(["synthetic-private"]),
            ),
        ] {
            let mut responses = setup(cwd.path());
            *responses[response].pointer_mut(pointer).unwrap() = replacement;
            let (result, sent) = run(cwd.path(), responses);
            assert!(
                !result
                    .unwrap_err()
                    .to_string()
                    .contains("synthetic-private")
            );
            assert!(sent.iter().all(|value| value["method"] != "turn/start"));
        }
    }

    #[test]
    fn refuses_tool_requests_and_never_sends_an_approval() {
        let cwd = tempfile::tempdir().unwrap();
        let mut responses = setup(cwd.path());
        responses.push(json!({"id": 5, "result":{"turn":{"id":"turn-1"}}}));
        responses
            .push(json!({"method":"item/commandExecution/requestApproval","id":99,"params":{}}));
        let (result, sent) = run(cwd.path(), responses);
        assert!(result.is_err());
        assert_eq!(sent.len(), 6);
    }
}
