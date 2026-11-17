pub mod agents;

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use sysinfo::{Pid, ProcessesToUpdate, System};
use tauri::{AppHandle, Emitter, State};

/// How much output a session remembers to replay when a panel attaches again.
const SCROLLBACK_BYTES: usize = 256 * 1024;
/// Size of a session that starts before any panel exists (an agent started in the background).
const DEFAULT_SIZE: (u16, u16) = (120, 30);
/// What a terminal answers to the cursor-position request (`ESC[6n`) that ConPTY sends and waits on.
const CURSOR_QUERY: &str = "\x1b[6n";
const CURSOR_REPLY: &[u8] = b"\x1b[1;1R";

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// The output a session has produced: the tail of it for replay, and a running number so a panel that attaches can
/// tell which live chunks the replay already contains.
#[derive(Default)]
struct Output {
    chunks: VecDeque<String>,
    len: usize,
    seq: u64,
}

impl Output {
    fn push(&mut self, data: String) {
        self.len += data.len();
        self.chunks.push_back(data);
        while self.len > SCROLLBACK_BYTES {
            match self.chunks.pop_front() {
                Some(old) => self.len -= old.len(),
                None => break,
            }
        }
    }

    fn replay(&self) -> String {
        self.chunks.iter().map(String::as_str).collect()
    }
}

/// What the session and its reader thread both see.
struct Shared {
    id: u64,
    out: Mutex<Output>,
    /// Unix milliseconds of the last output.
    last_output: AtomicU64,
    exited: AtomicBool,
    /// A panel shows the session: live output is sent to the frontend, and the panel answers the terminal queries.
    attached: AtomicBool,
}

struct Session {
    shared: Arc<Shared>,
    master: Box<dyn MasterPty + Send>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    child: Box<dyn Child + Send + Sync>,
    size: (u16, u16),
    /// The name of the agent preset started in it (None for a plain shell).
    agent: Option<String>,
    /// A program has been seen running (as a child process of the shell) since the agent was started.
    agent_seen: bool,
    /// How many looks in a row found the shell without a child process.
    no_child_looks: u8,
    /// The program has ended and the shell is back at its prompt (see `FINISHED_AFTER_LOOKS`). Not final: a new
    /// child process (the agent starting after a slow profile, a second command) makes it false again.
    agent_finished: bool,
}

/// A shell that had a child process and shows none for this many looks in a row counts as finished. One look is not
/// enough: a profile that starts helpers (conda, starship) runs them before the typed command, and the moment
/// between those and the agent must not read as the agent having ended.
const FINISHED_AFTER_LOOKS: u8 = 2;

impl Session {
    /// Whether the shell has a child process: a program (the agent, a build, ...) is running in it.
    fn has_child(&self, system: &System) -> bool {
        let Some(shell) = self.child.process_id() else { return false };
        let shell = Pid::from_u32(shell);
        system.processes().values().any(|p| p.parent() == Some(shell))
    }

    /// Learns from the process list whether the agent has started and ended. Call after refreshing `system`.
    fn update_agent_state(&mut self, system: &System) {
        if self.agent.is_none() {
            return;
        }
        if self.has_child(system) {
            self.agent_seen = true;
            self.no_child_looks = 0;
            self.agent_finished = false;
        } else if self.agent_seen {
            self.no_child_looks = self.no_child_looks.saturating_add(1);
            self.agent_finished = self.no_child_looks >= FINISHED_AFTER_LOOKS;
        }
    }

    fn stop(mut self) {
        let _ = self.child.kill();
        // Dropping the master closes the pseudo-terminal, which ends the reader thread.
    }

    fn write(&self, data: &[u8]) -> Result<(), String> {
        let mut w = self.writer.lock().unwrap();
        w.write_all(data).and_then(|_| w.flush()).map_err(|e| e.to_string())
    }

    fn resize(&mut self, cols: u16, rows: u16) -> Result<(), String> {
        let (cols, rows) = (cols.max(2), rows.max(1));
        self.master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }).map_err(|e| e.to_string())?;
        self.size = (cols, rows);
        Ok(())
    }

    /// Marks the session as shown and returns what a panel needs to catch up: the remembered output and its number.
    /// Taken under the output lock, so every chunk is either in the replay or sent live afterwards.
    fn attach(&self) -> (String, u64) {
        let out = self.shared.out.lock().unwrap();
        self.shared.attached.store(true, Ordering::SeqCst);
        (out.replay(), out.seq)
    }
}

