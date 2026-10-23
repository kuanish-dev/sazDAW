//! `MemoryStream`: an in-memory byte stream (`IBStream`) through which the plugin gives and takes its state
//! (`getState`/`setState`). The read, write and seek logic is in the pure `Cursor`; the COM wrapper only translates pointers.

use std::sync::Mutex;

use daw_contracts::sync::lock;
use vst3::Steinberg::IBStream_::{IStreamSeekMode, IStreamSeekMode_};
use vst3::Steinberg::{IBStream, IBStreamTrait, kInvalidArgument, kResultOk, tresult};
use vst3::{Class, ComWrapper};

/// Bytes and the read/write position.
#[derive(Default)]
struct Cursor {
    bytes: Vec<u8>,
    position: usize,
}

impl Cursor {
    fn read(&mut self, out: &mut [u8]) -> usize {
        let available = self.bytes.len().saturating_sub(self.position);
        let count = out.len().min(available);
        out[..count].copy_from_slice(&self.bytes[self.position..self.position + count]);
        self.position += count;
        count
    }

    /// Writes from the current position; a position past the end (after a seek) is padded with zeros.
    fn write(&mut self, data: &[u8]) {
        let end = self.position + data.len();
        if self.bytes.len() < end {
            self.bytes.resize(end, 0);
        }
        self.bytes[self.position..end].copy_from_slice(data);
        self.position = end;
    }

    /// The new position; a negative one gives `None` (the position does not change).
    fn seek(&mut self, offset: i64, mode: IStreamSeekMode) -> Option<i64> {
        let base = match mode {
            IStreamSeekMode_::kIBSeekSet => 0,
            IStreamSeekMode_::kIBSeekCur => i64::try_from(self.position).ok()?,
            IStreamSeekMode_::kIBSeekEnd => i64::try_from(self.bytes.len()).ok()?,
            _ => return None,
        };
        let target = base.checked_add(offset)?;
        self.position = usize::try_from(target).ok()?;
        Some(target)
    }
}

pub struct MemoryStream {
    cursor: Mutex<Cursor>,
}

impl Class for MemoryStream {
    type Interfaces = (IBStream,);
}

impl MemoryStream {
    /// An empty stream: the plugin writes its state into it.
    pub fn empty() -> ComWrapper<Self> {
        Self::with(Vec::new())
    }

    /// A stream with ready bytes (position at the start): the plugin reads its state from it.
    pub fn with(bytes: Vec<u8>) -> ComWrapper<Self> {
        ComWrapper::new(Self { cursor: Mutex::new(Cursor { bytes, position: 0 }) })
    }

    pub fn bytes(&self) -> Vec<u8> {
        lock(&self.cursor).bytes.clone()
    }
}

impl IBStreamTrait for MemoryStream {
    unsafe fn read(&self, buffer: *mut std::ffi::c_void, num_bytes: i32, num_bytes_read: *mut i32) -> tresult {
        let Some(count) = usize::try_from(num_bytes).ok().filter(|_| !buffer.is_null()) else {
            return kInvalidArgument;
        };
        // SAFETY: the plugin passed a non-null buffer of `num_bytes` bytes, alive for the duration of the call.
        let out = unsafe { std::slice::from_raw_parts_mut(buffer.cast::<u8>(), count) };
        let read = lock(&self.cursor).read(out);
        if !num_bytes_read.is_null() {
            // SAFETY: a non-null plugin pointer to one number.
            unsafe { *num_bytes_read = read as i32 };
        }
        kResultOk
    }

    unsafe fn write(&self, buffer: *mut std::ffi::c_void, num_bytes: i32, num_bytes_written: *mut i32) -> tresult {
        let Some(count) = usize::try_from(num_bytes).ok().filter(|_| !buffer.is_null()) else {
            return kInvalidArgument;
        };
        // SAFETY: the plugin passed a non-null buffer of `num_bytes` bytes, alive for the duration of the call; we only read.
        let data = unsafe { std::slice::from_raw_parts(buffer.cast::<u8>().cast_const(), count) };
        lock(&self.cursor).write(data);
        if !num_bytes_written.is_null() {
            // SAFETY: a non-null plugin pointer to one number.
            unsafe { *num_bytes_written = num_bytes };
        }
        kResultOk
    }

    unsafe fn seek(&self, pos: i64, mode: i32, result: *mut i64) -> tresult {
        let Some(position) = IStreamSeekMode::try_from(mode).ok().and_then(|mode| lock(&self.cursor).seek(pos, mode)) else {
            return kInvalidArgument;
        };
        if !result.is_null() {
            // SAFETY: a non-null plugin pointer to one number.
            unsafe { *result = position };
        }
        kResultOk
    }

    unsafe fn tell(&self, pos: *mut i64) -> tresult {
        if pos.is_null() {
            return kInvalidArgument;
        }
        let position = i64::try_from(lock(&self.cursor).position).unwrap_or(i64::MAX);
        // SAFETY: a non-null plugin pointer to one number.
        unsafe { *pos = position };
        kResultOk
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_reads_writes_and_seeks_like_a_file() {
        let mut cursor = Cursor::default();
        cursor.write(b"hello");
        assert_eq!(cursor.seek(0, IStreamSeekMode_::kIBSeekSet), Some(0));
        let mut out = [0u8; 3];
        assert_eq!(cursor.read(&mut out), 3);
        assert_eq!(&out, b"hel");
        assert_eq!(cursor.seek(-1, IStreamSeekMode_::kIBSeekEnd), Some(4));
        let mut rest = [0u8; 8];
        assert_eq!(cursor.read(&mut rest), 1, "at the end, reads as much as there is");
        assert_eq!(cursor.seek(-10, IStreamSeekMode_::kIBSeekCur), None, "cannot go before the start");
        assert_eq!(cursor.seek(7, IStreamSeekMode_::kIBSeekSet), Some(7));
        cursor.write(b"!");
        assert_eq!(cursor.bytes, b"hello\0\0!", "a write past the end pads with zeros");
    }

    /// Tested through the real COM interface, as the plugin sees it.
    #[test]
    #[allow(
        clippy::multiple_unsafe_ops_per_block,
        reason = "the test calls methods of our own COM object in a row: the same safety condition holds for all calls"
    )]
    fn plugin_writes_and_reads_back_through_the_com_stream() {
        let stream = MemoryStream::empty();
        let com = stream.as_com_ref::<IBStream>().expect("stream interface");
        let mut data = *b"state";
        let mut written = 0;
        let mut position = -1;
        let mut back = [0u8; 5];
        let mut read = 0;
        // SAFETY: buffers and numbers are local variables of the right size; the object is alive.
        unsafe {
            assert_eq!(com.write(data.as_mut_ptr().cast(), 5, &raw mut written), kResultOk);
            assert_eq!(com.tell(&raw mut position), kResultOk);
            assert_eq!(com.seek(0, IStreamSeekMode_::kIBSeekSet as i32, std::ptr::null_mut()), kResultOk);
            assert_eq!(com.read(back.as_mut_ptr().cast(), 5, &raw mut read), kResultOk);
            assert_eq!(com.read(std::ptr::null_mut(), 5, std::ptr::null_mut()), kInvalidArgument);
        }
        assert_eq!((written, position, read), (5, 5, 5));
        assert_eq!(&back, b"state");
        assert_eq!(stream.bytes(), b"state");
    }
}
