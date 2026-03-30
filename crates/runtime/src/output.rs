use std::{
    io,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use bytes::Bytes;
use chrono::{DateTime, Utc};
use tokio::io::AsyncWrite;
use wasmtime::format_err;
use wasmtime_wasi::{
    async_trait,
    cli::{IsTerminal, StdoutStream},
    p2::{OutputStream, Pollable, StreamError},
};

#[derive(Debug, Clone)]
pub enum LogStream {
    Stdout,
    Stderr,
}

#[derive(Debug)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub stream: LogStream,
    pub message: String,
}

#[derive(Debug)]
struct OutputInner {
    capacity_bytes: usize,
    total_bytes: usize,
    stdout_buf: String,
    stderr_buf: String,
    entries: Vec<LogEntry>,
}

impl OutputInner {
    fn push_stdout(&mut self, text: &str) {
        self.stdout_buf.push_str(text);
        while let Some(pos) = self.stdout_buf.find('\n') {
            let line = self.stdout_buf[..pos].trim_end_matches('\r').to_owned();
            self.stdout_buf.drain(..=pos);
            self.entries.push(LogEntry {
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
            self.entries.push(LogEntry {
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
    pub fn new(capacity_bytes: usize) -> Self {
        ModuleOutput {
            inner: Arc::new(Mutex::new(OutputInner {
                capacity_bytes,
                total_bytes: 0,
                stdout_buf: String::new(),
                stderr_buf: String::new(),
                entries: Vec::new(),
            })),
        }
    }

    pub fn entries(&self) -> Vec<LogEntry> {
        let inner = self.inner.lock().unwrap();
        inner
            .entries
            .iter()
            .map(|e| LogEntry {
                timestamp: e.timestamp,
                stream: match e.stream {
                    LogStream::Stdout => LogStream::Stdout,
                    LogStream::Stderr => LogStream::Stderr,
                },
                message: e.message.clone(),
            })
            .collect()
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
        if bytes.len() > inner.capacity_bytes - inner.total_bytes {
            return Err(StreamError::Trap(format_err!(
                "write beyond capacity of ModuleOutputPipe"
            )));
        }
        inner.total_bytes += bytes.len();
        let text = String::from_utf8_lossy(&bytes).into_owned();
        match self.stream {
            LogStream::Stdout => inner.push_stdout(&text),
            LogStream::Stderr => inner.push_stderr(&text),
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), StreamError> {
        Ok(())
    }

    fn check_write(&mut self) -> Result<usize, StreamError> {
        let inner = self.output.inner.lock().unwrap();
        if inner.total_bytes < inner.capacity_bytes {
            Ok(inner.capacity_bytes - inner.total_bytes)
        } else {
            Err(StreamError::Closed)
        }
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
        let amt = buf.len().min(inner.capacity_bytes - inner.total_bytes);
        inner.total_bytes += amt;
        let text = String::from_utf8_lossy(&buf[..amt]).into_owned();
        match self.stream {
            LogStream::Stdout => inner.push_stdout(&text),
            LogStream::Stderr => inner.push_stderr(&text),
        }
        Poll::Ready(Ok(amt))
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
