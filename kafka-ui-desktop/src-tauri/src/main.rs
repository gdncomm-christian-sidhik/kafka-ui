// Desktop shell for kafka-ui: starts the bundled Spring Boot backend on a free
// localhost port, waits until it answers HTTP, then points the window at it.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs::{self, File};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use tauri::{Manager, RunEvent, Url, WebviewWindow};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(180);

struct Backend(Mutex<Option<Child>>);

fn main() {
    // WebKitGTK's DMA-BUF renderer crashes on some Wayland/NVIDIA setups
    // ("Error 71 (Protocol error) dispatching to Wayland display").
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    let app = tauri::Builder::default()
        .manage(Backend(Mutex::new(None)))
        .setup(|app| {
            let window = app
                .get_webview_window("main")
                .expect("main window is defined in tauri.conf.json");
            match start_backend(app.handle()) {
                Ok((child, port, log_path)) => {
                    *app.state::<Backend>().0.lock().unwrap() = Some(child);
                    let handle = app.handle().clone();
                    thread::spawn(move || wait_and_navigate(handle, window, port, log_path));
                }
                Err(e) => {
                    // The splash page may not be loaded yet during setup.
                    thread::spawn(move || {
                        thread::sleep(Duration::from_millis(500));
                        show_error(&window, &e);
                    });
                }
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building kafka-ui desktop");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            stop_backend(handle);
        }
    });
}

fn start_backend(app: &tauri::AppHandle) -> Result<(Child, u16, PathBuf), String> {
    let paths = app.path();
    let resources = paths.resource_dir().map_err(|e| e.to_string())?;
    let config_dir = paths.app_config_dir().map_err(|e| e.to_string())?;
    let log_dir = paths.app_log_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&config_dir).map_err(|e| format!("cannot create {}: {e}", config_dir.display()))?;
    fs::create_dir_all(&log_dir).map_err(|e| format!("cannot create {}: {e}", log_dir.display()))?;

    let jar = std::env::var_os("KAFKA_UI_JAR")
        .map(PathBuf::from)
        .unwrap_or_else(|| resources.join("kafka-ui-api.jar"));
    if !jar.is_file() {
        return Err(format!("backend jar not found at {}", jar.display()));
    }
    let java = find_java(&resources)?;

    let port = free_port()?;
    let log_path = log_dir.join("backend.log");
    let log = File::create(&log_path).map_err(|e| format!("cannot create {}: {e}", log_path.display()))?;
    let log_err = log.try_clone().map_err(|e| e.to_string())?;

    let mut cmd = Command::new(&java);
    if let Ok(opts) = std::env::var("KAFKA_UI_JAVA_OPTS") {
        cmd.args(opts.split_whitespace());
    }
    cmd.arg("-jar")
        .arg(&jar)
        .arg("--server.address=127.0.0.1")
        .arg(format!("--server.port={port}"))
        // Clusters added in the UI are persisted here, so no config file is needed up front.
        .arg("--dynamic.config.enabled=true")
        .arg(format!("--dynamic.config.path={}", config_dir.join("dynamic_config.yaml").display()))
        .arg(format!("--config.related.uploads.dir={}", config_dir.join("uploads").display()))
        // An optional hand-written application.yml in the config dir is picked up too.
        .arg(format!("--spring.config.additional-location=optional:file:{}/", config_dir.display()))
        .current_dir(&config_dir)
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(log_err);
    kill_with_parent(&mut cmd);

    let child = cmd
        .spawn()
        .map_err(|e| format!("cannot start {}: {e}", java.display()))?;
    Ok((child, port, log_path))
}

fn find_java(resources: &Path) -> Result<PathBuf, String> {
    let exe = if cfg!(windows) { "java.exe" } else { "java" };
    let bundled = resources.join("runtime").join("bin").join(exe);
    if bundled.is_file() {
        return Ok(bundled);
    }
    if let Some(home) = std::env::var_os("JAVA_HOME") {
        let candidate = PathBuf::from(home).join("bin").join(exe);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    // Last resort: rely on PATH.
    Ok(PathBuf::from(exe))
}

fn free_port() -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("no free port: {e}"))?;
    listener.local_addr().map(|a| a.port()).map_err(|e| e.to_string())
}

#[cfg(target_os = "linux")]
fn kill_with_parent(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    // If the desktop app dies without a clean exit, the JVM receives SIGTERM.
    unsafe {
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
}

#[cfg(not(target_os = "linux"))]
fn kill_with_parent(_cmd: &mut Command) {}

fn wait_and_navigate(app: tauri::AppHandle, window: WebviewWindow, port: u16, log_path: PathBuf) {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let started = Instant::now();
    loop {
        if let Some(status) = backend_exit_status(&app) {
            show_error(&window, &format!("backend exited ({status})\n\n{}", log_tail(&log_path)));
            return;
        }
        if http_responds(addr) {
            let url: Url = format!("http://127.0.0.1:{port}/").parse().unwrap();
            if let Err(e) = window.navigate(url) {
                show_error(&window, &format!("cannot open backend page: {e}"));
            }
            return;
        }
        if started.elapsed() > STARTUP_TIMEOUT {
            show_error(&window, &format!("backend did not start within {}s\n\n{}",
                STARTUP_TIMEOUT.as_secs(), log_tail(&log_path)));
            return;
        }
        thread::sleep(Duration::from_millis(500));
    }
}

fn backend_exit_status(app: &tauri::AppHandle) -> Option<String> {
    let state = app.state::<Backend>();
    let mut guard = state.0.lock().unwrap();
    let child = guard.as_mut()?;
    match child.try_wait() {
        Ok(Some(status)) => Some(status.to_string()),
        Ok(None) => None,
        Err(e) => Some(e.to_string()),
    }
}

/// Any HTTP response from the health endpoint means the web server is up.
fn http_responds(addr: SocketAddr) -> bool {
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(500)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let request = "GET /actuator/health HTTP/1.0\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut buf = [0u8; 16];
    matches!(stream.read(&mut buf), Ok(n) if buf[..n].starts_with(b"HTTP/"))
}

fn log_tail(path: &Path) -> String {
    let content = fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = content.lines().collect();
    let tail = lines[lines.len().saturating_sub(40)..].join("\n");
    format!("Log: {}\n\n{tail}", path.display())
}

fn show_error(window: &WebviewWindow, message: &str) {
    let script = format!(
        "(function f() {{ if (window.showError) window.showError({}); else setTimeout(f, 100); }})()",
        js_string(message)
    );
    let _ = window.eval(&script);
}

fn js_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c == '\u{2028}' || c == '\u{2029}' => {
                out.push_str(&format!("\\u{:04x}", c as u32))
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn stop_backend(app: &tauri::AppHandle) {
    if let Some(mut child) = app.state::<Backend>().0.lock().unwrap().take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}
