//! SFTP v3 over the system OpenSSH client.
use super::Connection;
use crate::{limits, work::Cancellation};
use anyhow::{Context, Result, bail, ensure};
use glim_core::DirectoryEntry;
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

pub(super) struct Sftp {
    child: Arc<Mutex<Child>>,
    input: ChildStdin,
    output: ChildStdout,
    errors: Arc<Mutex<Vec<u8>>>,
    next_id: u32,
    healthy: Arc<AtomicBool>,
    io_activity: Arc<Mutex<Option<Instant>>>,
    timed_out: Arc<AtomicBool>,
    process_watch: Option<Watch>,
}

#[derive(Debug)]
struct StatusError {
    code: u32,
    message: String,
}
impl std::fmt::Display for StatusError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "SFTP: {} (status {})", self.message, self.code)
    }
}
impl std::error::Error for StatusError {}
impl Drop for Sftp {
    fn drop(&mut self) {
        self.process_watch = None;
        if let Ok(mut child) = self.child.lock() {
            terminate(&mut child);
            let _ = child.wait();
        }
    }
}
fn terminate(child: &mut Child) {
    #[cfg(unix)]
    let _ = std::process::Command::new("/bin/kill")
        .args(["-TERM", "--", &format!("-{}", child.id())])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    let _ = child.kill();
}