/// The terminal sessions of the app, one per working directory. They outlive the panel that shows them (a tab switch
/// only detaches it) and end when their tab closes, their worktree is removed, or the app exits. The second field is
/// the process list, kept between calls so refreshing it stays cheap (None when the system refuses to give one: agents
/// are then never marked finished).
pub struct Terminal(Mutex<HashMap<String, Session>>, Mutex<Option<System>>);

impl Default for Terminal {
    fn default() -> Self {
        Self(Mutex::default(), Mutex::new(System::new().ok()))
    }
}

/// Looks at the running processes (only while some session has an agent) and updates what each agent is doing.
fn refresh_agents(sessions: &mut HashMap<String, Session>, system: &Mutex<Option<System>>) {
    if !sessions.values().any(|s| s.agent.is_some()) {
        return;
    }
    let mut guard = system.lock().unwrap();
    let Some(system) = guard.as_mut() else { return };
    system.refresh_processes(ProcessesToUpdate::All, true);
    for s in sessions.values_mut() {
        s.update_agent_state(&system);
    }
}

#[derive(Serialize, Clone)]
struct TermData {
    id: u64,
    seq: u64,
    data: String,
}

/// What the reader thread reports; the app turns it into events.
enum TermEvent {
    Data(TermData),
    Exit(u64),
}

fn shell() -> CommandBuilder {
    #[cfg(windows)]
    let mut cmd = {
        let mut c = CommandBuilder::new("powershell.exe");
        c.arg("-NoLogo");
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        // Login shell, so PATH from the user's profile is loaded even when launched from the Dock.
        let sh = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        let mut c = CommandBuilder::new(sh);
        c.arg("-l");
        c
    };
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd
}

fn now_millis() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn spawn(cwd: &Path, cols: u16, rows: u16, mut cmd: CommandBuilder) -> Result<(Session, Box<dyn Read + Send>), String> {
    let (cols, rows) = (cols.max(2), rows.max(1));
    let pair = native_pty_system()
        .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| e.to_string())?;
    cmd.cwd(cwd);
    let child = pair.slave.spawn_command(cmd).map_err(|e| format!("Could not start shell: {e}"))?;
    drop(pair.slave);
    let reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let shared = Arc::new(Shared {
        id: NEXT_ID.fetch_add(1, Ordering::SeqCst),
        out: Mutex::new(Output::default()),
        last_output: AtomicU64::new(now_millis()),
        exited: AtomicBool::new(false),
        attached: AtomicBool::new(false),
    });
    let session = Session {
        shared,
        master: pair.master,
        writer: Arc::new(Mutex::new(writer)),
        child,
        size: (cols, rows),
        agent: None,
        agent_seen: false,
        no_child_looks: 0,
        agent_finished: false,
    };
    Ok((session, reader))
}

/// Starts the shell in `path` and the thread that reads its output into the scrollback.
fn start_session(
    path: &str,
    cols: u16,
    rows: u16,
    emit: impl Fn(TermEvent) + Send + 'static,
) -> Result<Session, String> {
    let (session, mut reader) = spawn(Path::new(path), cols, rows, shell())?;
    let shared = session.shared.clone();
    let writer = session.writer.clone();
    std::thread::spawn(move || {
        let mut chunker = Utf8Chunker::new();
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let data = chunker.push(&buf[..n]);
                    if data.is_empty() {
                        continue;
                    }
                    // ConPTY waits for the cursor position; a panel (xterm.js) answers it, but nobody does while the
                    // session runs without one.
                    if data.contains(CURSOR_QUERY) && !shared.attached.load(Ordering::SeqCst) {
                        let _ = writer.lock().unwrap().write_all(CURSOR_REPLY);
                    }
                    shared.last_output.store(now_millis(), Ordering::SeqCst);
                    let mut out = shared.out.lock().unwrap();
                    out.seq += 1;
                    let seq = out.seq;
                    // The query is not kept: replaying it would make the next panel answer into the shell.
                    out.push(data.replace(CURSOR_QUERY, ""));
                    if shared.attached.load(Ordering::SeqCst) {
                        emit(TermEvent::Data(TermData { id: shared.id, seq, data }));
                    }
                }
            }
        }
        shared.exited.store(true, Ordering::SeqCst);
        emit(TermEvent::Exit(shared.id));
    });
    Ok(session)
}

