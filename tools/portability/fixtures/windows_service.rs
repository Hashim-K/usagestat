//! Synthetic daemon for the GUI launcher's native process-tree contract.
use std::os::windows::process::CommandExt;
use std::{env, fs, net::TcpListener, path::PathBuf, process::Command, thread, time::Duration};
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetConsoleWindow() -> *mut std::ffi::c_void;
}
fn main() {
    let args: Vec<_> = env::args_os().collect();
    let path = PathBuf::from(&args[2]);
    if args[1] == "--descendant" {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        fs::write(
            path.join("descendant.ready"),
            listener.local_addr().unwrap().to_string(),
        )
        .unwrap();
        loop {
            let _ = listener.accept();
        }
    }
    assert_eq!(args[1], "--service-settings");
    let root = path.parent().unwrap();
    assert!(
        unsafe { GetConsoleWindow() }.is_null(),
        "background daemon received a console"
    );
    fs::write(root.join("parent.pid"), std::process::id().to_string()).unwrap();
    println!("synthetic daemon stdout");
    eprintln!("synthetic daemon stderr");
    if fs::read_to_string(root.join("fixture-mode")).unwrap() == "exit" {
        std::process::exit(37);
    }
    let _child = Command::new(env::current_exe().unwrap())
        .creation_flags(0x08000000)
        .arg("--descendant")
        .arg(root)
        .spawn()
        .unwrap();
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}
