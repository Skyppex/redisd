mod cli;

use clap::Parser;
use redis::{Commands, Connection};
use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::process;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use cli::{Cli, Command};

const SOCKET_PATH: &str = "/tmp/redisd.sock";
const PID_FILE: &str = "/tmp/redisd.pid";

#[derive(Serialize, Deserialize, Debug)]
enum Message {
    Connect { url: String, client_pid: u32 },
    Disconnect,
    Kill,
    Keys,
    KeysWithPttl,
    Get { key: String },
    Set { key: String, value: String },
    Pexpire { key: String, ms: u64 },
    Exists { key: String },
    Pttl { key: String },
}

#[derive(Serialize, Deserialize, Debug)]
enum Response {
    Ok(Option<String>),
    Error(String),
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    if std::env::var("REDISD_DAEMON").is_ok() {
        return run_daemon();
    }

    if let Some(cmd) = cli.subcommand {
        match cmd {
            Command::Connect { url } => {
                if !daemon_running() {
                    spawn_daemon()?;
                }

                match send_message(Message::Connect {
                    url: url.to_string(),
                    client_pid: process::id(),
                }) {
                    Ok(Response::Ok(_)) => println!("connected"),
                    Ok(Response::Error(e)) => eprintln!("error: {}", e),
                    Err(e) => eprintln!("error: {}", e),
                }
            }
            Command::Disconnect => {
                if daemon_running() {
                    match send_message(Message::Disconnect) {
                        Ok(Response::Ok(_)) => println!("disconnected"),
                        Ok(Response::Error(e)) => eprintln!("error: {}", e),
                        Err(e) => eprintln!("error: {}", e),
                    }
                }
            }
            Command::Kill => {
                if daemon_running() {
                    let _ = send_message(Message::Kill);
                }
            }
            Command::Keys => {
                if daemon_running() {
                    match send_message(Message::Keys) {
                        Ok(Response::Ok(Some(keys))) => println!("{}", keys),
                        Ok(Response::Ok(None)) => {}
                        Ok(Response::Error(e)) => eprintln!("error: {}", e),
                        Err(e) => eprintln!("error: {}", e),
                    }
                }
            }
            Command::KeysWithPttl => {
                if daemon_running() {
                    match send_message(Message::KeysWithPttl) {
                        Ok(Response::Ok(Some(output))) => println!("{}", output),
                        Ok(Response::Ok(None)) => {}
                        Ok(Response::Error(e)) => eprintln!("error: {}", e),
                        Err(e) => eprintln!("error: {}", e),
                    }
                }
            }
            Command::Get { key } => {
                if daemon_running() {
                    match send_message(Message::Get { key }) {
                        Ok(Response::Ok(Some(val))) => println!("{}", val),
                        Ok(Response::Ok(None)) => println!("(nil)"),
                        Ok(Response::Error(e)) => eprintln!("error: {}", e),
                        Err(e) => eprintln!("error: {}", e),
                    }
                }
            }
            Command::Set { key, value } => {
                if daemon_running() {
                    match send_message(Message::Set { key, value }) {
                        Ok(Response::Ok(_)) => println!("OK"),
                        Ok(Response::Error(e)) => eprintln!("error: {}", e),
                        Err(e) => eprintln!("error: {}", e),
                    }
                }
            }
            Command::Pexpire { key, ms } => {
                if daemon_running() {
                    match send_message(Message::Pexpire { key, ms }) {
                        Ok(Response::Ok(_)) => println!("OK"),
                        Ok(Response::Error(e)) => eprintln!("error: {}", e),
                        Err(e) => eprintln!("error: {}", e),
                    }
                }
            }
            Command::Exists { key } => {
                if daemon_running() {
                    match send_message(Message::Exists { key }) {
                        Ok(Response::Ok(Some(val))) => println!("{}", val),
                        Ok(Response::Ok(None)) => {}
                        Ok(Response::Error(e)) => eprintln!("error: {}", e),
                        Err(e) => eprintln!("error: {}", e),
                    }
                }
            }
            Command::Pttl { key } => {
                if daemon_running() {
                    match send_message(Message::Pttl { key }) {
                        Ok(Response::Ok(Some(val))) => println!("{}", val),
                        Ok(Response::Ok(None)) => {}
                        Ok(Response::Error(e)) => eprintln!("error: {}", e),
                        Err(e) => eprintln!("error: {}", e),
                    }
                }
            }
        }
    }

    Ok(())
}

// fn process_alive(pid: u32) -> bool {
//     Path::new(&format!("/proc/{}", pid)).exists()
// }

fn send_message(msg: Message) -> Result<Response, Box<dyn Error>> {
    let mut stream = UnixStream::connect(SOCKET_PATH)?;
    let json = serde_json::to_string(&msg)?;
    stream.write_all(json.as_bytes())?;
    stream.write_all(b"\n")?;

    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader.read_line(&mut response)?;
    let response: Response = serde_json::from_str(response.trim())?;
    Ok(response)
}

