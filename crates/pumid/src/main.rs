use pumi_proto::paths::{File, Paths};
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;
const USAGE: &str = "usage: pumid [--daemon] [--data-dir DIR] [--bin-dir DIR] [--no-persist] [--stay] [--no-host] [--clean] [-u FILE|NONE]
       pumid --headless [--clean] [-u FILE|NONE]
       pumid --stop | --restart | --version | --api-info [--internal]";
fn main() {
    if std::env::args_os().nth(1).is_some_and(|a| a == pumi_sys::pty::TTY_EXEC) {
        pumi_sys::pty::tty_exec(std::env::args_os().skip(2));
    }
    if let Ok(exe) = pumid::lifecycle::exe() {
        pumi_sys::pty::use_trampoline(exe);
    }
    pumi_proto::paths::export_appname();
    let mut args = std::env::args().skip(1);
    let mut daemon = false;
    let mut stop = false;
    let mut restart = false;
    let mut data_dir = None;
    let mut bin_dir = None;
    let mut persist = true;
    let mut stay = false;
    let mut headless = false;
    let mut host = Some(Vec::<String>::new());
    let mut resume = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--headless" => headless = true,
            "--clean" => {
                if let Some(h) = &mut host {
                    h.push(arg);
                }
            }
            "-u" => {
                let Some(file) = args.next() else {
                    eprintln!("pumid: -u needs a file or NONE");
                    std::process::exit(2);
                };
                if let Some(h) = &mut host {
                    h.extend([arg, file]);
                }
            }
            "--no-host" => host = None,
            "--daemon" => daemon = true,
            "--foreground" => daemon = false,
            "--stop" => stop = true,
            "--restart" => restart = true,
            "--data-dir" => data_dir = args.next().map(PathBuf::from),
            "--bin-dir" => bin_dir = args.next().map(PathBuf::from),
            "--no-persist" => persist = false,
            "--stay" => stay = true,
            "--resume" => resume = args.next().map(PathBuf::from),
            "--internal" => {}
            "--api-info" => {
                pumid::api::link();
                let mut functions = pumi_api::Registry::new().functions();
                if std::env::args().any(|a| a == "--internal") {
                    functions.extend(pumi_api::Registry::internal_functions());
                }
                let info = pumi_api::ApiInfo::new(functions);
                println!("{}", serde_json::to_string_pretty(&info).expect("the API is JSON"));
                return;
            }
            "--version" | "-V" => {
                println!("pumid {} (API level {})", pumi_proto::PUMI_VERSION, pumi_api::API_LEVEL);
                return;
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                return;
            }
            other => {
                eprintln!("pumid: unknown argument {other}\n{USAGE}");
                std::process::exit(2);
            }
        }
    }
    if headless {
        let dir = std::env::temp_dir().join(format!("pumi-h{}", std::process::id()));
        if resume.is_none() {
            let _ = std::fs::remove_dir_all(&dir);
        }
        data_dir = Some(dir);
        persist = false;
        stay = true;
        daemon = false;
    }
    let paths = data_dir.map(Paths::at).unwrap_or_else(Paths::resolve);
    if stop || restart {
        std::process::exit(control(&paths, stop));
    }
    let mut config = pumid::Config::new(paths);
    config.bin_dir = bin_dir.or_else(pumid::lifecycle::default_bin_dir);
    config.persist = persist;
    config.headless = headless;
    config.host = host;
    config.resume = resume;
    config.restart_after = std::env::var(pumid::handoff::RESTART_AFTER).ok().and_then(|pid| pid.parse().ok());
    if stay {
        config.idle_exit = None;
    }
    if daemon && config.resume.is_none() {
        let _ = config.paths.ensure();
        if let Err(err) = pumi_sys::daemon::detach(&config.paths.file(File::Log)) {
            eprintln!("pumid: cannot detach: {err}");
            std::process::exit(1);
        }
    }
    pumi_sys::signal::ignore_hangups();
    stop_on_term(&config.paths);
    let socket = config.paths.file(File::PumiSocket);
    let data = config.paths.data.clone();
    let result = pumid::run(config, || {
        if headless {
            println!("{}", socket.display());
            let _ = std::io::stdout().flush();
        }
    });
    if headless {
        let _ = std::fs::remove_dir_all(&data);
    }
    if let Err(err) = result {
        eprintln!("pumid: {err}");
        std::process::exit(1);
    }
}
fn stop_on_term(paths: &Paths) {
    let socket = paths.file(File::PumiSocket);
    pumi_sys::signal::on_shutdown(move || {
        if pumi_proto::server::call(&socket, "server.kill", serde_json::json!([])).is_err() {
            std::process::exit(1);
        }
        std::thread::sleep(Duration::from_secs(10));
        std::process::exit(1);
    });
}
fn control(paths: &Paths, stop: bool) -> i32 {
    use pumi_proto::server;
    if !server::answers(&paths.file(File::PumiSocket)) {
        eprintln!("pumid is not running");
        return if stop { 0 } else { 1 };
    }
    let done = match stop {
        true => server::stop(paths),
        false => pumid::lifecycle::exe().and_then(|exe| server::restart(&exe, paths)),
    };
    match done {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("pumid: {err}");
            1
        }
    }
}
