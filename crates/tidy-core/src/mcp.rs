//! A small MCP server over stdio (JSON-RPC 2.0, one message per line) that lets an AI already
//! installed on the Mac (Claude Code, Codex, Gemini CLI) look at a scan.
//!
//! Every tool is read-only. `propose_cleanup` deletes nothing: the window watches the
//! assistant's tool calls and shows the proposal for the person to approve, and only then
//! moves the items to the Trash.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::Path;

use serde_json::{Value, json};

use crate::format;
use crate::snapshot::{Entry, Snapshot};

pub const SERVER_NAME: &str = "tidy";
pub const PROPOSE_TOOL: &str = "propose_cleanup";
/// Handled by the window, like `propose_cleanup`: the server only acknowledges.
pub const SHOW_TOOL: &str = "show_in_tidy";
pub const VIEW_TOOL: &str = "open_view";
const PROTOCOL: &str = "2025-06-18";

/// The tool names as an MCP client prefixes them (`mcp__tidy__overview`, …).
pub fn qualified_tools() -> Vec<String> {
    tools().as_array().unwrap().iter().map(|t| format!("mcp__{SERVER_NAME}__{}", t["name"].as_str().unwrap())).collect()
}

pub fn tools() -> Value {
    json!([
        {
            "name": "overview",
            "description": "Summary of the scan Tidy already made (no new scan; answers instantly): total size, volume capacity and free space, bytes per kind of file, the largest top-level items, and Tidy's own offline cleanup suggestions (known caches and build folders). Call this first.",
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
        },
        {
            "name": "list_folder",
            "description": "The largest items directly inside a folder from the scan, with size, file count, kind and last modified/opened dates. Use it to drill into where space goes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Absolute folder path inside the scanned root."},
                    "limit": {"type": "integer", "description": "Max items (default 25, max 100)."}
                },
                "required": ["path"],
                "additionalProperties": false
            }
        },
        {
            "name": "find_files",
            "description": "Search the scan for files (not folders) by size, kind, age and name. Results are largest first.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "min_mb": {"type": "number", "description": "Minimum size in MB (default 50)."},
                    "kind": {"type": "string", "enum": ["app", "video", "image", "audio", "document", "archive", "disk_image", "code", "library", "other"]},
                    "not_opened_days": {"type": "integer", "description": "Only files not opened for at least this many days."},
                    "under": {"type": "string", "description": "Only inside this folder."},
                    "name_contains": {"type": "string"},
                    "limit": {"type": "integer", "description": "Max results (default 30, max 100)."}
                },
                "additionalProperties": false
            }
        },
        {
            "name": "developer_findings",
            "description": "What Tidy's Developer view found: git worktrees (with uncommitted-change checks), build output of idle projects, iOS simulators and runtimes, Android emulators, package caches, toolchains, Docker. Each with size, last use, and how Tidy would clean it. The person cleans these from the Developer view; you can point them there or propose the ones with a path.",
            "inputSchema": {
                "type": "object",
                "properties": {"category": {"type": "string", "description": "Optional: one category title, e.g. 'Git worktrees'."}},
                "additionalProperties": false
            }
        },
        {
            "name": SHOW_TOOL,
            "description": "Show a folder or file in Tidy's window: the map zooms to it and selects it, so the person sees what you're talking about. Use it when you point at something specific.",
            "inputSchema": {
                "type": "object",
                "properties": {"path": {"type": "string", "description": "Absolute path from the scan."}},
                "required": ["path"],
                "additionalProperties": false
            }
        },
        {
            "name": VIEW_TOOL,
            "description": "Open one of Tidy's views for the person: the space map, Tidy's offline suggestions, large files, duplicates, the developer view (worktrees, simulators, build output, caches), or changes since earlier scans.",
            "inputSchema": {
                "type": "object",
                "properties": {"view": {"type": "string", "enum": ["map", "suggestions", "large_files", "duplicates", "developer", "changes"]}},
                "required": ["view"],
                "additionalProperties": false
            }
        },
        {
            "name": PROPOSE_TOOL,
            "description": "Show the user a cleanup proposal in Tidy's window: a list of files or folders to move to the Trash, each with a reason and a risk level. Nothing is deleted by this call; the user reviews, unticks what they want to keep, and confirms. Only propose paths you saw in this scan's tool results.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "Short heading, e.g. 'Developer caches'."},
                    "items": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "path": {"type": "string"},
                                "reason": {"type": "string", "description": "One sentence: why it's safe or worth removing."},
                                "risk": {"type": "string", "enum": ["safe", "review"]}
                            },
                            "required": ["path", "reason", "risk"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["title", "items"],
                "additionalProperties": false
            }
        }
    ])
}

