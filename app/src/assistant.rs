//! The assistant: an AI already installed on this Mac, driven headless, looking at the scan
//! through Tidy's MCP server (`tidy mcp <snapshot>`, see `tidy_core::mcp`).
//!
//! Claude Code runs as one long-lived `claude -p` process per conversation, with stream-json on
//! stdin and stdout, Tidy's MCP tools as its only tools, and the person's own login. Codex
//! (`codex exec --json`, resumed by thread id) and Gemini CLI (`gemini -p -o stream-json`,
//! resumed in its own folder) run one process per turn, with the same tools and the same events.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};

use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderKind {
    ClaudeCode,
    Codex,
    Gemini,
}

impl ProviderKind {
    pub const ALL: [ProviderKind; 3] = [ProviderKind::ClaudeCode, ProviderKind::Codex, ProviderKind::Gemini];

    /// The name settings store it under.
    pub fn id(self) -> &'static str {
        match self {
            ProviderKind::ClaudeCode => "claude_code",
            ProviderKind::Codex => "codex",
            ProviderKind::Gemini => "gemini",
        }
    }

    /// Model names this CLI takes, as hints for the model field.
    pub fn model_hint(self) -> &'static str {
        match self {
            ProviderKind::ClaudeCode => "opus, sonnet, haiku…",
            ProviderKind::Codex => "Codex's model name",
            ProviderKind::Gemini => "gemini-… model name",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ProviderKind::ClaudeCode => "Claude Code",
            ProviderKind::Codex => "Codex",
            ProviderKind::Gemini => "Gemini CLI",
        }
    }
    fn binary(self) -> &'static str {
        match self {
            ProviderKind::ClaudeCode => "claude",
            ProviderKind::Codex => "codex",
            ProviderKind::Gemini => "gemini",
        }
    }
    pub fn supported(self) -> bool {
        true
    }
}

#[derive(Clone, Debug)]
pub struct Provider {
    pub kind: ProviderKind,
    pub path: PathBuf,
}

/// What the found CLIs need to run: the login shell's PATH (a Finder-launched app gets a bare one).
#[derive(Clone, Debug, Default)]
pub struct Detected {
    pub providers: Vec<Provider>,
    pub path_env: Option<String>,
}

/// Looks for installed AI CLIs. Slow-ish (starts a login shell): call it off the UI thread.
pub fn detect() -> Detected {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let path_env = Command::new(&shell)
        .args(["-lc", "printf %s \"$PATH\""])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .filter(|p| !p.is_empty());
    let mut dirs: Vec<PathBuf> = path_env.as_deref().unwrap_or("").split(':').filter(|d| !d.is_empty()).map(PathBuf::from).collect();
    for extra in [".local/bin", ".claude/local", ".npm-global/bin", ".bun/bin"] {
        dirs.push(home.join(extra));
    }
    dirs.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    let mut providers = Vec::new();
    for kind in [ProviderKind::ClaudeCode, ProviderKind::Codex, ProviderKind::Gemini] {
        if let Some(p) = dirs.iter().map(|d| d.join(kind.binary())).find(|p| p.is_file()) {
            providers.push(Provider { kind, path: p });
        }
    }
    Detected { providers, path_env }
}

/// What the conversation hears from the process.
#[derive(Debug)]
pub enum Event {
    Ready { model: String },
    /// Text streaming into the current assistant message.
    Delta(String),
    /// A block finished: the whole text of an assistant text block.
    Text(String),
    /// The assistant called one of Tidy's tools.
    Tool { name: String, input: Value },
    TurnDone { cost_usd: f64, error: Option<String> },
    Exited(String),
}

pub struct Session {
    kind: ProviderKind,
    /// The model to ask for, when the person chose one (else the CLI's own default).
    model: Option<String>,
    provider: Provider,
    path_env: Option<String>,
    /// Claude Code: the conversation's process and its stdin.
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    /// Codex: the thread to resume; Gemini: whether a session exists in its folder yet.
    thread: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    turns: usize,
    tx: Sender<Event>,
    waker: rune::Waker,
    pub events: Receiver<Event>,
}

