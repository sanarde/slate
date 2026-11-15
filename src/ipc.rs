// SPDX-License-Identifier: GPL-3.0-only

use std::io::{BufRead, BufReader, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rustix::event::{EventfdFlags, eventfd};

use crate::bindings::{self, Action};
use crate::json;

const REPLY_TIMEOUT: Duration = Duration::from_secs(2);

pub struct Request {
    line: String,
    reply: Sender<String>,
}

pub struct Ipc {
    requests: Receiver<Request>,
    wake: OwnedFd,
    subscribers: Arc<Mutex<Vec<Sender<String>>>>,
    path: PathBuf,
}

pub enum Command {
    Version,
    State,
    Windows,
    Workspaces,
    Outputs,
    Action(Action),
    Focus(u64),
    Close(u64),
    Minimize(u64),
    Restore(u64),
    MoveToWorkspace(u64, usize),
    Layout(usize, String),
}

pub struct WindowInfo {
    pub id: u64,
    pub title: String,
    pub app_id: String,
    pub workspace: usize,
    pub focused: bool,
    pub floating: bool,
    pub tiled: bool,
    pub maximized: bool,
    pub fullscreen: bool,
    pub minimized: bool,
}

pub struct WorkspaceInfo {
    pub index: usize,
    pub layout: String,
    pub windows: usize,
    pub active: bool,
}

pub struct OutputInfo {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f64,
    pub enabled: bool,
}

#[derive(Default)]
pub struct Snapshot {
    pub workspace: usize,
    pub focused: Option<u64>,
    pub workspaces: Vec<WorkspaceInfo>,
    pub windows: Vec<WindowInfo>,
    pub outputs: Vec<OutputInfo>,
}

pub trait Handler {
    fn snapshot(&mut self) -> Snapshot;
    fn execute(&mut self, command: Command) -> Result<(), String>;
}

pub fn default_path() -> PathBuf {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    runtime.join("slate.sock")
}

fn bind() -> Option<(UnixListener, PathBuf)> {
    let path = default_path();
    match UnixListener::bind(&path) {
        Ok(listener) => return Some((listener, path)),
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            if UnixStream::connect(&path).is_err() {
                let _ = std::fs::remove_file(&path);
                if let Ok(listener) = UnixListener::bind(&path) {
                    return Some((listener, path));
                }
            }
        }
        Err(error) => {
            tracing::warn!(%error, "failed to open the IPC socket");
            return None;
        }
    }

    let fallback = path.with_file_name(format!("slate-{}.sock", std::process::id()));
    match UnixListener::bind(&fallback) {
        Ok(listener) => Some((listener, fallback)),
        Err(error) => {
            tracing::warn!(%error, "failed to open the IPC socket");
            None
        }
    }
}

fn wake(fd: &OwnedFd) {
    let _ = rustix::io::write(fd, &1u64.to_ne_bytes());
}

fn serve_client(
    stream: UnixStream,
    requests: Sender<Request>,
    subscribers: Arc<Mutex<Vec<Sender<String>>>>,
    wake_fd: Arc<OwnedFd>,
) {
    let Ok(reader) = stream.try_clone() else {
        return;
    };
    let mut writer = stream;

    for line in BufReader::new(reader).lines() {
        let Ok(line) = line else {
            return;
        };
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        if line == "subscribe" {
            let (sender, events) = mpsc::channel();
            subscribers.lock().unwrap().push(sender);
            if writeln!(writer, "{}", json::object(&[("ok", "true".to_string())])).is_err() {
                return;
            }
            for event in events {
                if writeln!(writer, "{event}").is_err() {
                    return;
                }
            }
            return;
        }

        let (reply, answer) = mpsc::channel();
        if requests.send(Request { line, reply }).is_err() {
            return;
        }
        wake(&wake_fd);

        let response = answer
            .recv_timeout(REPLY_TIMEOUT)
            .unwrap_or_else(|_| error_response("the window manager did not answer"));
        if writeln!(writer, "{response}").is_err() {
            return;
        }
    }
}