pub(super) struct Watch {
    done: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Watch {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            thread.thread().unpark();
            let _ = thread.join();
        }
    }
}
impl Sftp {
    pub fn connect(connection: &Connection, cancel: &Cancellation) -> Result<Self> {
        Self::connect_mode(connection, cancel, false)
    }
    pub(super) fn connect_background(
        connection: &Connection,
        cancel: &Cancellation,
    ) -> Result<Self> {
        Self::connect_mode(connection, cancel, true)
    }
    fn connect_mode(
        connection: &Connection,
        cancel: &Cancellation,
        background: bool,
    ) -> Result<Self> {
        cancel.check()?;
        let mut command = connection.ssh_command()?;
        if background {
            command.args(["-o", "BatchMode=yes"]);
        }
        command.arg("-s").arg(&connection.host).arg("sftp");
        let mut child = command
            .spawn()
            .context("Cannot start the macOS SSH client")?;
        let input = child.stdin.take().context("SSH input unavailable")?;
        let output = child.stdout.take().context("SSH output unavailable")?;
        let mut stderr = child.stderr.take().context("SSH diagnostics unavailable")?;
        let errors = Arc::new(Mutex::new(Vec::new()));
        let capture = errors.clone();
        std::thread::spawn(move || {
            let mut bytes = [0; 1024];
            while let Ok(size) = stderr.read(&mut bytes) {
                if size == 0 {
                    break;
                }
                if let Ok(mut output) = capture.lock() {
                    output.extend_from_slice(&bytes[..size]);
                    let excess = output.len().saturating_sub(limits::REMOTE_DIAGNOSTIC_BYTES);
                    output.drain(..excess);
                }
            }
        });
        let mut session = Self {
            child: Arc::new(Mutex::new(child)),
            input,
            output,
            errors,
            next_id: 0,
            healthy: Arc::new(AtomicBool::new(true)),
            io_activity: Arc::new(Mutex::new(None)),
            timed_out: Arc::new(AtomicBool::new(false)),
            process_watch: None,
        };
        let _watch = session.watch(cancel);
        session.send(&[1, 0, 0, 0, 3])?;
        let packet = session.receive()?;
        let mut packet = Packet::new(&packet);
        ensure!(
            packet.byte()? == 2 && packet.u32()? == 3,
            "The server does not support SFTP v3"
        );
        session.process_watch = Some(session.watch_process());
        Ok(session)
    }
    pub fn healthy(&self) -> bool {
        self.healthy.load(Ordering::Relaxed)
    }
    fn watch_process(&self) -> Watch {
        let done = Arc::new(AtomicBool::new(false));
        let stop = done.clone();
        let child = self.child.clone();
        let healthy = self.healthy.clone();
        let thread = std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let alive = child
                    .lock()
                    .is_ok_and(|mut child| matches!(child.try_wait(), Ok(None)));
                if !alive {
                    healthy.store(false, Ordering::Relaxed);
                    break;
                }
                std::thread::park_timeout(Duration::from_millis(250));
            }
        });
        Watch {
            done,
            thread: Some(thread),
        }
    }
    pub(super) fn watch(&self, cancel: &Cancellation) -> Watch {
        let done = Arc::new(AtomicBool::new(false));
        let stop = done.clone();
        let cancel = cancel.clone();
        let child = self.child.clone();
        let healthy = self.healthy.clone();
        let io_activity = self.io_activity.clone();
        let timed_out = self.timed_out.clone();
        let thread = std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let stalled = io_activity.lock().is_ok_and(|activity| {
                    activity.is_some_and(|last| {
                        last.elapsed() >= Duration::from_millis(limits::REMOTE_IDLE_TIMEOUT_MS)
                    })
                });
                if cancel.check().is_err() || stalled {
                    timed_out.store(stalled, Ordering::Relaxed);
                    healthy.store(false, Ordering::Relaxed);
                    if let Ok(mut child) = child.lock() {
                        terminate(&mut child);
                    }
                    break;
                }
                std::thread::park_timeout(Duration::from_millis(20));
            }
        });
        Watch {
            done,
            thread: Some(thread),
        }
    }
    fn mark_io(&self, pending: bool) {
        if let Ok(mut activity) = self.io_activity.lock() {
            *activity = pending.then(Instant::now);
        }
    }
    fn send(&mut self, bytes: &[u8]) -> Result<()> {
        self.mark_io(true);
        self.input
            .write_all(&u32::try_from(bytes.len())?.to_be_bytes())?;
        self.mark_io(true);
        let mut remaining = bytes;
        while !remaining.is_empty() {
            let written = self.input.write(remaining)?;
            ensure!(written > 0, "SSH connection closed while sending a request");
            remaining = &remaining[written..];
            self.mark_io(true);
        }
        self.input.flush()?;
        Ok(())
    }
    fn read_packet_bytes(&mut self, mut bytes: &mut [u8]) -> Result<()> {
        while !bytes.is_empty() {
            let count = self.output.read(bytes);
            if self.timed_out.load(Ordering::Relaxed) {
                bail!(
                    "{}",
                    crate::locale::tr("SSH connection timed out; reconnect to continue")
                );
            }
            match count {
                Ok(0) => bail!("SSH connection closed"),
                Ok(count) => {
                    bytes = &mut bytes[count..];
                    self.mark_io(true);
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
    fn receive(&mut self) -> Result<Vec<u8>> {
        let mut size = [0; 4];
        if let Err(error) = self.read_packet_bytes(&mut size) {
            let diagnostic = self
                .errors
                .lock()
                .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_owned())
                .unwrap_or_default();
            bail!("SSH connection closed: {diagnostic} ({error})");
        }
        let size = u32::from_be_bytes(size) as usize;
        ensure!(
            size > 0 && size <= limits::REMOTE_PACKET_BYTES,
            "SFTP response exceeds the active packet budget"
        );
        let mut bytes = vec![0; size];
        self.read_packet_bytes(&mut bytes)?;
        self.mark_io(false);
        Ok(bytes)
    }
    fn request(&mut self, kind: u8, body: &[u8]) -> Result<Vec<u8>> {
        self.next_id = self.next_id.wrapping_add(1);
        let mut request = vec![kind];
        request.extend_from_slice(&self.next_id.to_be_bytes());
        request.extend_from_slice(body);
        let response = self.send(&request).and_then(|_| self.receive());
        if response.is_err() {
            self.healthy.store(false, Ordering::Relaxed);
        }
        let reply = response?;
        let mut packet = Packet::new(&reply);
        let kind = packet.byte()?;
        ensure!(packet.u32()? == self.next_id, "Unexpected SFTP response");
        if kind == 101 {
            let status = packet.u32()?;
            if status != 0 && status != 1 {
                let message = String::from_utf8_lossy(packet.string()?).into_owned();
                return Err(StatusError {
                    code: status,
                    message,
                }
                .into());
            }
        }
        Ok(reply)
    }
    fn expect_ok(reply: &[u8]) -> Result<()> {
        let mut packet = Packet::body(reply, 101)?;
        ensure!(packet.u32()? == 0, "SFTP operation did not complete");
        Ok(())
    }
    pub(super) fn mkdir(&mut self, path: &Path, cancel: &Cancellation) -> Result<()> {
        cancel.check()?;
        let _watch = self.watch(cancel);
        let mut body = string(path.as_os_str().as_encoded_bytes())?;
        body.extend_from_slice(&4u32.to_be_bytes()); // SSH_FILEXFER_ATTR_PERMISSIONS
        body.extend_from_slice(&0o700u32.to_be_bytes());
        Self::expect_ok(&self.request(14, &body)?)
    }
    pub(super) fn create_file(&mut self, path: &Path, cancel: &Cancellation) -> Result<()> {
        cancel.check()?;
        let _watch = self.watch(cancel);
        let handle = self.open_exclusive(path)?;
        self.close_handle(&handle)
    }
    pub(super) fn open_exclusive(&mut self, path: &Path) -> Result<Vec<u8>> {
        let mut body = string(path.as_os_str().as_encoded_bytes())?;
        body.extend_from_slice(&(2u32 | 8 | 32).to_be_bytes()); // WRITE | CREAT | EXCL
        body.extend_from_slice(&4u32.to_be_bytes());
        body.extend_from_slice(&0o600u32.to_be_bytes());
        let reply = self.request(3, &body)?;
        Ok(Packet::body(&reply, 102)?.string()?.to_vec())
    }
    pub(super) fn close_handle(&mut self, handle: &[u8]) -> Result<()> {
        Self::expect_ok(&self.request(4, &string(handle)?)?)
    }
    pub(super) fn check_writable(&mut self, path: &Path, cancel: &Cancellation) -> Result<()> {
        cancel.check()?;
        let _watch = self.watch(cancel);
        let mut body = string(path.as_os_str().as_encoded_bytes())?;
        body.extend_from_slice(&2u32.to_be_bytes()); // WRITE, without CREAT or TRUNC
        body.extend_from_slice(&0u32.to_be_bytes());
        let reply = self.request(3, &body)?;
        self.close_handle(Packet::body(&reply, 102)?.string()?)
    }
    pub(super) fn upload(
        &mut self,
        handle: &[u8],
        bytes: &[u8],
        cancel: &Cancellation,
    ) -> Result<()> {
        let mut offset = 0u64;
        for chunk in bytes.chunks(limits::REMOTE_TRANSFER_BYTES as usize) {
            cancel.check()?;
            let mut body = string(handle)?;
            body.extend_from_slice(&offset.to_be_bytes());
            body.extend_from_slice(&string(chunk)?);
            Self::expect_ok(&self.request(6, &body)?)?;
            offset += chunk.len() as u64;
        }
        Ok(())
    }
    pub(super) fn copy_save_attributes(
        &mut self,
        path: &Path,
        source: &super::FileStamp,
        temporary: &super::FileStamp,
    ) -> Result<()> {
        let mut body = string(path.as_os_str().as_encoded_bytes())?;
        let ownership = source.owner.filter(|_| source.owner != temporary.owner);
        body.extend_from_slice(&(4u32 | if ownership.is_some() { 2 } else { 0 }).to_be_bytes());
        if let Some((uid, gid)) = ownership {
            body.extend_from_slice(&uid.to_be_bytes());
            body.extend_from_slice(&gid.to_be_bytes());
        }
        body.extend_from_slice(&(source.mode & 0o7777).to_be_bytes());
        Self::expect_ok(&self.request(9, &body)?)
    }
    pub(super) fn replace_file(&mut self, source: &Path, destination: &Path) -> Result<()> {
        let mut body = string(b"posix-rename@openssh.com")?;
        body.extend_from_slice(&string(source.as_os_str().as_encoded_bytes())?);
        body.extend_from_slice(&string(destination.as_os_str().as_encoded_bytes())?);
        Self::expect_ok(&self.request(200, &body)?)
            .context("Remote server could not atomically replace the file")
    }
    pub(super) fn remove_save_file(&mut self, path: &Path) -> Result<()> {
        Self::expect_ok(&self.request(13, &string(path.as_os_str().as_encoded_bytes())?)?)
    }
    pub(super) fn rename(
        &mut self,
        source: &Path,
        destination: &Path,
        cancel: &Cancellation,
    ) -> Result<()> {
        cancel.check()?;
        let _watch = self.watch(cancel);
        let mut body = string(source.as_os_str().as_encoded_bytes())?;
        body.extend_from_slice(&string(destination.as_os_str().as_encoded_bytes())?);
        // v3 RENAME refuses replacement; do not use posix-rename's overwrite semantics.
        Self::expect_ok(&self.request(18, &body)?)
    }

    pub fn realpath(&mut self, path: &Path, cancel: &Cancellation) -> Result<PathBuf> {
        cancel.check()?;
        let _watch = self.watch(cancel);
        let reply = self.request(16, &string(path.as_os_str().as_encoded_bytes())?)?;
        let mut packet = Packet::body(&reply, 104)?;
        ensure!(packet.u32()? == 1, "Invalid canonical SFTP path");
        Ok(PathBuf::from(crate::platform::decode_path_bytes(
            packet.string()?.to_vec(),
        )?))
    }
    pub fn stat(&mut self, path: &Path, cancel: &Cancellation) -> Result<super::FileStamp> {
        self.read_stamp(path, 17, cancel)
    }
    fn read_stamp(
        &mut self,
        path: &Path,
        kind: u8,
        cancel: &Cancellation,
    ) -> Result<super::FileStamp> {
        cancel.check()?;
        let _watch = self.watch(cancel);
        let reply = self.request(kind, &string(path.as_os_str().as_encoded_bytes())?)?;
        Packet::body(&reply, 105)?.file_stamp()
    }
    pub(super) fn lstat_optional(
        &mut self,
        path: &Path,
        cancel: &Cancellation,
    ) -> Result<Option<super::FileStamp>> {
        self.optional_stamp(path, 7, cancel)
    }
    pub fn stat_optional(
        &mut self,
        path: &Path,
        cancel: &Cancellation,
    ) -> Result<Option<super::FileStamp>> {
        self.optional_stamp(path, 17, cancel)
    }
    fn optional_stamp(
        &mut self,
        path: &Path,
        kind: u8,
        cancel: &Cancellation,
    ) -> Result<Option<super::FileStamp>> {
        match self.read_stamp(path, kind, cancel) {
            Ok(stamp) => Ok(Some(stamp)),
            Err(error)
                if error
                    .downcast_ref::<StatusError>()
                    .is_some_and(|status| status.code == 2) =>
            {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }
    pub fn list(&mut self, path: &Path, cancel: &Cancellation) -> Result<Vec<DirectoryEntry>> {
        let mut entries = Vec::new();
        self.visit_directory(path, cancel, |entry| {
            entries.push(entry);
            Ok(())
        })?;
        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.path.file_name().cmp(&b.path.file_name()))
        });
        Ok(entries)
    }
    pub(super) fn visit_directory(
        &mut self,
        path: &Path,
        cancel: &Cancellation,
        mut visit: impl FnMut(DirectoryEntry) -> Result<()>,
    ) -> Result<()> {
        cancel.check()?;
        let _watch = self.watch(cancel);
        let reply = self.request(11, &string(path.as_os_str().as_encoded_bytes())?)?;
        let handle = Packet::body(&reply, 102)?.string()?.to_vec();
        let result = self.read_directory(path, &handle, cancel, &mut visit);
        let close = self.request(4, &string(&handle)?);
        result?;
        close?;
        Ok(())
    }
    fn read_directory(
        &mut self,
        path: &Path,
        handle: &[u8],
        cancel: &Cancellation,
        visit: &mut impl FnMut(DirectoryEntry) -> Result<()>,
    ) -> Result<()> {
        loop {
            cancel.check()?;
            let reply = self.request(12, &string(handle)?)?;
            if eof(&reply) {
                break;
            }
            let mut packet = Packet::body(&reply, 104)?;
            let count = packet.u32()?;
            for _ in 0..count {
                cancel.check()?;
                let name = packet.string()?.to_vec();
                packet.string()?;
                let mode = packet.attrs()?;
                if name == b"." || name == b".." {
                    continue;
                }
                ensure!(
                    !name.is_empty() && !name.contains(&b'/') && !name.contains(&0),
                    "Invalid SFTP filename"
                );
                let target = path.join(crate::platform::decode_path_bytes(name)?);
                let symlink = mode & 0o170000 == 0o120000;
                let mode = if symlink {
                    self.request(17, &string(target.as_os_str().as_encoded_bytes())?)
                        .ok()
                        .and_then(|reply| Packet::body(&reply, 105).ok()?.attrs().ok())
                        .unwrap_or(mode)
                } else {
                    mode
                };
                visit(DirectoryEntry {
                    path: target,
                    is_dir: mode & 0o170000 == 0o040000,
                    is_symlink: symlink,
                })?;
            }
        }
        Ok(())
    }
    pub(super) fn read_chunks(
        &mut self,
        path: &Path,
        cancel: &Cancellation,
        mut consume: impl FnMut(&[u8]) -> Result<bool>,
    ) -> Result<bool> {
        cancel.check()?;
        let _watch = self.watch(cancel);
        let mut open = string(path.as_os_str().as_encoded_bytes())?;
        open.extend_from_slice(&1u32.to_be_bytes()); // SSH_FXF_READ
        open.extend_from_slice(&0u32.to_be_bytes());
        let reply = self.request(3, &open)?;
        let handle = Packet::body(&reply, 102)?.string()?.to_vec();
        let result = (|| {
            let mut offset = 0u64;
            loop {
                cancel.check()?;
                let mut read = string(&handle)?;
                read.extend_from_slice(&offset.to_be_bytes());
                read.extend_from_slice(&limits::REMOTE_TRANSFER_BYTES.to_be_bytes());
                let reply = self.request(5, &read)?;
                if eof(&reply) {
                    return Ok(true);
                }
                let mut packet = Packet::body(&reply, 103)?;
                let bytes = packet.string()?;
                ensure!(
                    !bytes.is_empty(),
                    "The SFTP server returned an empty data block"
                );
                if !consume(bytes)? {
                    return Ok(false);
                }
                offset = offset
                    .checked_add(bytes.len() as u64)
                    .context("Remote file offset overflow")?;
            }
        })();
        let close = self.request(4, &string(&handle)?);
        let complete = result?;
        close?;
        Ok(complete)
    }
    pub fn download(
        &mut self,
        path: &Path,
        target: &Path,
        received: &AtomicU64,
        cancel: &Cancellation,
    ) -> Result<()> {
        let mut file =
            tempfile::NamedTempFile::new_in(target.parent().context("Missing cache directory")?)?;
        let mut offset = 0;
        self.read_chunks(path, cancel, |bytes| {
            file.write_all(bytes)?;
            offset += bytes.len() as u64;
            received.store(offset, Ordering::Relaxed);
            Ok(true)
        })?;
        cancel.check()?;
        let mut permissions = file.as_file().metadata()?.permissions();
        permissions.set_readonly(true);
        file.as_file().set_permissions(permissions)?;
        file.persist(target).map_err(|error| error.error)?;
        Ok(())
    }
}
fn eof(reply: &[u8]) -> bool {
    reply.first() == Some(&101) && reply.get(5..9) == Some(&1u32.to_be_bytes())
}
fn string(value: &[u8]) -> Result<Vec<u8>> {
    let mut bytes = u32::try_from(value.len())?.to_be_bytes().to_vec();
    bytes.extend_from_slice(value);
    Ok(bytes)
}
struct Packet<'a> {
    bytes: &'a [u8],
}
impl<'a> Packet<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }
    fn body(bytes: &'a [u8], kind: u8) -> Result<Self> {
        let mut packet = Self::new(bytes);
        ensure!(packet.byte()? == kind, "Unexpected SFTP packet type");
        packet.u32()?;
        Ok(packet)
    }
    fn take(&mut self, size: usize) -> Result<&'a [u8]> {
        ensure!(size <= self.bytes.len(), "Truncated SFTP packet");
        let (value, rest) = self.bytes.split_at(size);
        self.bytes = rest;
        Ok(value)
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into()?))
    }
    fn string(&mut self) -> Result<&'a [u8]> {
        let size = self.u32()? as usize;
        self.take(size)
    }
    fn attrs(&mut self) -> Result<u32> {
        Ok(self.file_stamp()?.mode)
    }
    fn file_stamp(&mut self) -> Result<super::FileStamp> {
        let flags = self.u32()?;
        let size = if flags & 1 != 0 {
            Some(u64::from_be_bytes(self.take(8)?.try_into()?))
        } else {
            None
        };
        let owner = if flags & 2 != 0 {
            Some((self.u32()?, self.u32()?))
        } else {
            None
        };
        let mode = if flags & 4 != 0 { self.u32()? } else { 0 };
        let modified = if flags & 8 != 0 {
            self.u32()?;
            Some(self.u32()?)
        } else {
            None
        };
        if flags & 0x80000000 != 0 {
            for _ in 0..self.u32()? {
                self.string()?;
                self.string()?;
            }
        }
        Ok(super::FileStamp {
            size,
            modified,
            mode,
            owner,
        })
    }
}

#[cfg(all(test, target_os = "macos"))]
impl Sftp {
    pub(super) fn local_test(directory: &Path) -> Result<Self> {
        use std::{
            os::unix::process::CommandExt,
            process::{Command, Stdio},
        };
        let mut child = Command::new("/usr/libexec/sftp-server")
            .current_dir(directory)
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let mut session = Self {
            input: child.stdin.take().unwrap(),
            output: child.stdout.take().unwrap(),
            child: Arc::new(Mutex::new(child)),
            errors: Default::default(),
            next_id: 0,
            healthy: Arc::new(AtomicBool::new(true)),
            io_activity: Default::default(),
            timed_out: Default::default(),
            process_watch: None,
        };
        session.send(&[1, 0, 0, 0, 3])?;
        let reply = session.receive()?;
        let mut packet = Packet::new(&reply);
        ensure!(
            packet.byte()? == 2 && packet.u32()? == 3,
            "Expected SFTP v3"
        );
        Ok(session)
    }
}
