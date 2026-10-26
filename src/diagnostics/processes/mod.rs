//! The processes the app is made of and the ones that draw for it: the app, its children, the
//! server (pumid, pumi-host) and WindowServer — their CPU, memory and share of the GPU — and the
//! app's own threads by name. Read from the OS once a second; off macOS there is nothing to read.
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod os;
#[cfg(not(target_os = "macos"))]
#[path = "other.rs"]
mod os;

use os::COMPOSITOR;

/// One process, one second.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Process {
    pub name: String,
    pub pid: i32,
    /// What it is to pumi: `app`, `child`, `server`, `host`, `compositor`.
    pub role: &'static str,
    /// CPU, 100 a core; none for a process the app may not read (another user's).
    pub cpu: Option<f32>,
    /// Its share of the GPU's time, 100 the whole GPU.
    pub gpu: Option<f32>,
    pub rss: Option<u64>,
    pub threads: Option<u32>,
}

/// The app's threads that share a name, one second.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Threads {
    pub name: String,
    pub count: u32,
    pub cpu: f32,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Processes {
    pub list: Vec<Process>,
    pub threads: Vec<Threads>,
    /// The GPU's own utilisation (every process's work), 100 the whole GPU.
    pub gpu_total: Option<f32>,
}

/// What the last read saw, for the next one's differences.
pub struct Reader {
    data_dir: PathBuf,
    at: Instant,
    cpu: HashMap<i32, Duration>,
    gpu: HashMap<i32, u64>,
    thread_cpu: HashMap<u64, Duration>,
    compositor: Option<i32>,
}

impl Reader {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            at: Instant::now(),
            cpu: HashMap::new(),
            gpu: HashMap::new(),
            thread_cpu: HashMap::new(),
            compositor: None,
        }
    }

    /// The processes and threads since the last read (the first read has no differences: zeros).
    pub fn read(&mut self) -> Processes {
        let now = Instant::now();
        let wall = now.saturating_duration_since(self.at).max(Duration::from_millis(1));
        self.at = now;
        let percent = |busy: Duration| (100.0 * busy.as_secs_f64() / wall.as_secs_f64()) as f32;
        let mut list = self.members();
        let gpu = os::gpu_times();
        let mut cpu_seen = HashMap::new();
        let mut gpu_seen = HashMap::new();
        for process in &mut list {
            if let Some(task) = os::task(process.pid) {
                let was = self.cpu.get(&process.pid).copied().unwrap_or(task.cpu);
                process.cpu = Some(percent(task.cpu.saturating_sub(was)));
                process.rss = Some(task.rss);
                process.threads = Some(task.threads);
                cpu_seen.insert(process.pid, task.cpu);
            }
            if let Some(&time) = gpu.as_ref().and_then(|gpu| gpu.get(&process.pid)) {
                let was = self.gpu.get(&process.pid).copied().unwrap_or(time);
                process.gpu = Some(percent(Duration::from_nanos(time.saturating_sub(was))));
                gpu_seen.insert(process.pid, time);
            } else if gpu.is_some() {
                process.gpu = Some(0.0);
            }
        }
        self.cpu = cpu_seen;
        self.gpu = gpu_seen;
        let mut by_name: HashMap<String, Threads> = HashMap::new();
        let mut thread_seen = HashMap::new();
        for thread in os::threads() {
            let was = self.thread_cpu.get(&thread.id).copied().unwrap_or(thread.cpu);
            let entry = by_name
                .entry(thread.name.clone())
                .or_insert_with(|| Threads { name: thread.name, ..Default::default() });
            entry.count += 1;
            entry.cpu += percent(thread.cpu.saturating_sub(was));
            thread_seen.insert(thread.id, thread.cpu);
        }
        self.thread_cpu = thread_seen;
        let mut threads: Vec<Threads> = by_name.into_values().collect();
        threads.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then_with(|| a.name.cmp(&b.name)));
        Processes { list, threads, gpu_total: os::gpu_utilisation() }
    }

    /// The app, its children, the server and its host, and the compositor.
    fn members(&mut self) -> Vec<Process> {
        let me = std::process::id() as i32;
        let process = |pid: i32, role: &'static str| Process {
            name: os::name(pid).unwrap_or_else(|| format!("pid {pid}")),
            pid,
            role,
            ..Process::default()
        };
        let mut list = vec![Process { name: "pumi (app)".into(), ..process(me, "app") }];
        list.extend(os::children(me).into_iter().map(|pid| process(pid, "child")));
        if let Some(server) = server_pid(&self.data_dir).filter(|pid| os::name(*pid).is_some()) {
            list.push(process(server, "server"));
            list.extend(
                os::children(server)
                    .into_iter()
                    .map(|pid| process(pid, "host"))
                    .filter(|p| p.name.starts_with("pumi-host")),
            );
        }
        let compositor = self.compositor.filter(|pid| os::name(*pid).as_deref() == Some(COMPOSITOR));
        self.compositor = compositor.or_else(|| os::find(COMPOSITOR));
        list.extend(self.compositor.map(|pid| process(pid, "compositor")));
        list
    }
}

/// The server's pid, as its lock file has it.
fn server_pid(data_dir: &std::path::Path) -> Option<i32> {
    std::fs::read_to_string(data_dir.join("server.lock")).ok()?.trim().parse().ok()
}

pub struct Thread {
    pub id: u64,
    pub name: String,
    pub cpu: Duration,
}

pub struct Task {
    pub cpu: Duration,
    pub rss: u64,
    pub threads: u32,
}