impl Ipc {
    pub fn start() -> Option<Ipc> {
        let (listener, path) = bind()?;
        let wake_fd = eventfd(0, EventfdFlags::CLOEXEC | EventfdFlags::NONBLOCK).ok()?;
        let thread_wake = Arc::new(rustix::io::fcntl_dupfd_cloexec(&wake_fd, 0).ok()?);

        let (sender, receiver) = mpsc::channel();
        let subscribers: Arc<Mutex<Vec<Sender<String>>>> = Arc::new(Mutex::new(Vec::new()));
        let accept_subscribers = subscribers.clone();

        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let requests = sender.clone();
                let subscribers = accept_subscribers.clone();
                let wake_fd = thread_wake.clone();
                std::thread::spawn(move || serve_client(stream, requests, subscribers, wake_fd));
            }
        });

        tracing::info!(path = %path.display(), "IPC socket is open");
        Some(Ipc {
            requests: receiver,
            wake: wake_fd,
            subscribers,
            path,
        })
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn wake_fd(&self) -> Option<OwnedFd> {
        rustix::io::fcntl_dupfd_cloexec(self.wake.as_fd(), 0).ok()
    }

    pub fn broadcast(&self, event: String) {
        self.subscribers
            .lock()
            .unwrap()
            .retain(|subscriber| subscriber.send(event.clone()).is_ok());
    }

    fn drain_wake(&self) {
        let mut buffer = [0u8; 8];
        let _ = rustix::io::read(&self.wake, &mut buffer);
    }

    fn take_requests(&self) -> Vec<Request> {
        self.drain_wake();
        self.requests.try_iter().collect()
    }
}

impl Drop for Ipc {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub fn error_response(message: &str) -> String {
    json::object(&[("ok", "false".to_string()), ("error", json::quote(message))])
}

fn ok_response(data: String) -> String {
    json::object(&[("ok", "true".to_string()), ("data", data)])
}

pub fn parse(line: &str) -> Result<Command, String> {
    let mut parts = line.split_whitespace();
    let name = parts.next().ok_or("empty command")?;
    let rest: Vec<&str> = parts.collect();

    let id = |index: usize| -> Result<u64, String> {
        rest.get(index)
            .ok_or("a window id is missing".to_string())?
            .parse()
            .map_err(|_| "the window id is not a number".to_string())
    };

    match name {
        "version" => Ok(Command::Version),
        "state" => Ok(Command::State),
        "windows" => Ok(Command::Windows),
        "workspaces" => Ok(Command::Workspaces),
        "outputs" => Ok(Command::Outputs),
        "focus" => Ok(Command::Focus(id(0)?)),
        "close" => Ok(Command::Close(id(0)?)),
        "minimize" => Ok(Command::Minimize(id(0)?)),
        "restore" => Ok(Command::Restore(id(0)?)),
        "move-to-workspace" => {
            let number: usize = rest
                .get(1)
                .ok_or("a workspace number is missing")?
                .parse()
                .map_err(|_| "the workspace is not a number".to_string())?;
            if number == 0 {
                return Err("workspaces are numbered from 1".to_string());
            }
            Ok(Command::MoveToWorkspace(id(0)?, number - 1))
        }
        "layout" => {
            let number: usize = rest
                .first()
                .ok_or("a workspace number is missing")?
                .parse()
                .map_err(|_| "the workspace is not a number".to_string())?;
            if number == 0 {
                return Err("workspaces are numbered from 1".to_string());
            }
            let layout = rest.get(1).ok_or("a layout name is missing")?;
            Ok(Command::Layout(number - 1, layout.to_string()))
        }
        "action" => {
            let text = line.trim_start().strip_prefix("action").unwrap_or("").trim();
            match bindings::parse_action(text)? {
                Some(action) => Ok(Command::Action(action)),
                None => Err("none is not an action".to_string()),
            }
        }
        other => Err(format!("unknown command {other}")),
    }
}

fn window_json(window: &WindowInfo) -> String {
    let flag = |value: bool| value.to_string();
    json::object(&[
        ("id", window.id.to_string()),
        ("title", json::quote(&window.title)),
        ("app_id", json::quote(&window.app_id)),
        ("workspace", (window.workspace + 1).to_string()),
        ("focused", flag(window.focused)),
        ("floating", flag(window.floating)),
        ("tiled", flag(window.tiled)),
        ("maximized", flag(window.maximized)),
        ("fullscreen", flag(window.fullscreen)),
        ("minimized", flag(window.minimized)),
    ])
}

fn workspace_json(workspace: &WorkspaceInfo) -> String {
    json::object(&[
        ("index", (workspace.index + 1).to_string()),
        ("layout", json::quote(&workspace.layout)),
        ("windows", workspace.windows.to_string()),
        ("active", workspace.active.to_string()),
    ])
}

fn output_json(output: &OutputInfo) -> String {
    json::object(&[
        ("name", json::quote(&output.name)),
        ("x", output.x.to_string()),
        ("y", output.y.to_string()),
        ("width", output.width.to_string()),
        ("height", output.height.to_string()),
        ("scale", output.scale.to_string()),
        ("enabled", output.enabled.to_string()),
    ])
}

fn windows_json(snapshot: &Snapshot) -> String {
    json::array(&snapshot.windows.iter().map(window_json).collect::<Vec<_>>())
}

fn workspaces_json(snapshot: &Snapshot) -> String {
    json::array(&snapshot.workspaces.iter().map(workspace_json).collect::<Vec<_>>())
}

fn outputs_json(snapshot: &Snapshot) -> String {
    json::array(&snapshot.outputs.iter().map(output_json).collect::<Vec<_>>())
}

pub fn handle(handler: &mut impl Handler, line: &str) -> String {
    let command = match parse(line) {
        Ok(command) => command,
        Err(error) => return error_response(&error),
    };

    match command {
        Command::Version => ok_response(json::quote(env!("CARGO_PKG_VERSION"))),
        Command::State => {
            let snapshot = handler.snapshot();
            ok_response(json::object(&[
                ("workspace", (snapshot.workspace + 1).to_string()),
                (
                    "focused",
                    snapshot.focused.map_or("null".to_string(), |id| id.to_string()),
                ),
                ("workspaces", workspaces_json(&snapshot)),
                ("windows", windows_json(&snapshot)),
                ("outputs", outputs_json(&snapshot)),
            ]))
        }
        Command::Windows => ok_response(windows_json(&handler.snapshot())),
        Command::Workspaces => ok_response(workspaces_json(&handler.snapshot())),
        Command::Outputs => ok_response(outputs_json(&handler.snapshot())),
        other => match handler.execute(other) {
            Ok(()) => ok_response("null".to_string()),
            Err(error) => error_response(&error),
        },
    }
}

pub fn serve(ipc: &Ipc, handler: &mut impl Handler) {
    for request in ipc.take_requests() {
        let response = handle(handler, &request.line);
        let _ = request.reply.send(response);
    }
}

pub fn event(name: &str, fields: &[(&str, String)]) -> String {
    let mut all = vec![("event", json::quote(name))];
    all.extend(fields.iter().map(|(key, value)| (*key, value.clone())));
    json::object(&all)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        executed: Vec<String>,
    }

