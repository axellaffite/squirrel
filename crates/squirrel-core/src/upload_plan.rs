/// How a file of `size` bytes is split into fixed-size chunks (the last one may
/// be shorter). An empty file has no chunks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadPlan {
    size: u64,
    chunk_size: u64,
}

impl UploadPlan {
    /// `chunk_size` must be > 0.
    pub fn new(size: u64, chunk_size: u64) -> Self {
        assert!(chunk_size > 0, "chunk size must be positive");
        Self { size, chunk_size }
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn chunk_size(&self) -> u64 {
        self.chunk_size
    }

    pub fn chunk_count(&self) -> u64 {
        self.size.div_ceil(self.chunk_size)
    }

    /// Byte offset of chunk `index`, or `None` past the last chunk.
    pub fn offset(&self, index: u64) -> Option<u64> {
        (index < self.chunk_count()).then(|| index * self.chunk_size)
    }

    /// Exact length chunk `index` must have, or `None` past the last chunk.
    pub fn chunk_len(&self, index: u64) -> Option<u64> {
        self.offset(index)
            .map(|offset| (self.size - offset).min(self.chunk_size))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_into_full_chunks_and_a_short_tail() {
        let plan = UploadPlan::new(10, 4);
        assert_eq!(plan.chunk_count(), 3);
        assert_eq!(plan.offset(2), Some(8));
        assert_eq!(plan.chunk_len(0), Some(4));
        assert_eq!(plan.chunk_len(2), Some(2));
        assert_eq!(plan.chunk_len(3), None);
    }

    #[test]
    fn exact_multiple_has_no_short_tail() {
        let plan = UploadPlan::new(8, 4);
        assert_eq!(plan.chunk_count(), 2);
        assert_eq!(plan.chunk_len(1), Some(4));
    }

    #[test]
    fn empty_file_has_no_chunks() {
        let plan = UploadPlan::new(0, 4);
        assert_eq!(plan.chunk_count(), 0);
        assert_eq!(plan.offset(0), None);
    }

    #[test]
    fn huge_files_do_not_overflow() {
        let plan = UploadPlan::new(u64::MAX, 8 << 20);
        let last = plan.chunk_count() - 1;
        assert_eq!(
            plan.offset(last).unwrap() + plan.chunk_len(last).unwrap(),
            u64::MAX
        );
    }
}
