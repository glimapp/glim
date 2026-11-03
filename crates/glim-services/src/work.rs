//! Cancellation and fixed working buffers for streaming reader operations.
pub mod scheduler;
use anyhow::Result;
use std::{
    io::{Read, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Debug)]
pub struct Cancelled;
impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Reading canceled")
    }
}
impl std::error::Error for Cancelled {}

#[derive(Clone)]
pub struct Cancellation(Arc<Vec<Arc<AtomicBool>>>);
impl Default for Cancellation {
    fn default() -> Self {
        Self(Arc::new(vec![Arc::new(AtomicBool::new(false))]))
    }
}
pub struct CancelOnDrop(Cancellation);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.0[0].store(true, Ordering::Relaxed);
    }
}
impl Cancellation {
    pub fn from_flag(flag: Arc<AtomicBool>) -> Self {
        Self(Arc::new(vec![flag]))
    }

    pub fn task() -> (Self, CancelOnDrop) {
        let token = Self::default();
        (token.clone(), CancelOnDrop(token))
    }
    pub fn linked(&self, other: &Self) -> Self {
        Self(Arc::new(
            self.0.iter().chain(other.0.iter()).cloned().collect(),
        ))
    }
    pub fn check(&self) -> Result<()> {
        if self.0.iter().any(|flag| flag.load(Ordering::Relaxed)) {
            return Err(Cancelled.into());
        }
        Ok(())
    }
    pub fn copy(&self, mut input: impl Read, mut output: impl Write) -> Result<u64> {
        let mut buffer = vec![0; crate::limits::IO_BUFFER_BYTES];
        let mut total = 0;
        loop {
            self.check()?;
            let read = input.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            output.write_all(&buffer[..read])?;
            total += read as u64;
        }
        Ok(total)
    }
}

pub struct Reader<R> {
    pub inner: R,
    pub cancel: Cancellation,
}
impl<R: Read> Read for Reader<R> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.cancel.check().map_err(std::io::Error::other)?;
        self.inner.read(bytes)
    }
}
impl<R: std::io::Seek> std::io::Seek for Reader<R> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.cancel.check().map_err(std::io::Error::other)?;
        self.inner.seek(pos)
    }
}

pub fn run(
    command: &mut std::process::Command,
    cancel: &Cancellation,
) -> Result<std::process::ExitStatus> {
    let mut child = command.spawn()?;
    loop {
        if let Err(error) = cancel.check() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Limit one active parser record, resetting after each completed record.
pub struct RecordReader<R> {
    pub inner: R,
    remaining: usize,
    cancel: Cancellation,
}
impl<R> RecordReader<R> {
    pub fn new(inner: R, bytes: usize, cancel: &Cancellation) -> Self {
        Self {
            inner,
            remaining: bytes,
            cancel: cancel.clone(),
        }
    }
    pub fn reset(&mut self, bytes: usize) {
        self.remaining = bytes;
    }
}
impl<R: Read> Read for RecordReader<R> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        self.cancel.check().map_err(std::io::Error::other)?;
        if self.remaining == 0 {
            return Err(std::io::Error::other(
                "Active parser record exceeds the working buffer budget",
            ));
        }
        let size = output.len().min(self.remaining);
        let read = self.inner.read(&mut output[..size])?;
        self.remaining -= read;
        Ok(read)
    }
}

pub struct SwitchReader<R> {
    pub inner: R,
    pub cancel: Arc<std::sync::Mutex<Cancellation>>,
}
impl<R: Read> Read for SwitchReader<R> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.cancel
            .lock()
            .map_err(|_| std::io::Error::other("Reader canceled"))?
            .check()
            .map_err(std::io::Error::other)?;
        self.inner.read(bytes)
    }
}
