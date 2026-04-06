use std::{
    collections::VecDeque,
    io,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use bytes::Bytes;
use chrono::{DateTime, Utc};
use tokio::io::AsyncWrite;
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

#[derive(Debug)]
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
}

impl OutputInner {
    fn push_entry(&mut self, entry: LogEntry) {
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
            })),
        }
    }

    pub fn entries(&self) -> Vec<LogEntry> {
        let inner = self.inner.lock().unwrap();
        let mut entries: Vec<LogEntry> = inner
            .entries
            .iter()
            .map(|e| LogEntry {
                timestamp: e.timestamp,
                stream: match e.stream {
                    LogStream::Stdout => LogStream::Stdout,
                    LogStream::Stderr => LogStream::Stderr,
                    LogStream::System => LogStream::System,
                },
                message: e.message.clone(),
            })
            .collect();
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

#[async_trait]
impl OutputStream for ModuleOutputPipe {
    fn write(&mut self, bytes: Bytes) -> Result<(), StreamError> {
        let mut inner = self.output.inner.lock().unwrap();
        let text = String::from_utf8_lossy(&bytes).into_owned();
        match self.stream {
            LogStream::Stdout => inner.push_stdout(&text),
            LogStream::Stderr | LogStream::System => inner.push_stderr(&text),
        }
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
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let mut inner = self.output.inner.lock().unwrap();
        let text = String::from_utf8_lossy(buf).into_owned();
        match self.stream {
            LogStream::Stdout => inner.push_stdout(&text),
            LogStream::Stderr | LogStream::System => inner.push_stderr(&text),
        }
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
