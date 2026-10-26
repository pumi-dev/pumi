use std::path::PathBuf;
const USAGE: &str = "usage: pumi-host --socket SOCKET [--data-dir DIR] [--clean] [-u NONE]
       pumi-host --api-info";
fn main() {
    pumi_proto::paths::export_appname();
    let mut args = std::env::args().skip(1);
    let mut socket = std::env::var_os("PUMI").map(PathBuf::from);
    let mut data = None;
    let mut startup = pumi_host::defaults::Startup::default();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--socket" => socket = args.next().map(PathBuf::from),
            "--data-dir" => data = args.next().map(PathBuf::from),
            "--clean" => startup.clean = true,
            "-u" => match args.next().as_deref() {
                Some("NONE") => startup.none = true,
                Some(file) => {
                    eprintln!(
                        "pumi-host: -u {file}: there are no init files; a startup file is `defaults` in settings.json"
                    );
                    std::process::exit(2);
                }
                None => {
                    eprintln!("pumi-host: -u needs NONE");
                    std::process::exit(2);
                }
            },
            "--api-info" => {
                pumi_host::api::link();
                let info = pumi_api::ApiInfo::new(pumi_api::Registry::new().functions());
                println!("{}", serde_json::to_string_pretty(&info).expect("the API is JSON"));
                return;
            }
            "--version" | "-V" => {
                println!("pumi-host {}", pumi_proto::PUMI_VERSION);
                return;
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                return;
            }
            other => {
                eprintln!("pumi-host: unknown argument {other}\n{USAGE}");
                std::process::exit(2);
            }
        }
    }
    let Some(socket) = socket else {
        eprintln!("pumi-host: no core to serve (--socket, or $PUMI)\n{USAGE}");
        std::process::exit(2);
    };
    pumi_sys::signal::ignore_hangups();
    // A socket's address is a file in the data directory on Unix, a pipe name on Windows.
    let data = data.or_else(|| socket.parent().map(PathBuf::from)).unwrap_or_default();
    if let Err(err) = pumi_host::run(pumi_host::Config { socket, data, startup }) {
        eprintln!("pumi-host: {err}");
        std::process::exit(1);
    }
}
