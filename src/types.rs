use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrStatus {
    Queued,
    Testing,
    Batched,
    Merged,
    Failed,
    Cancelled,
}

impl fmt::Display for PrStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_ref())
    }
}

impl AsRef<str> for PrStatus {
    fn as_ref(&self) -> &str {
        match self {
            Self::Queued => "queued",
            Self::Testing => "testing",
            Self::Batched => "batched",
            Self::Merged => "merged",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

impl From<&str> for PrStatus {
    fn from(s: &str) -> Self {
        match s {
            "queued" => Self::Queued,
            "testing" => Self::Testing,
            "batched" => Self::Batched,
            "merged" => Self::Merged,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            other => panic!("unknown PR status: {other}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchStatus {
    Pending,
    Testing,
    Done,
    Failed,
}

impl fmt::Display for BatchStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_ref())
    }
}

impl AsRef<str> for BatchStatus {
    fn as_ref(&self) -> &str {
        match self {
            Self::Pending => "pending",
            Self::Testing => "testing",
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }
}

impl From<&str> for BatchStatus {
    fn from(s: &str) -> Self {
        match s {
            "pending" => Self::Pending,
            "testing" => Self::Testing,
            "done" => Self::Done,
            "failed" => Self::Failed,
            other => panic!("unknown batch status: {other}"),
        }
    }
}
