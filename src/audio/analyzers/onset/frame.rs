//! Sliding audio frame buffer.
//!
//! Per §2.1 of the paper: each frame is a group of consecutive audio buffers,
//! and the start of each frame is separated by exactly one buffer size (so
//! frames overlap by `frame_size - buffer_size` samples). With the paper's
//! defaults this is a 2048-sample frame advancing in 512-sample hops.

/// Accumulates fixed-size audio buffers into overlapping frames.
///
/// Calling code pushes buffers of exactly `buffer_size` samples. Once
/// `frame_size / buffer_size` buffers have been pushed, every subsequent call
/// returns a new frame (sliding window, hop = `buffer_size`).
pub struct FrameBuffer {
    buffer_size: usize,
    frame_size: usize,
    /// Ring of the last `frame_size` samples.
    ring: Vec<f32>,
    /// Write position within `ring`.
    write_pos: usize,
    /// Number of samples written total (saturates for "is warm" check).
    filled: usize,
    /// Reusable scratch buffer returned to the caller as a contiguous frame.
    scratch: Vec<f32>,
}

impl FrameBuffer {
    /// Panics if `frame_size` is not a multiple of `buffer_size` or either is zero.
    pub fn new(buffer_size: usize, frame_size: usize) -> Self {
        assert!(buffer_size > 0, "buffer_size must be > 0");
        assert!(frame_size > 0, "frame_size must be > 0");
        assert!(
            frame_size % buffer_size == 0,
            "frame_size ({}) must be a multiple of buffer_size ({})",
            frame_size,
            buffer_size,
        );
        Self {
            buffer_size,
            frame_size,
            ring: vec![0.0; frame_size],
            write_pos: 0,
            filled: 0,
            scratch: vec![0.0; frame_size],
        }
    }

    pub fn buffer_size(&self) -> usize {
        self.buffer_size
    }

    pub fn frame_size(&self) -> usize {
        self.frame_size
    }

    /// Push exactly one buffer (`buffer_size` samples). Returns `Some(frame)`
    /// once the internal ring is full. The returned slice is valid until the
    /// next call to `push`.
    pub fn push(&mut self, buffer: &[f32]) -> Option<&[f32]> {
        assert_eq!(
            buffer.len(),
            self.buffer_size,
            "push() requires exactly buffer_size samples",
        );

        // Copy into ring. No wraparound is possible because frame_size is a
        // multiple of buffer_size and write_pos always sits on a buffer boundary.
        let dst = &mut self.ring[self.write_pos..self.write_pos + self.buffer_size];
        dst.copy_from_slice(buffer);
        self.write_pos = (self.write_pos + self.buffer_size) % self.frame_size;
        self.filled = self.filled.saturating_add(self.buffer_size).min(self.frame_size);

        if self.filled < self.frame_size {
            return None;
        }

        // Reassemble the frame in time order: oldest buffer first. After the
        // write, `write_pos` points at the start of the oldest data.
        let start = self.write_pos;
        let tail = self.frame_size - start;
        self.scratch[..tail].copy_from_slice(&self.ring[start..]);
        self.scratch[tail..].copy_from_slice(&self.ring[..start]);
        Some(&self.scratch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yields_nothing_until_warm() {
        let mut fb = FrameBuffer::new(2, 8);
        assert!(fb.push(&[1.0, 2.0]).is_none());
        assert!(fb.push(&[3.0, 4.0]).is_none());
        assert!(fb.push(&[5.0, 6.0]).is_none());
        let frame = fb.push(&[7.0, 8.0]).unwrap();
        assert_eq!(frame, &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
    }

    #[test]
    fn slides_by_one_buffer() {
        let mut fb = FrameBuffer::new(2, 8);
        for b in [[1.0, 2.0], [3.0, 4.0], [5.0, 6.0]] {
            fb.push(&b);
        }
        let f1 = fb.push(&[7.0, 8.0]).unwrap().to_vec();
        let f2 = fb.push(&[9.0, 10.0]).unwrap().to_vec();
        assert_eq!(f1, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
        assert_eq!(f2, vec![3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]);
    }
}