fn emitter(app: AppHandle) -> impl Fn(TermEvent) + Send + 'static {
    move |event| match event {
        TermEvent::Data(d) => {
            let _ = app.emit("term-data", d);
        }
        TermEvent::Exit(id) => {
            let _ = app.emit("term-exit", id);
        }
    }
}

/// Turns raw bytes into text chunks, holding back an incomplete UTF-8 sequence that was
/// split across two reads so multi-byte characters never get mangled.
struct Utf8Chunker {
    pending: Vec<u8>,
}

impl Utf8Chunker {
    fn new() -> Self {
        Self { pending: Vec::new() }
    }

    fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut out = String::new();
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(s) => {
                    out.push_str(s);
                    self.pending.clear();
                    return out;
                }
                Err(e) => {
                    let valid = e.valid_up_to();
                    out.push_str(std::str::from_utf8(&self.pending[..valid]).unwrap());
                    match e.error_len() {
                        // Genuinely invalid bytes: substitute and keep going.
                        Some(n) => {
                            out.push('\u{FFFD}');
                            self.pending.drain(..valid + n);
                        }
                        // Sequence cut off at the end of the buffer: wait for the rest.
                        None => {
                            self.pending.drain(..valid);
                            return out;
                        }
                    }
                }
            }
        }
    }
}

/// What a panel gets when it shows a session.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TermAttach {
    /// Identifies the session in `term-data` / `term-exit` events (a restart gets a new one).
    id: u64,
    /// Live chunks numbered up to this are already in `replay`.
    seq: u64,
    replay: String,
    exited: bool,
    agent: Option<String>,
}

/// Shows the terminal of `path`: starts its shell when there is none (or when `restart` asks for a fresh one) and
/// returns what it printed so far. The panel then receives its live output as `term-data` events until `term_detach`.
#[tauri::command]
pub fn term_start(
    app: AppHandle,
    state: State<Terminal>,
    path: String,
    cols: u16,
    rows: u16,
    restart: bool,
) -> Result<TermAttach, String> {
    let mut sessions = state.0.lock().unwrap();
    if restart {
        if let Some(old) = sessions.remove(&path) {
            old.stop();
        }
    }
    if !sessions.contains_key(&path) {
        let session = start_session(&path, cols, rows, emitter(app))?;
        sessions.insert(path.clone(), session);
    }
    let s = sessions.get_mut(&path).expect("just inserted");
    if s.size != (cols.max(2), rows.max(1)) && !s.shared.exited.load(Ordering::SeqCst) {
        s.resize(cols, rows)?;
    }
    let (replay, seq) = s.attach();
    Ok(TermAttach {
        id: s.shared.id,
        seq,
        replay,
        exited: s.shared.exited.load(Ordering::SeqCst),
        agent: s.agent.clone(),
    })
}

/// The panel of `path` went away (another tab was selected); the session keeps running without it.
#[tauri::command]
pub fn term_detach(state: State<Terminal>, path: String) {
    if let Some(s) = state.0.lock().unwrap().get(&path) {
        s.shared.attached.store(false, Ordering::SeqCst);
    }
}

#[tauri::command]
pub fn term_write(state: State<Terminal>, id: u64, data: String) -> Result<(), String> {
    match state.0.lock().unwrap().values().find(|s| s.shared.id == id) {
        Some(s) => s.write(data.as_bytes()),
        None => Ok(()), // stale session: drop the input
    }
}

#[tauri::command]
pub fn term_resize(state: State<Terminal>, id: u64, cols: u16, rows: u16) -> Result<(), String> {
    match state.0.lock().unwrap().values_mut().find(|s| s.shared.id == id) {
        Some(s) => s.resize(cols, rows),
        None => Ok(()),
    }
}