struct Server {
    snap: Snapshot,
    by_path: HashMap<String, usize>,
}

impl Server {
    fn new(snap: Snapshot) -> Server {
        let by_path = snap.entries.iter().enumerate().map(|(i, e)| (e.path.clone(), i)).collect();
        Server { snap, by_path }
    }

    fn entry_json(&self, e: &Entry, now: i64) -> Value {
        json!({
            "path": e.path,
            "size": format::bytes(e.size),
            "bytes": e.size,
            "type": if e.dir { "folder" } else { "file" },
            "files": e.files,
            "kind": e.kind,
            "modified": format::age(now - e.modified),
            "last_opened": format::age(now - e.accessed),
        })
    }

    fn call(&self, name: &str, args: &Value) -> Result<Value, String> {
        let now = crate::scan::now();
        let s = &self.snap;
        match name {
            "overview" => {
                let top: Vec<Value> = self.children(&s.root, 15).into_iter().map(|e| self.entry_json(e, now)).collect();
                Ok(json!({
                    "root": s.root,
                    "scanned": format::age(now - s.scanned_at),
                    "total": format::bytes(s.total),
                    "files": s.files,
                    "volume_capacity": format::bytes(s.volume_total),
                    "volume_free": format::bytes(s.volume_free),
                    "by_kind": s.by_kind.iter().map(|k| json!({"kind": k.label, "size": format::bytes(k.bytes), "files": k.files})).collect::<Vec<_>>(),
                    "largest_items": top,
                    "tidy_suggestions": s.suggestions.iter().take(25).collect::<Vec<_>>(),
                    "more_suggestions": s.suggestions.len().saturating_sub(25),
                    "note": format!("Items under {} are left out of this scan's index.", format::bytes(s.min_size)),
                }))
            }
            "list_folder" => {
                let path = args["path"].as_str().ok_or("path is required")?.trim_end_matches('/');
                let path = if path.is_empty() { "/" } else { path };
                let limit = args["limit"].as_u64().unwrap_or(25).min(100) as usize;
                let Some(&i) = self.by_path.get(path) else {
                    return Err(format!("{path} isn't in the scan (outside {} or smaller than {}).", s.root, format::bytes(s.min_size)));
                };
                let me = &s.entries[i];
                let kids = self.children(path, limit);
                let shown: u64 = kids.iter().map(|e| e.size).sum();
                Ok(json!({
                    "folder": self.entry_json(me, now),
                    "items": kids.iter().map(|e| self.entry_json(e, now)).collect::<Vec<_>>(),
                    "not_listed": format::bytes(me.size.saturating_sub(shown)),
                }))
            }
            "find_files" => {
                let min = (args["min_mb"].as_f64().unwrap_or(50.0) * 1_000_000.0) as u64;
                let kind = args["kind"].as_str();
                let idle = args["not_opened_days"].as_i64().map(|d| now - d * 86_400);
                let under = args["under"].as_str().map(|u| u.trim_end_matches('/').to_owned() + "/");
                let needle = args["name_contains"].as_str().map(str::to_lowercase);
                let limit = args["limit"].as_u64().unwrap_or(30).min(100) as usize;
                let mut hits: Vec<&Entry> = s
                    .entries
                    .iter()
                    .filter(|e| !e.dir && e.size >= min)
                    .filter(|e| kind.is_none_or(|k| e.kind == k))
                    .filter(|e| idle.is_none_or(|t| e.accessed <= t))
                    .filter(|e| under.as_ref().is_none_or(|u| e.path.starts_with(u.as_str())))
                    .filter(|e| needle.as_ref().is_none_or(|n| e.path.rsplit('/').next().unwrap_or("").to_lowercase().contains(n.as_str())))
                    .collect();
                hits.sort_by(|a, b| b.size.cmp(&a.size));
                let total: u64 = hits.iter().map(|e| e.size).sum();
                let count = hits.len();
                hits.truncate(limit);
                Ok(json!({
                    "matches": count,
                    "total": format::bytes(total),
                    "files": hits.iter().map(|e| self.entry_json(e, now)).collect::<Vec<_>>(),
                }))
            }
            "developer_findings" => {
                let cat = args["category"].as_str();
                let list: Vec<&Value> = s.developer.iter().filter(|f| cat.is_none_or(|c| f["category"].as_str() == Some(c))).collect();
                let total: u64 = list.iter().filter_map(|f| f["bytes"].as_u64()).sum();
                Ok(json!({"total": format::bytes(total), "findings": list.iter().take(80).collect::<Vec<_>>()}))
            }
            SHOW_TOOL => {
                let p = args["path"].as_str().ok_or("path is required")?;
                if !self.by_path.contains_key(p) && !Path::new(p).exists() {
                    return Err(format!("{p} isn't in the scan"));
                }
                Ok(json!({"shown": true}))
            }
            VIEW_TOOL => Ok(json!({"opened": args["view"]})),
            PROPOSE_TOOL => {
                let items = args["items"].as_array().ok_or("items is required")?;
                let mut unknown = Vec::new();
                let mut total = 0;
                for it in items {
                    let p = it["path"].as_str().unwrap_or("");
                    match self.by_path.get(p) {
                        Some(&i) => total += s.entries[i].size,
                        None => unknown.push(p.to_owned()),
                    }
                }
                if !unknown.is_empty() {
                    return Err(format!("These paths aren't in the scan; propose only paths from tool results: {}", unknown.join(", ")));
                }
                Ok(json!({
                    "shown_to_user": true,
                    "reclaimable": format::bytes(total),
                    "note": "The proposal is on screen. The user decides; nothing has been deleted. Don't repeat the list in your reply, just say a word about it.",
                }))
            }
            _ => Err(format!("unknown tool {name}")),
        }
    }