const SYSTEM: &str = "You are Tidy's assistant, inside a native Mac app that shows where disk space goes. \
You can't read file contents or run commands: your only window on the disk is Tidy's tools, which read the \
scan Tidy has already made (paths, sizes, dates, kinds). The disk is already scanned: never say you'll scan it, \
and don't ask the person to; say you're looking at Tidy's scan. Tool calls answer instantly. \
Start from `overview`, drill with `list_folder` and `find_files`. When you find things worth removing, call \
`propose_cleanup` with exact paths from tool results: it shows a checklist in the app and the person decides; \
nothing is ever deleted by you. You can steer the window: `show_in_tidy` zooms the map to a folder or file you're \
talking about, `open_view` opens a view (duplicates, developer, changes…); use them when it helps the person see it. \
For developer leftovers (worktrees, simulators, build output) use `developer_findings` and point to the Developer view. Prefer things that rebuild themselves (caches, build output, old installers) and \
mark anything personal (photos, documents, projects, mail) as `review`, explaining what it is. Never propose the \
system's own folders, app bundles in /Applications, or anything you aren't sure about. \
Keep replies short and concrete, with sizes, in the language the person writes in. Use Markdown sparingly: \
short lists, bold for the key number.";

impl Session {
    /// Starts a conversation with `provider`, its tools reading `snapshot`.
    pub fn start(provider: &Provider, model: Option<String>, path_env: Option<&str>, snapshot: &Path, waker: rune::Waker) -> Result<Session, String> {
        let model = model.filter(|m| !m.trim().is_empty()).map(|m| m.trim().to_owned());
        match provider.kind {
            ProviderKind::ClaudeCode => Self::start_claude(provider, model, path_env, snapshot, waker),
            _ => {
                let (tx, rx) = channel();
                Ok(Session {
                    kind: provider.kind,
                    model,
                    provider: provider.clone(),
                    path_env: path_env.map(str::to_owned),
                    child: None,
                    stdin: None,
                    thread: Default::default(),
                    turns: 0,
                    tx,
                    waker,
                    events: rx,
                })
            }
        }
    }