fn daemon_running() -> bool {
    Path::new(SOCKET_PATH).exists() && UnixStream::connect(SOCKET_PATH).is_ok()
}

fn spawn_daemon() -> Result<(), Box<dyn Error>> {
    let exe = std::env::current_exe()?;
    process::Command::new(exe)
        .env("REDISD_DAEMON", "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    for _ in 0..50 {
        if daemon_running() {
            return Ok(());
        }

        thread::sleep(Duration::from_millis(100));
    }
    Err("daemon failed to start".into())
}

fn cleanup() {
    let _ = fs::remove_file(SOCKET_PATH);
    let _ = fs::remove_file(PID_FILE);
}

fn run_daemon() -> Result<(), Box<dyn Error>> {
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler({
        let r = r.clone();
        move || {
            r.store(false, Ordering::SeqCst);
        }
    })?;

    if Path::new(SOCKET_PATH).exists() {
        fs::remove_file(SOCKET_PATH)?;
    }
    if Path::new(PID_FILE).exists() {
        fs::remove_file(PID_FILE)?;
    }

    fs::write(PID_FILE, process::id().to_string())?;

    let listener = UnixListener::bind(SOCKET_PATH)?;
    listener.set_nonblocking(true)?;
    let mut redis_conn: Option<Connection> = None;

    while running.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let mut reader = BufReader::new(&stream);
                let mut buf = String::new();

                if reader.read_line(&mut buf).is_ok()
                    && let Ok(msg) = serde_json::from_str::<Message>(buf.trim())
                {
                    let response = match msg {
                        Message::Connect { url, .. } => match redis::Client::open(url.clone()) {
                            Ok(client) => match client.get_connection() {
                                Ok(conn) => {
                                    redis_conn = Some(conn);
                                    Response::Ok(None)
                                }
                                Err(e) => Response::Error(e.to_string()),
                            },
                            Err(e) => Response::Error(e.to_string()),
                        },
                        Message::Disconnect => {
                            redis_conn = None;
                            Response::Ok(None)
                        }
                        Message::Kill => {
                            running.store(false, Ordering::SeqCst);
                            Response::Ok(None)
                        }
                        Message::Keys => {
                            if let Some(ref mut conn) = redis_conn {
                                let keys: Vec<String> = redis::cmd("KEYS").arg("*").query(conn)?;
                                Response::Ok(Some(keys.join("\n")))
                            } else {
                                Response::Error("not connected".into())
                            }
                        }
                        Message::KeysWithPttl => {
                            if let Some(ref mut conn) = redis_conn {
                                let keys: Vec<String> = redis::cmd("KEYS").arg("*").query(conn)?;
                                if keys.is_empty() {
                                    Response::Ok(None)
                                } else {
                                    let mut pipe = redis::pipe();
                                    for key in &keys {
                                        pipe.pttl(key);
                                    }
                                    let ttls: Vec<i64> = pipe.query(conn)?;
                                    let output: Vec<String> = keys
                                        .iter()
                                        .zip(ttls.iter())
                                        .map(|(k, &t)| format!("{}\t{}", k, t))
                                        .collect();
                                    Response::Ok(Some(output.join("\n")))
                                }
                            } else {
                                Response::Error("not connected".into())
                            }
                        }
                        Message::Get { key } => {
                            if let Some(ref mut conn) = redis_conn {
                                let val: Option<String> = conn.get(&key)?;
                                Response::Ok(val)
                            } else {
                                Response::Error("not connected".into())
                            }
                        }
                        Message::Set { key, value } => {
                            if let Some(ref mut conn) = redis_conn {
                                let _: () = conn.set(&key, &value)?;
                                Response::Ok(None)
                            } else {
                                Response::Error("not connected".into())
                            }
                        }
                        Message::Pexpire { key, ms } => {
                            if let Some(ref mut conn) = redis_conn {
                                let _: () = redis::cmd("PEXPIRE").arg(&key).arg(ms).query(conn)?;
                                Response::Ok(None)
                            } else {
                                Response::Error("not connected".into())
                            }
                        }
                        Message::Exists { key } => {
                            if let Some(ref mut conn) = redis_conn {
                                let val: bool = conn.exists(&key)?;
                                Response::Ok(Some(val.to_string()))
                            } else {
                                Response::Error("not connected".into())
                            }
                        }
                        Message::Pttl { key } => {
                            if let Some(ref mut conn) = redis_conn {
                                let val: i64 = redis::cmd("PTTL").arg(&key).query(conn)?;
                                Response::Ok(Some(val.to_string()))
                            } else {
                                Response::Error("not connected".into())
                            }
                        }
                    };

                    let mut stream = reader.into_inner();
                    if let Ok(json) = serde_json::to_string(&response) {
                        let _ = stream.write_all(json.as_bytes());
                        let _ = stream.write_all(b"\n");
                    }
                }
            }
            Err(_) => {
                thread::sleep(Duration::from_millis(100));
            }
        }
    }

    cleanup();
    Ok(())
}
