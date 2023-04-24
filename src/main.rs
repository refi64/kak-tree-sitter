mod rc;
mod request;

use clap::Parser;
use std::{
  env,
  fs::{self, File},
  io::Write,
  os::unix::net::UnixListener,
  path::PathBuf,
  process::Stdio,
  time::Duration,
};

#[derive(Debug, Parser)]
#[clap(about = "A client/server interface between Kakoune and tree-sitter.")]
pub struct Cli {
  /// Whether we start from Kakoune and then we should inject the rc/.
  #[clap(short, long)]
  kakoune: bool,

  /// Try to daemonize, if not already done.
  #[clap(short, long)]
  daemonize: bool,

  /// Kakoune session to connect to.
  #[clap(short, long)]
  session: Option<String>,

  /// Kakoune client to connect with, if any.
  #[clap(short, long)]
  client: Option<String>,
}

fn main() {
  let cli = Cli::parse();

  if cli.daemonize {
    start_daemon();
  }

  if let Some(session) = cli.session {
    let mut kak_sess = KakSession::new(session, cli.client);

    if cli.kakoune {
      // inject the rc/
      kak_sess.send(rc::rc_commands());
    }
  } else {
    std::process::exit(1);
  }
}

#[derive(Debug)]
pub struct Daemon {
  daemon_dir: PathBuf,
  unix_listener: UnixListener,
}

impl Daemon {
  fn new(daemon_dir: PathBuf) -> Self {
    let unix_listener = UnixListener::bind(daemon_dir.join("socket")).unwrap(); // FIXME: unwrap()

    Self {
      daemon_dir,
      unix_listener,
    }
  }

  // Wait for incoming client and handle their requests.
  fn run(&self) {
    for client in self.unix_listener.incoming() {
      println!("client connected: {client:?}");
    }
  }
}

impl Drop for Daemon {
  fn drop(&mut self) {
    let _ = std::fs::remove_dir_all(&self.daemon_dir);
  }
}

fn start_daemon() {
  // ensure we have a directory to write in
  let tmpdir = PathBuf::from(env::var("TMPDIR").expect("temporary directory"));
  let user = env::var("USER").expect("user");
  let daemon_dir = tmpdir.join(format!("kak-tree-sitter-{}", user));
  fs::create_dir_all(&daemon_dir).unwrap(); // FIXME: error

  // create stdout / stderr files
  let stdout_path = daemon_dir.join("stdout.txt");
  let stderr_path = daemon_dir.join("stderr.txt");
  let stdout = File::create(&stdout_path).unwrap();
  let stderr = File::create(&stderr_path).unwrap();

  // PID file
  let pid_file = daemon_dir.join("pid");

  daemonize::Daemonize::new()
    .stdout(stdout)
    .stderr(stderr)
    .pid_file(pid_file)
    .start()
    .expect("daemon");

  let daemon = Daemon::new(daemon_dir);
  println!("daemon started: {daemon:?}");

  daemon.run();
}

#[derive(Debug)]
struct KakSession {
  session_name: String,
  client_name: Option<String>,
}

impl KakSession {
  fn new(session_name: impl Into<String>, client_name: impl Into<Option<String>>) -> Self {
    Self {
      session_name: session_name.into(),
      client_name: client_name.into(),
    }
  }

  /// Format a command to send to Kakoune.
  ///
  /// If `client_name` exists, it will be added to provide additional context and more commands (like info, etc.).
  fn fmt_cmd(&self, cmd: impl AsRef<str>) -> String {
    let cmd = cmd.as_ref();

    if let Some(ref client_name) = self.client_name {
      format!("eval -client {client_name} '{cmd}'\n")
    } else {
      format!("{}\n", cmd)
    }
  }

  /// FIXME: I’m not entirely sure why but something is off with UnixStream. It’s like we’re not correctly connected with the right address?!
  fn send(&mut self, cmd: impl AsRef<str>) {
    let child = std::process::Command::new("kak")
      .args(["-p", self.session_name.as_str()])
      .stdin(Stdio::piped())
      .spawn()
      .unwrap(); // FIXME: unwrap()
    let mut child_stdin = child.stdin.unwrap(); // FIXME: unwrap()
    child_stdin.write_all(self.fmt_cmd(cmd).as_bytes()).unwrap(); // FIXME: unwrap
    child_stdin.flush().unwrap(); // FIXME: unwrap
  }
}