    /// Starts Claude Code with Tidy's MCP server reading `snapshot`.
    fn start_claude(provider: &Provider, model: Option<String>, path_env: Option<&str>, snapshot: &Path, waker: rune::Waker) -> Result<Session, String> {
        let me = mcp_command()?;
        let support = support_dir();
        let workdir = support.join("assistant");
        std::fs::create_dir_all(&workdir).map_err(|e| e.to_string())?;
        let config = json!({"mcpServers": {tidy_core::mcp::SERVER_NAME: {
            "type": "stdio",
            "command": me,
            "args": ["mcp", snapshot],
        }}});
        let config_path = support.join("mcp.json");
        std::fs::write(&config_path, config.to_string()).map_err(|e| e.to_string())?;
        let allowed = tidy_core::mcp::qualified_tools().join(",");
        let mut cmd = Command::new(&provider.path);
        cmd.args(["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--include-partial-messages"])
            .arg("--mcp-config")
            .arg(&config_path)
            .args(["--strict-mcp-config", "--tools", "", "--allowedTools", &allowed, "--no-session-persistence"])
            .args(["--system-prompt", SYSTEM])
            .args(model.iter().flat_map(|m| ["--model", m.as_str()]))
            .current_dir(&workdir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(p) = path_env {
            cmd.env("PATH", p);
        }
        let mut child = cmd.spawn().map_err(|e| format!("Couldn't start {}: {e}", provider.path.display()))?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let stderr = child.stderr.take();
        let (tx, rx) = channel();
        let (tx2, w2) = (tx.clone(), waker.clone());
        std::thread::spawn(move || read_events(stdout, stderr, tx2, w2, parse));
        Ok(Session {
            kind: ProviderKind::ClaudeCode,
            model,
            provider: provider.clone(),
            path_env: path_env.map(str::to_owned),
            child: Some(child),
            stdin: Some(stdin),
            thread: Default::default(),
            turns: 0,
            tx,
            waker,
            events: rx,
        })
    }

    pub fn send(&mut self, text: &str) -> Result<(), String> {
        self.turns += 1;
        match self.kind {
            ProviderKind::ClaudeCode => {
                let stdin = self.stdin.as_mut().ok_or("The assistant stopped.")?;
                let msg = json!({"type": "user", "message": {"role": "user", "content": text}});
                writeln!(stdin, "{msg}").and_then(|_| stdin.flush()).map_err(|e| e.to_string())
            }
            ProviderKind::Codex => self.codex_turn(text),
            ProviderKind::Gemini => self.gemini_turn(text),
        }
    }

    /// The first turn carries Tidy's instructions: these CLIs take no system prompt from us.
    fn with_instructions(&self, text: &str) -> String {
        if self.turns == 1 { format!("<instructions>\n{SYSTEM}\n</instructions>\n\n{text}") } else { text.to_owned() }
    }

    fn spawn_turn(&mut self, mut cmd: Command, parse: fn(&Value, &std::sync::Mutex<Option<String>>) -> Vec<Event>) -> Result<(), String> {
        if let Some(p) = &self.path_env {
            cmd.env("PATH", p);
        }
        cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| format!("Couldn't start {}: {e}", self.provider.path.display()))?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let stderr = child.stderr.take();
        let (tx, waker, thread) = (self.tx.clone(), self.waker.clone(), self.thread.clone());
        std::thread::spawn(move || {
            let mut done = false;
            let mut said = false;
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                // Gemini asks to sign in on stdout when its login has expired.
                if line.contains("authentication page") || line.contains("Please sign in") {
                    let _ = tx.send(Event::TurnDone { cost_usd: 0.0, error: Some("Sign in to this assistant first: run it once in Terminal, then try again.".into()) });
                    waker();
                    let _ = child.kill();
                    return;
                }
                let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
                for e in parse(&v, &thread) {
                    said |= matches!(e, Event::Text(_) | Event::Delta(_));
                    done |= matches!(e, Event::TurnDone { .. });
                    let _ = tx.send(e);
                    waker();
                }
            }
            let status = child.wait();
            if !done {
                let mut err = String::new();
                if let Some(mut s) = stderr {
                    use std::io::Read;
                    let _ = s.read_to_string(&mut err);
                }
                let failed = status.map(|s| !s.success()).unwrap_or(true);
                let why = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("The assistant stopped.").to_owned();
                let _ = tx.send(Event::TurnDone { cost_usd: 0.0, error: (failed || !said).then_some(why) });
            }
            waker();
        });
        Ok(())
    }

    fn workdir(name: &str) -> Result<PathBuf, String> {
        let d = support_dir().join(name);
        std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
        Ok(d)
    }

    fn codex_turn(&mut self, text: &str) -> Result<(), String> {
        let me = mcp_command()?;
        let snap = snapshot_path();
        let mut cmd = Command::new(&self.provider.path);
        cmd.current_dir(Self::workdir("assistant-codex")?);
        cmd.args(["exec", "--json", "--skip-git-repo-check", "-s", "read-only"]);
        if let Some(m) = &self.model {
            cmd.args(["-m", m]);
        }
        cmd
            .args(["-c", "approval_policy=\"never\""])
            .args(["-c", &format!("mcp_servers.{}.command={:?}", tidy_core::mcp::SERVER_NAME, me.to_string_lossy())])
            .args(["-c", &format!("mcp_servers.{}.args=[\"mcp\", {:?}]", tidy_core::mcp::SERVER_NAME, snap.to_string_lossy())])
            .args(["-c", &format!("mcp_servers.{}.default_tools_approval_mode=\"approve\"", tidy_core::mcp::SERVER_NAME)]);
        // Only Tidy's tools: the person's other MCP servers, plugins (computer use, browser…)
        // and turn hooks stay off for this conversation. Their config file isn't touched.
        cmd.args(["--disable", "computer_use", "--disable", "apps", "-c", "notify=[]"]);
        for over in codex_isolation() {
            cmd.args(["-c", &over]);
        }
        let prompt = self.with_instructions(text);
        match self.thread.lock().unwrap().clone() {
            Some(id) => {
                cmd.args(["resume", &id, &prompt]);
            }
            None => {
                cmd.arg(&prompt);
            }
        }
        self.spawn_turn(cmd, parse_codex)
    }

    fn gemini_turn(&mut self, text: &str) -> Result<(), String> {
        let me = mcp_command()?;
        let dir = Self::workdir("assistant-gemini")?;
        // Tidy's tools trusted; Gemini's own file, shell and web tools off.
        let off = ["run_shell_command", "write_file", "replace", "web_fetch", "google_web_search", "read_file", "read_many_files", "list_directory", "glob", "search_file_content", "save_memory"];
        let settings = json!({
            "mcpServers": {tidy_core::mcp::SERVER_NAME: {"command": me, "args": ["mcp", snapshot_path()], "trust": true}},
            "excludeTools": off,
            "tools": {"exclude": off},
        });
        std::fs::create_dir_all(dir.join(".gemini")).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(".gemini/settings.json"), settings.to_string()).map_err(|e| e.to_string())?;
        let mut cmd = Command::new(&self.provider.path);
        cmd.current_dir(&dir);
        // Only Tidy's server, and none of the person's extensions.
        if let Some(m) = &self.model {
            cmd.args(["-m", m]);
        }
        cmd.args(["-o", "stream-json", "--skip-trust", "--allowed-mcp-server-names", tidy_core::mcp::SERVER_NAME, "--extensions", "none"]);
        if self.turns > 1 {
            cmd.args(["--resume", "latest"]);
        }
        cmd.args(["-p", &self.with_instructions(text)]);
        self.spawn_turn(cmd, parse_gemini)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(c) = &mut self.child {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// The program the assistant runs as Tidy's MCP server: this app (`tidy mcp`). Tests point
/// it at a built app with `TIDY_MCP_BIN`, since their own executable is the test harness.
fn mcp_command() -> Result<PathBuf, String> {
    match std::env::var_os("TIDY_MCP_BIN") {
        Some(p) => Ok(PathBuf::from(p)),
        None => std::env::current_exe().map_err(|e| e.to_string()),
    }
}

/// `-c` overrides turning off every MCP server and plugin in the person's Codex config.
fn codex_isolation() -> Vec<String> {
    let home = std::env::var_os("CODEX_HOME").map(PathBuf::from).unwrap_or_else(|| std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join(".codex"));
    let Ok(cfg) = std::fs::read_to_string(home.join("config.toml")) else { return Vec::new() };
    codex_overrides(&cfg)
}

pub fn codex_overrides(cfg: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in cfg.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("[mcp_servers.").and_then(|r| r.strip_suffix(']')) {
            // A server's own table, not its `.env` or `.tools` sub-tables.
            if !rest.contains('.') && rest != tidy_core::mcp::SERVER_NAME {
                out.push(format!("mcp_servers.{rest}.enabled=false"));
            }
        } else if let Some(rest) = l.strip_prefix("[plugins.").and_then(|r| r.strip_suffix(']')) {
            out.push(format!("plugins.{rest}.enabled=false"));
        }
    }
    out
}