/// Ends the terminal of `path` (and whatever runs in it).
#[tauri::command]
pub fn term_stop(state: State<Terminal>, path: String) {
    if let Some(s) = state.0.lock().unwrap().remove(&path) {
        s.stop();
    }
}

/// Types `command` and Enter into the terminal of `path` (started without a panel when there is none) and remembers
/// that `agent` runs there. Refused while an earlier agent session is alive: the text would go to that agent.
#[tauri::command]
pub fn term_run(
    app: AppHandle,
    state: State<Terminal>,
    path: String,
    command: String,
    agent: String,
) -> Result<(), String> {
    let mut sessions = state.0.lock().unwrap();
    if sessions.get(&path).is_some_and(|s| s.shared.exited.load(Ordering::SeqCst)) {
        if let Some(old) = sessions.remove(&path) {
            old.stop();
        }
    }
    refresh_agents(&mut sessions, &state.1);
    if let Some(s) = sessions.get(&path) {
        if let (Some(running), false) = (&s.agent, s.agent_finished) {
            return Err(format!("{running} was already started in this worktree. Stop its terminal session first."));
        }
    } else {
        let (cols, rows) = DEFAULT_SIZE;
        sessions.insert(path.clone(), start_session(&path, cols, rows, emitter(app))?);
    }
    let s = sessions.get_mut(&path).expect("just inserted");
    s.write(format!("{command}\r").as_bytes())?;
    s.agent = Some(agent);
    s.agent_seen = false;
    s.no_child_looks = 0;
    s.agent_finished = false;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TermInfo {
    path: String,
    running: bool,
    agent: Option<String>,
    /// The agent's program has ended and the shell is back at its prompt.
    finished: bool,
    /// Milliseconds since the session last printed anything.
    idle_ms: u64,
}

/// Every terminal session, for the status dots (cheap: the process list is only read while an agent is running).
#[tauri::command]
pub fn term_list(state: State<Terminal>) -> Vec<TermInfo> {
    let now = now_millis();
    let mut sessions = state.0.lock().unwrap();
    refresh_agents(&mut sessions, &state.1);
    sessions
        .iter()
        .map(|(path, s)| TermInfo {
            path: path.clone(),
            running: !s.shared.exited.load(Ordering::SeqCst),
            agent: s.agent.clone(),
            finished: s.agent_finished,
            idle_ms: now.saturating_sub(s.shared.last_output.load(Ordering::SeqCst)),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn chunker_keeps_split_characters_intact() {
        let mut c = Utf8Chunker::new();
        let euro = "€".as_bytes(); // 3 bytes
        assert_eq!(c.push(&euro[..1]), "");
        assert_eq!(c.push(&euro[1..2]), "");
        assert_eq!(c.push(&euro[2..]), "€");
        assert_eq!(c.push(b"ab\xFFcd"), "ab\u{FFFD}cd");
        assert_eq!(c.push("ä".as_bytes()), "ä");
    }

    #[test]
    fn scrollback_keeps_only_the_newest_output() {
        let mut out = Output::default();
        let chunk = "x".repeat(SCROLLBACK_BYTES / 4);
        for i in 0..6 {
            out.push(format!("{i}{chunk}"));
        }
        let replay = out.replay();
        assert!(replay.len() <= SCROLLBACK_BYTES + 1);
        assert!(replay.starts_with('3') && replay.contains('5') && !replay.contains('2'));
    }

    /// The agent counts as finished once its program has been seen as a child of the shell and is gone again, not
    /// before it started (the moment between typing the command and the program appearing).
    #[test]
    fn notices_when_the_program_in_the_shell_ends() {
        let dir = std::env::temp_dir().join(format!("gc-term-agent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut session = start_session(dir.to_str().unwrap(), 100, 30, |_| {}).unwrap();
        session.agent = Some("test".into());
        let mut system = System::new().unwrap();

        // Looks at the processes until `done` holds (or 30 s pass).
        fn watch(session: &mut Session, system: &mut System, done: fn(&Session) -> bool) {
            let deadline = Instant::now() + Duration::from_secs(30);
            while Instant::now() < deadline && !done(session) {
                std::thread::sleep(Duration::from_millis(200));
                system.refresh_processes(ProcessesToUpdate::All, true);
                session.update_agent_state(system);
            }
        }

        // Nothing runs in the fresh shell yet, so it is not an agent that has finished.
        std::thread::sleep(Duration::from_secs(2));
        system.refresh_processes(ProcessesToUpdate::All, true);
        session.update_agent_state(&system);
        assert!(!session.agent_finished, "an agent that never started is not finished");

        // A program that runs for a few seconds, like an agent would.
        #[cfg(windows)]
        let cmd = "ping -n 6 127.0.0.1 > $null\r\n";
        #[cfg(not(windows))]
        let cmd = "sleep 5\r\n";
        session.write(cmd.as_bytes()).unwrap();

        watch(&mut session, &mut system, |s| s.agent_seen);
        if !session.agent_seen {
            // What the failure needs: the pid the session reports, who its children are, and what the shell printed.
            let shell = session.child.process_id();
            let interesting: Vec<String> = system
                .processes()
                .values()
                .filter(|p| {
                    let name = p.name().to_string_lossy().to_lowercase();
                    p.parent().map(|pp| pp.as_u32()) == shell
                        || ["ping", "cmd", "sleep", "powershell", "conhost", "openconsole"]
                            .iter()
                            .any(|n| name.contains(n))
                })
                .map(|p| format!("{} {:?} parent={:?}", p.pid().as_u32(), p.name(), p.parent().map(|pp| pp.as_u32())))
                .collect();
            let tail: String = session.shared.out.lock().unwrap().replay().chars().rev().take(400).collect();
            let tail: String = tail.chars().rev().collect();
            panic!("the program was never seen as a child of the shell (pid {shell:?})\nprocesses: {interesting:#?}\noutput tail: {tail:?}");
        }
        assert!(!session.agent_finished);
        watch(&mut session, &mut system, |s| s.agent_finished);
        assert!(session.agent_finished, "the program ended but the session is not marked finished");
        session.stop();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A session without any panel: the reader answers ConPTY's cursor query itself, remembers the output for a
    /// later panel (without the query), and only sends live events once one is attached.
    #[test]
    fn shell_runs_unattended_and_replays_its_output() {
        let dir = std::env::temp_dir().join(format!("gc-term-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let events = Arc::new(Mutex::new(Vec::<u64>::new()));
        let seen = events.clone();
        let session = start_session(dir.to_str().unwrap(), 100, 30, move |e| {
            if let TermEvent::Data(d) = e {
                seen.lock().unwrap().push(d.seq);
            }
        })
        .unwrap();

        // The marker is built from two halves, so it only appears in the *output*, not in the echoed command line.
        let leaf = dir.file_name().unwrap().to_string_lossy().to_string();
        #[cfg(windows)]
        let cmd = "(Get-Location).Path; Write-Output ('gc-' + 'done')\r\n";
        #[cfg(not(windows))]
        let cmd = "pwd; echo gc-\"done\"\n";
        session.write(cmd.as_bytes()).unwrap();

        let deadline = Instant::now() + Duration::from_secs(20);
        let mut replay = String::new();
        while Instant::now() < deadline && !replay.contains("gc-done") {
            std::thread::sleep(Duration::from_millis(200));
            replay = session.shared.out.lock().unwrap().replay();
        }
        assert!(replay.contains("gc-done"), "no output marker; got: {replay:?}");
        assert!(replay.contains(&leaf), "cwd not used; got: {replay:?}");
        assert!(!replay.contains(CURSOR_QUERY), "the cursor query must not be replayed");
        assert!(events.lock().unwrap().is_empty(), "nothing is sent while no panel is attached");

        // Attaching hands over everything so far; what comes after is sent live and numbered after it.
        let (caught_up, seq) = session.attach();
        assert_eq!(caught_up, replay);
        session.write(b"echo gc-more\r\n").unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline && events.lock().unwrap().is_empty() {
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(events.lock().unwrap().iter().all(|&s| s > seq));
        assert!(!events.lock().unwrap().is_empty());
        session.stop();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
