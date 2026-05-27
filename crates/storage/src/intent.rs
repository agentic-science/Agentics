use crate::{Result, StorageError};

/// Storage write/read purpose with an explicit byte cap.
#[derive(Debug, Clone, Copy)]
pub struct StorageWriteIntent {
    label: &'static str,
    max_bytes: u64,
}

impl StorageWriteIntent {
    /// Create a write intent with a caller-owned byte limit.
    pub const fn new(label: &'static str, max_bytes: u64) -> Self {
        Self { label, max_bytes }
    }

    /// User-facing purpose label.
    pub const fn label(self) -> &'static str {
        self.label
    }

    /// Maximum bytes allowed for this object.
    pub const fn max_bytes(self) -> u64 {
        self.max_bytes
    }

    /// Verify a byte length against this intent.
    pub fn ensure_len(self, actual: u64) -> Result<()> {
        if actual > self.max_bytes {
            return Err(StorageError::ObjectTooLarge {
                label: self.label,
                actual,
                limit: self.max_bytes,
            });
        }
        Ok(())
    }
}