/// Codex `exec --json` events.
pub fn parse_codex(v: &Value, thread: &std::sync::Mutex<Option<String>>) -> Vec<Event> {
    let item = &v["item"];
    match v["type"].as_str() {
        Some("thread.started") => {
            *thread.lock().unwrap() = v["thread_id"].as_str().map(str::to_owned);
            vec![Event::Ready { model: String::new() }]
        }
        Some("item.completed") if item["type"] == "agent_message" => vec![Event::Text(item["text"].as_str().unwrap_or("").to_owned())],
        Some("item.started") if item["type"] == "mcp_tool_call" => {
            vec![Event::Tool { name: format!("mcp__{}__{}", item["server"].as_str().unwrap_or(""), item["tool"].as_str().unwrap_or("")), input: item["arguments"].clone() }]
        }
        Some("turn.completed") => vec![Event::TurnDone { cost_usd: 0.0, error: None }],
        Some("turn.failed") | Some("error") => {
            let msg = v["error"]["message"].as_str().or(v["message"].as_str()).unwrap_or("Codex stopped.").to_owned();
            vec![Event::TurnDone { cost_usd: 0.0, error: Some(msg) }]
        }
        _ => vec![],
    }
}

/// Gemini CLI `-o stream-json` events.
pub fn parse_gemini(v: &Value, _: &std::sync::Mutex<Option<String>>) -> Vec<Event> {
    match v["type"].as_str() {
        Some("init") => vec![Event::Ready { model: v["model"].as_str().unwrap_or("Gemini").to_owned() }],
        Some("message") if v["role"] == "assistant" => vec![Event::Delta(v["content"].as_str().unwrap_or("").to_owned())],
        Some("tool_use") => vec![Event::Tool {
            name: v["tool_name"].as_str().or(v["name"].as_str()).unwrap_or("").to_owned(),
            input: if v["parameters"].is_null() { v["args"].clone() } else { v["parameters"].clone() },
        }],
        Some("result") => {
            let failed = v["status"].as_str().is_some_and(|s| s != "success");
            vec![Event::TurnDone { cost_usd: 0.0, error: failed.then(|| v["error"]["message"].as_str().unwrap_or("Gemini stopped.").to_owned()) }]
        }
        Some("error") => vec![Event::TurnDone { cost_usd: 0.0, error: Some(v["message"].as_str().unwrap_or("Gemini stopped.").to_owned()) }],
        _ => vec![],
    }
}