    fn children(&self, path: &str, limit: usize) -> Vec<&Entry> {
        let prefix = if path == "/" { "/".to_owned() } else { format!("{path}/") };
        let mut v: Vec<&Entry> =
            self.snap.entries.iter().filter(|e| e.path.len() > prefix.len() && e.path.starts_with(&prefix) && !e.path[prefix.len()..].contains('/')).collect();
        v.sort_by(|a, b| b.size.cmp(&a.size));
        v.truncate(limit);
        v
    }

    fn handle(&self, msg: &Value) -> Option<Value> {
        let id = msg.get("id")?.clone(); // notifications get no answer
        let method = msg["method"].as_str().unwrap_or("");
        let result = match method {
            "initialize" => json!({
                "protocolVersion": msg["params"]["protocolVersion"].as_str().unwrap_or(PROTOCOL),
                "capabilities": {"tools": {}},
                "serverInfo": {"name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION")},
            }),
            "ping" => json!({}),
            "tools/list" => json!({"tools": tools()}),
            "tools/call" => {
                let name = msg["params"]["name"].as_str().unwrap_or("");
                let args = &msg["params"]["arguments"];
                match self.call(name, args) {
                    Ok(v) => json!({"content": [{"type": "text", "text": v.to_string()}]}),
                    Err(e) => json!({"content": [{"type": "text", "text": e}], "isError": true}),
                }
            }
            _ => return Some(json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": format!("no method {method}")}})),
        };
        Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
    }
}

/// Runs the server on stdin/stdout until stdin closes.
pub fn serve(snapshot: &Path) -> std::io::Result<()> {
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let mut server = Server::new(Snapshot::read(snapshot)?);
    let mut seen = modified(snapshot);
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
        // The window rewrites the snapshot after a rescan or a cleanup: answer from the latest.
        let now = modified(snapshot);
        if now != seen {
            if let Ok(s) = Snapshot::read(snapshot) {
                server = Server::new(s);
            }
            seen = now;
        }
        if let Some(reply) = server.handle(&msg) {
            writeln!(out, "{reply}")?;
            out.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> Server {
        let e = |path: &str, size, dir| Entry { path: path.into(), size, files: 1, dir, kind: "video".into(), modified: 0, accessed: 0 };
        Server::new(Snapshot {
            root: "/u".into(),
            total: 300,
            files: 2,
            volume_total: 1000,
            volume_free: 10,
            scanned_at: 0,
            min_size: 1,
            by_kind: vec![],
            suggestions: vec![],
            developer: vec![],
            entries: vec![e("/u", 300, true), e("/u/a", 200, true), e("/u/a/x.mov", 200, false), e("/u/b.mov", 100, false)],
        })
    }

    #[test]
    fn lists_and_finds() {
        let s = server();
        let v = s.call("list_folder", &json!({"path": "/u"})).unwrap();
        assert_eq!(v["items"].as_array().unwrap().len(), 2);
        let v = s.call("find_files", &json!({"min_mb": 0, "under": "/u/a"})).unwrap();
        assert_eq!(v["matches"], 1);
        assert!(s.call(PROPOSE_TOOL, &json!({"title": "t", "items": [{"path": "/nope", "reason": "r", "risk": "safe"}]})).is_err());
        let init = s.handle(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}})).unwrap();
        assert_eq!(init["result"]["serverInfo"]["name"], "tidy");
        assert!(s.handle(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).is_none());
    }
}
