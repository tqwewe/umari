use std::{
    collections::VecDeque,
    fs::{File, OpenOptions},
    io::{self, Write},
    path::Path,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use bytes::Bytes;
use chrono::{DateTime, Utc};
use tokio::io::AsyncWrite;
use tracing::warn;
use wasmtime_wasi::{
    async_trait,
    cli::{IsTerminal, StdoutStream},
    p2::{OutputStream, Pollable, StreamError},
};

#[derive(Debug, Clone)]
pub enum LogStream {
    Stdout,
    Stderr,
    System,
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub stream: LogStream,
    pub message: String,
}

#[derive(Debug)]
struct OutputInner {
    max_entries: usize,
    stdout_buf: String,
    stderr_buf: String,
    entries: VecDeque<LogEntry>,
    // Optional append-only log file. `None` if no path was supplied or if a
    // write previously failed; the ring buffer remains the source of truth
    // for the UI either way.
    file: Option<File>,
}

impl OutputInner {
    fn push_entry(&mut self, entry: LogEntry) {
        if let Some(file) = self.file.as_mut() {
            let stream = match entry.stream {
                LogStream::Stdout => "stdout",
                LogStream::Stderr => "stderr",
                LogStream::System => "system",
            };
            if let Err(err) = writeln!(
                file,
                "{} [{stream}] {}",
                entry
                    .timestamp
                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                entry.message,
            ) {
                warn!(%err, "failed to write to module log file; disabling file logging for this module");
                self.file = None;
            }
        }
        if self.entries.len() >= self.max_entries {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    fn push_stdout(&mut self, text: &str) {
        self.stdout_buf.push_str(text);
        while let Some(pos) = self.stdout_buf.find('\n') {
            let line = self.stdout_buf[..pos].trim_end_matches('\r').to_owned();
            self.stdout_buf.drain(..=pos);
            self.push_entry(LogEntry {
                timestamp: Utc::now(),
                stream: LogStream::Stdout,
                message: line,
            });
        }
    }

    fn push_stderr(&mut self, text: &str) {
        self.stderr_buf.push_str(text);
        while let Some(pos) = self.stderr_buf.find('\n') {
            let line = self.stderr_buf[..pos].trim_end_matches('\r').to_owned();
            self.stderr_buf.drain(..=pos);
            self.push_entry(LogEntry {
                timestamp: Utc::now(),
                stream: LogStream::Stderr,
                message: line,
            });
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModuleOutput {
    inner: Arc<Mutex<OutputInner>>,
}

impl ModuleOutput {
    pub fn new(max_entries: usize) -> Self {
        ModuleOutput {
            inner: Arc::new(Mutex::new(OutputInner {
                max_entries,
                stdout_buf: String::new(),
                stderr_buf: String::new(),
                entries: VecDeque::new(),
                file: None,
            })),
        }
    }

    pub fn with_file(max_entries: usize, path: &Path) -> Self {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let file = match OpenOptions::new().create(true).append(true).open(path) {
            Ok(f) => Some(f),
            Err(err) => {
                warn!(path = %path.display(), %err, "failed to open module log file; falling back to in-memory only");
                None
            }
        };
        ModuleOutput {
            inner: Arc::new(Mutex::new(OutputInner {
                max_entries,
                stdout_buf: String::new(),
                stderr_buf: String::new(),
                entries: VecDeque::new(),
                file,
            })),
        }
    }

    pub fn entries(&self) -> Vec<LogEntry> {
        let inner = self.inner.lock().unwrap();
        let mut entries: Vec<LogEntry> = inner.entries.iter().cloned().collect();
        if !inner.stdout_buf.is_empty() {
            entries.push(LogEntry {
                timestamp: Utc::now(),
                stream: LogStream::Stdout,
                message: inner.stdout_buf.clone(),
            });
        }
        if !inner.stderr_buf.is_empty() {
            entries.push(LogEntry {
                timestamp: Utc::now(),
                stream: LogStream::Stderr,
                message: inner.stderr_buf.clone(),
            });
        }
        entries
    }

    pub fn push_stderr(&self, message: impl Into<String>) {
        let mut inner = self.inner.lock().unwrap();
        inner.push_entry(LogEntry {
            timestamp: Utc::now(),
            stream: LogStream::Stderr,
            message: message.into(),
        });
    }

    pub fn push_system(&self, message: impl Into<String>) {
        let mut inner = self.inner.lock().unwrap();
        inner.push_entry(LogEntry {
            timestamp: Utc::now(),
            stream: LogStream::System,
            message: message.into(),
        });
    }

    pub fn stdout_pipe(&self) -> ModuleOutputPipe {
        ModuleOutputPipe {
            stream: LogStream::Stdout,
            output: self.clone(),
        }
    }

    pub fn stderr_pipe(&self) -> ModuleOutputPipe {
        ModuleOutputPipe {
            stream: LogStream::Stderr,
            output: self.clone(),
        }
    }
}

#[derive(Clone)]
pub struct ModuleOutputPipe {
    stream: LogStream,
    output: ModuleOutput,
}

impl ModuleOutputPipe {
    fn do_write(&mut self, bytes: &[u8]) {
        let mut inner = self.output.inner.lock().unwrap();
        let text = String::from_utf8_lossy(bytes).into_owned();
        match self.stream {
            LogStream::Stdout => inner.push_stdout(&text),
            LogStream::Stderr | LogStream::System => inner.push_stderr(&text),
        }
    }
}

#[async_trait]
impl OutputStream for ModuleOutputPipe {
    fn write(&mut self, bytes: Bytes) -> Result<(), StreamError> {
        self.do_write(&bytes);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), StreamError> {
        Ok(())
    }

    fn check_write(&mut self) -> Result<usize, StreamError> {
        Ok(64 * 1024)
    }
}

#[async_trait]
impl Pollable for ModuleOutputPipe {
    async fn ready(&mut self) {}
}

impl AsyncWrite for ModuleOutputPipe {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.do_write(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

impl IsTerminal for ModuleOutputPipe {
    fn is_terminal(&self) -> bool {
        false
    }
}

impl StdoutStream for ModuleOutputPipe {
    fn async_stream(&self) -> Box<dyn AsyncWrite + Send + Sync> {
        Box::new(self.clone())
    }

    fn p2_stream(&self) -> Box<dyn OutputStream> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use tempfile::TempDir;
    use wasmtime_wasi::p2::OutputStream;

    use super::{LogStream, ModuleOutput};

    fn write_stdout(output: &ModuleOutput, text: &str) {
        let mut pipe = output.stdout_pipe();
        pipe.write(Bytes::copy_from_slice(text.as_bytes())).unwrap();
    }

    fn messages(output: &ModuleOutput) -> Vec<(LogStream, String)> {
        output
            .entries()
            .into_iter()
            .map(|entry| (entry.stream, entry.message))
            .collect()
    }

    #[test]
    fn stdout_is_split_into_one_entry_per_line() {
        let output = ModuleOutput::new(16);
        write_stdout(&output, "line1\nline2\n");
        let msgs = messages(&output);
        assert_eq!(msgs.len(), 2);
        assert!(matches!(&msgs[0], (LogStream::Stdout, m) if m == "line1"));
        assert!(matches!(&msgs[1], (LogStream::Stdout, m) if m == "line2"));
    }

    #[test]
    fn partial_line_is_buffered_then_committed_on_newline() {
        let output = ModuleOutput::new(16);
        write_stdout(&output, "partial");

        // Uncommitted text still surfaces via `entries()` as a trailing entry.
        let peek = messages(&output);
        assert_eq!(peek.len(), 1);
        assert!(matches!(&peek[0], (LogStream::Stdout, m) if m == "partial"));

        write_stdout(&output, "rest\n");
        let msgs = messages(&output);
        assert_eq!(msgs.len(), 1);
        assert!(matches!(&msgs[0], (LogStream::Stdout, m) if m == "partialrest"));
    }

    #[test]
    fn carriage_return_is_trimmed() {
        let output = ModuleOutput::new(16);
        write_stdout(&output, "line\r\n");
        let msgs = messages(&output);
        assert_eq!(msgs.len(), 1);
        assert!(matches!(&msgs[0], (LogStream::Stdout, m) if m == "line"));
    }

    #[test]
    fn ring_buffer_evicts_oldest_beyond_capacity() {
        let output = ModuleOutput::new(2);
        output.push_system("a");
        output.push_system("b");
        output.push_system("c");
        let msgs: Vec<String> = messages(&output).into_iter().map(|(_, m)| m).collect();
        assert_eq!(msgs, vec!["b".to_string(), "c".to_string()]);
    }

    #[test]
    fn with_file_persists_entries_to_disk() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nested/module.log");
        let output = ModuleOutput::with_file(16, &path);
        output.push_system("system message");
        output.push_stderr("stderr message");

        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("[system] system message"), "{contents}");
        assert!(contents.contains("[stderr] stderr message"), "{contents}");
    }
}