/// A tool's own name, whatever prefix the assistant's CLI puts on it (`mcp__tidy__overview`,
/// `tidy__overview`, `mcp_tidy_overview`).
pub fn tool_name(full: &str) -> String {
    let short = full.rsplit("__").next().unwrap_or(full);
    for prefix in ["mcp_tidy_", "tidy_", "tidy."] {
        if let Some(rest) = short.strip_prefix(prefix) {
            return rest.to_owned();
        }
    }
    short.to_owned()
}

fn read_events(stdout: std::process::ChildStdout, stderr: Option<std::process::ChildStderr>, tx: Sender<Event>, waker: rune::Waker, parse: fn(&Value) -> Vec<Event>) {
    let send = |e: Event| {
        let _ = tx.send(e);
        waker();
    };
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        for e in parse(&v) {
            send(e);
        }
    }
    let mut err = String::new();
    if let Some(mut s) = stderr {
        use std::io::Read;
        let _ = s.read_to_string(&mut err);
    }
    send(Event::Exited(err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("The assistant stopped.").to_owned()));
}

/// One stream-json line to the events it means.
pub fn parse(v: &Value) -> Vec<Event> {
    match v["type"].as_str() {
        Some("system") if v["subtype"] == "init" => vec![Event::Ready { model: v["model"].as_str().unwrap_or("").to_owned() }],
        Some("stream_event") => {
            let e = &v["event"];
            if e["type"] == "content_block_delta" && e["delta"]["type"] == "text_delta" {
                return vec![Event::Delta(e["delta"]["text"].as_str().unwrap_or("").to_owned())];
            }
            vec![]
        }
        Some("assistant") => v["message"]["content"]
            .as_array()
            .map(|blocks| {
                blocks
                    .iter()
                    .filter_map(|b| match b["type"].as_str() {
                        Some("text") => Some(Event::Text(b["text"].as_str().unwrap_or("").to_owned())),
                        Some("tool_use") => Some(Event::Tool { name: b["name"].as_str().unwrap_or("").to_owned(), input: b["input"].clone() }),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        Some("result") => {
            let error = (v["is_error"] == true).then(|| v["result"].as_str().unwrap_or("Something went wrong.").to_owned());
            vec![Event::TurnDone { cost_usd: v["total_cost_usd"].as_f64().unwrap_or(0.0), error }]
        }
        _ => vec![],
    }
}

/// `~/Library/Application Support/Tidy`.
pub fn support_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    home.join("Library/Application Support/Tidy")
}

pub fn snapshot_path() -> PathBuf {
    support_dir().join("snapshot.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_off_the_person_s_codex_servers_and_plugins() {
        let cfg = "[mcp_servers.node_repl]\ncommand = \"x\"\n[mcp_servers.node_repl.env]\nA = \"b\"\n[plugins.\"computer-use@openai-bundled\"]\nenabled = true\n";
        assert_eq!(codex_overrides(cfg), vec!["mcp_servers.node_repl.enabled=false", "plugins.\"computer-use@openai-bundled\".enabled=false"]);
    }

    #[test]
    fn reads_codex_and_gemini_events() {
        let t = std::sync::Mutex::new(None);
        parse_codex(&json!({"type": "thread.started", "thread_id": "abc"}), &t);
        assert_eq!(t.lock().unwrap().as_deref(), Some("abc"));
        let e = parse_codex(&json!({"type": "item.started", "item": {"type": "mcp_tool_call", "server": "tidy", "tool": "overview", "arguments": {}}}), &t);
        assert!(matches!(&e[..], [Event::Tool { name, .. }] if tool_name(name) == "overview"));
        let e = parse_gemini(&json!({"type": "message", "role": "assistant", "content": "Hi", "delta": true}), &t);
        assert!(matches!(&e[..], [Event::Delta(x)] if x == "Hi"));
        assert_eq!(tool_name("mcp_tidy_list_folder"), "list_folder");
        assert_eq!(tool_name("tidy__propose_cleanup"), "propose_cleanup");
    }

    #[test]
    fn parses_stream_json() {
        let e = parse(&json!({"type": "stream_event", "event": {"type": "content_block_delta", "delta": {"type": "text_delta", "text": "Hi"}}}));
        assert!(matches!(&e[..], [Event::Delta(t)] if t == "Hi"));
        let e = parse(&json!({"type": "assistant", "message": {"content": [{"type": "tool_use", "name": "mcp__tidy__overview", "input": {}}]}}));
        assert!(matches!(&e[..], [Event::Tool { name, .. }] if name == "mcp__tidy__overview"));
        let e = parse(&json!({"type": "result", "is_error": false, "total_cost_usd": 0.02}));
        assert!(matches!(&e[..], [Event::TurnDone { error: None, .. }]));
    }
}

#[cfg(test)]
mod live {
    use super::*;

    /// Talks to the Codex installed here: `cargo test -p tidy live_codex -- --ignored`.
    #[test]
    #[ignore]
    fn live_codex() {
        let d = detect();
        let p = d.providers.iter().find(|p| p.kind == ProviderKind::Codex).expect("codex installed").clone();
        let mut s = Session::start(&p, None, d.path_env.as_deref(), &snapshot_path(), std::sync::Arc::new(|| {})).unwrap();
        s.send("Call the overview tool, then say the total size in one short sentence.").unwrap();
        let mut seen = Vec::new();
        while let Ok(e) = s.events.recv_timeout(std::time::Duration::from_secs(150)) {
            let done = matches!(e, Event::TurnDone { .. });
            seen.push(format!("{e:?}").chars().take(120).collect::<String>());
            if done {
                break;
            }
        }
        for x in &seen {
            eprintln!("{x}");
        }
        assert!(seen.iter().any(|x| x.starts_with("Tool") && x.contains("mcp__tidy__overview")), "it called Tidy's tool");
        assert!(!seen.iter().any(|x| x.starts_with("Tool") && !x.contains("mcp__tidy__")), "and no other");
        assert!(seen.iter().any(|x| x.starts_with("Text")), "and answered");
        // A second turn resumes the same thread.
        s.send("And how many files? One sentence.").unwrap();
        let mut answered = false;
        while let Ok(e) = s.events.recv_timeout(std::time::Duration::from_secs(150)) {
            answered |= matches!(e, Event::Text(_));
            if matches!(e, Event::TurnDone { .. }) {
                eprintln!("turn 2: {e:?}");
                break;
            }
        }
        assert!(answered);
    }
}
