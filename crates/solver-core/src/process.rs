//! One owned incremental SMT process. Every exit path reaps the child and joins its reader.
use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

#[derive(Debug)]
pub enum Failure {
    Cancelled,
    Worker(String),
}

impl From<std::io::Error> for Failure {
    fn from(error: std::io::Error) -> Self {
        Self::Worker(error.to_string())
    }
}

/// An explicit override is authoritative: a bad override must fail, never select another solver.
pub fn executable() -> PathBuf {
    if let Some(path) = std::env::var_os("ASTRA_CVC5") {
        return path.into();
    }
    if let Ok(current) = std::env::current_exe() {
        let path = current.with_file_name(if cfg!(windows) { "cvc5.exe" } else { "cvc5" });
        if path.is_file() {
            return path;
        }
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&paths) {
            let path = directory.join(if cfg!(windows) { "cvc5.exe" } else { "cvc5" });
            if path.is_file() {
                return path;
            }
        }
    }
    // GUI processes may inherit PATH from before cvc5 was installed.
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let path = PathBuf::from(local).join("Programs/cvc5/bin/cvc5.exe");
        if path.is_file() {
            return path;
        }
    }
    PathBuf::from("cvc5")
}

pub struct Session {
    child: Child,
    input: Option<ChildStdin>,
    output: Receiver<Result<String, String>>,
    reader: Option<JoinHandle<()>>,
}

impl Session {
    pub fn new() -> Result<Self, Failure> {
        let path = executable();
        let mut command = Command::new(&path);
        command
            .args(["--lang=smt2", "--incremental", "--produce-models"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = command.spawn().map_err(|e| {
            Failure::Worker(format!(
                "Cannot start cvc5 at {}: {e}. Install cvc5 or set ASTRA_CVC5 to its executable.",
                path.display()
            ))
        })?;
        let input = child.stdin.take();
        let stdout = child.stdout.take().expect("piped stdout");
        let (send, output) = mpsc::channel();
        let reader = thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if send.send(line.map_err(|e| e.to_string())).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            input,
            output,
            reader: Some(reader),
        })
    }

    pub fn write(&mut self, text: &str) -> Result<(), Failure> {
        let input = self.input.as_mut().expect("live process stdin");
        input.write_all(text.as_bytes())?;
        input.flush()?;
        Ok(())
    }

    pub fn response(&self, cancel: &AtomicBool, stop: &AtomicBool) -> Result<String, Failure> {
        let mut response = String::new();
        let mut depth = 0i64;
        loop {
            if cancel.load(Ordering::Relaxed) || stop.load(Ordering::Relaxed) {
                return Err(Failure::Cancelled);
            }
            match self.output.recv_timeout(Duration::from_millis(25)) {
                Ok(Ok(line)) => {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    if line.starts_with("(error") {
                        return Err(Failure::Worker(line.to_owned()));
                    }
                    for byte in line.bytes() {
                        if byte == b'(' {
                            depth += 1;
                        }
                        if byte == b')' {
                            depth -= 1;
                        }
                    }
                    response.push_str(line);
                    response.push(' ');
                    if depth == 0 {
                        return Ok(response.trim().to_owned());
                    }
                }
                Ok(Err(error)) => return Err(Failure::Worker(error)),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(Failure::Worker("cvc5 exited before answering".to_owned()));
                }
            }
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Killing is cleanup, never an UNSAT result. wait and join are included in solve time.
        let _ = self.child.kill();
        drop(self.input.take());
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_errors_are_failures_and_cancel_does_not_mean_unsat() {
        let mut session = Session::new().unwrap();
        session
            .write("(set-logic QF_LRA)\n(assert undeclared)\n")
            .unwrap();
        assert!(matches!(
            session.response(&AtomicBool::new(false), &AtomicBool::new(false)),
            Err(Failure::Worker(_))
        ));
        drop(session);
        let mut session = Session::new().unwrap();
        session.write("(set-logic QF_LRA)\n(check-sat)\n").unwrap();
        assert!(matches!(
            session.response(&AtomicBool::new(true), &AtomicBool::new(false)),
            Err(Failure::Cancelled)
        ));
        // Drop kills and reaps even when stdout contains an unread answer.
        drop(session);
    }
}