    impl Handler for Fake {
        fn snapshot(&mut self) -> Snapshot {
            Snapshot {
                workspace: 1,
                focused: Some(7),
                workspaces: vec![WorkspaceInfo {
                    index: 1,
                    layout: "spiral".to_string(),
                    windows: 1,
                    active: true,
                }],
                windows: vec![WindowInfo {
                    id: 7,
                    title: "a \"b\"".to_string(),
                    app_id: "foot".to_string(),
                    workspace: 1,
                    focused: true,
                    floating: false,
                    tiled: true,
                    maximized: false,
                    fullscreen: false,
                    minimized: false,
                }],
                outputs: Vec::new(),
            }
        }

        fn execute(&mut self, command: Command) -> Result<(), String> {
            self.executed.push(match command {
                Command::Focus(id) => format!("focus {id}"),
                Command::MoveToWorkspace(id, workspace) => format!("move {id} {workspace}"),
                Command::Layout(workspace, name) => format!("layout {workspace} {name}"),
                Command::Action(_) => "action".to_string(),
                _ => "other".to_string(),
            });
            Ok(())
        }
    }

    #[test]
    fn commands_are_parsed() {
        assert!(matches!(parse("focus 5"), Ok(Command::Focus(5))));
        assert!(matches!(parse("move-to-workspace 5 2"), Ok(Command::MoveToWorkspace(5, 1))));
        assert!(matches!(parse("layout 3 grid"), Ok(Command::Layout(2, _))));
        assert!(matches!(parse("action workspace 4"), Ok(Command::Action(Action::Workspace(3)))));
        assert!(parse("focus x").is_err());
        assert!(parse("layout 0 grid").is_err());
        assert!(parse("bogus").is_err());
        assert!(parse("action none").is_err());
    }

    #[test]
    fn responses_are_json() {
        let mut fake = Fake { executed: Vec::new() };

        let state = handle(&mut fake, "state");
        assert!(state.starts_with("{\"ok\":true,\"data\":{\"workspace\":2,\"focused\":7"), "{state}");
        assert!(state.contains("\"title\":\"a \\\"b\\\"\""));

        assert_eq!(handle(&mut fake, "focus 9"), "{\"ok\":true,\"data\":null}");
        assert_eq!(fake.executed, vec!["focus 9".to_string()]);
        assert!(handle(&mut fake, "nope").contains("\"ok\":false"));
    }

    #[test]
    fn events_carry_their_fields() {
        assert_eq!(
            event("workspace", &[("index", "3".to_string())]),
            "{\"event\":\"workspace\",\"index\":3}"
        );
    }
}
