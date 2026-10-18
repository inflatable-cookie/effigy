//! Bounded redacted rotating logs. I/O failure changes capture metadata only.
use super::*;
use crate::fs::{DirFd, FileName, Trust};
use crate::root::{Family, StateRoot};
#[derive(Debug)]
pub(crate) struct Log {
    dir: DirFd,
    name: FileName,
    file: std::fs::File,
    written: u64,
    rotate: u64,
    keep: u8,
}
impl Log {
    pub fn open(
        root: &StateRoot,
        run: crate::id::RunId,
        sequence: i64,
        limits: &crate::ctx::Limits,
    ) -> io::Result<Self> {
        let parent = root
            .family(Family::Runs)
            .map_err(io::Error::other)?
            .ok_or_else(|| io::Error::other("runs directory missing"))?;
        let dir = parent
            .child(&run.to_string(), Trust::Private, true)
            .map_err(io::Error::other)?
            .ok_or_else(|| io::Error::other("run directory missing"))?;
        let name = FileName::new(&format!("{sequence}.log")).map_err(io::Error::other)?;
        let (file, _) = dir
            .open_file(
                &name,
                libc::O_WRONLY | libc::O_EXCL,
                Some(0o600),
                Trust::Private,
            )
            .map_err(io::Error::other)?
            .ok_or_else(|| io::Error::other("log missing"))?;
        Ok(Self {
            dir,
            name,
            file,
            written: 0,
            rotate: limits.log_rotate.max(1),
            keep: limits.log_keep,
        })
    }
    pub fn append(&mut self, mut bytes: &[u8]) -> io::Result<()> {
        while !bytes.is_empty() {
            if self.written == self.rotate {
                if self.keep > 0 {
                    let last = FileName::new(&format!("{}.{}", self.name.as_str(), self.keep))
                        .map_err(io::Error::other)?;
                    self.dir.remove(&last, false).map_err(io::Error::other)?;
                    for n in (1..self.keep).rev() {
                        let from = FileName::new(&format!("{}.{}", self.name.as_str(), n))
                            .map_err(io::Error::other)?;
                        let to = FileName::new(&format!("{}.{}", self.name.as_str(), n + 1))
                            .map_err(io::Error::other)?;
                        if self.dir.entry(&from).map_err(io::Error::other)?.is_some() {
                            self.dir.rename(&from, &to).map_err(io::Error::other)?;
                        }
                    }
                    self.dir
                        .rename(
                            &self.name,
                            &FileName::new(&format!("{}.1", self.name.as_str()))
                                .map_err(io::Error::other)?,
                        )
                        .map_err(io::Error::other)?;
                } else {
                    self.dir
                        .remove(&self.name, false)
                        .map_err(io::Error::other)?;
                }
                self.file = self
                    .dir
                    .open_file(
                        &self.name,
                        libc::O_WRONLY | libc::O_EXCL,
                        Some(0o600),
                        Trust::Private,
                    )
                    .map_err(io::Error::other)?
                    .ok_or_else(|| io::Error::other("log missing"))?
                    .0;
                self.written = 0;
            }
            let count = bytes
                .len()
                .min((self.rotate - self.written).min(usize::MAX as u64) as usize);
            self.file.write_all(&bytes[..count])?;
            self.written += count as u64;
            bytes = &bytes[count..];
        }
        Ok(())
    }
}
