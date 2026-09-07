#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod native;

#[cfg(windows)]
fn main() {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--service-settings")) {
        std::process::exit(2);
    }
    let Some(path) = args.next().map(std::path::PathBuf::from) else {
        std::process::exit(2);
    };
    if args.next().is_some() || !path.is_absolute() {
        std::process::exit(2);
    }
    let code = match native::run(&path) {
        Ok(code) => code,
        Err(error) => {
            if let Some(parent) = path.parent() {
                let message = format!("Windows background launcher failed: {error}\n");
                let _ = usagestat_core::storage::append_private(
                    &parent.join("daemon-startup-error.log"),
                    message.as_bytes(),
                );
            }
            1
        }
    };
    std::process::exit(code as i32);
}

#[cfg(not(windows))]
fn main() {
    eprintln!("usagestat-service is an internal Windows launcher; use usagestatd on this platform");
    std::process::exit(2);
}
