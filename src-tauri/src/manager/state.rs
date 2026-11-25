use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    Queued,
    Probing,
    Downloading,
    Paused,
    Verifying,
    Completed,
    Failed,
    WaitingRefresh,
    Canceled,
}

impl DownloadStatus {
    /// Validates state machine transitions based on Section 5.1 of SPEC.md
    pub fn can_transition_to(&self, target: DownloadStatus) -> bool {
        match (self, target) {
            // Self-transitions are always no-op allowed
            (a, b) if *a == b => true,

            // From Queued
            (Self::Queued, Self::Probing) | (Self::Queued, Self::Canceled) => true,

            // From Probing
            (Self::Probing, Self::Downloading)
            | (Self::Probing, Self::Failed)
            | (Self::Probing, Self::Canceled) => true,

            // From Downloading
            (Self::Downloading, Self::Paused)
            | (Self::Downloading, Self::Verifying)
            | (Self::Downloading, Self::Failed)
            | (Self::Downloading, Self::WaitingRefresh)
            | (Self::Downloading, Self::Canceled) => true,

            // From Paused
            (Self::Paused, Self::Probing)
            | (Self::Paused, Self::Downloading)
            | (Self::Paused, Self::Queued)
            | (Self::Paused, Self::Canceled) => true,

            // From Verifying
            (Self::Verifying, Self::Completed) | (Self::Verifying, Self::Failed) => true,

            // From Failed
            (Self::Failed, Self::Queued)
            | (Self::Failed, Self::Probing)
            | (Self::Failed, Self::Canceled) => true,

            // From WaitingRefresh (after user or extension refreshes URL)
            (Self::WaitingRefresh, Self::Probing)
            | (Self::WaitingRefresh, Self::Queued)
            | (Self::WaitingRefresh, Self::Canceled) => true,

            // Terminal states
            (Self::Completed, _) => false,
            (Self::Canceled, _) => false,

            _ => false,
        }
    }
}

impl std::fmt::Display for DownloadStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Queued => write!(f, "queued"),
            Self::Probing => write!(f, "probing"),
            Self::Downloading => write!(f, "downloading"),
            Self::Paused => write!(f, "paused"),
            Self::Verifying => write!(f, "verifying"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::WaitingRefresh => write!(f, "waiting_refresh"),
            Self::Canceled => write!(f, "canceled"),
        }
    }
}

impl FromStr for DownloadStatus {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "queued" => Ok(Self::Queued),
            "probing" => Ok(Self::Probing),
            "downloading" => Ok(Self::Downloading),
            "paused" => Ok(Self::Paused),
            "verifying" => Ok(Self::Verifying),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "waiting_refresh" => Ok(Self::WaitingRefresh),
            "canceled" => Ok(Self::Canceled),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadErrorKind {
    NetworkTimeout,
    ConnectionReset,
    RangeNotSatisfiable, // 416
    LinkExpired,         // 403, 410 -> triggers waiting_refresh
    RateLimited,         // 429, 503
    ServerIgnoredRange,  // 200 returned instead of 206
    DiskFull,
    PermissionDenied,
    ETagChanged,
    Other(String),
}

impl DownloadErrorKind {
    pub fn from_http_status(status: StatusCode) -> Option<Self> {
        match status {
            StatusCode::FORBIDDEN | StatusCode::GONE => Some(Self::LinkExpired),
            StatusCode::RANGE_NOT_SATISFIABLE => Some(Self::RangeNotSatisfiable),
            StatusCode::TOO_MANY_REQUESTS | StatusCode::SERVICE_UNAVAILABLE => Some(Self::RateLimited),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_transitions() {
        assert!(DownloadStatus::Queued.can_transition_to(DownloadStatus::Probing));
        assert!(DownloadStatus::Probing.can_transition_to(DownloadStatus::Downloading));
        assert!(DownloadStatus::Downloading.can_transition_to(DownloadStatus::Paused));
        assert!(DownloadStatus::Downloading.can_transition_to(DownloadStatus::WaitingRefresh));
        assert!(DownloadStatus::Downloading.can_transition_to(DownloadStatus::Verifying));
        assert!(DownloadStatus::Verifying.can_transition_to(DownloadStatus::Completed));
        assert!(DownloadStatus::WaitingRefresh.can_transition_to(DownloadStatus::Probing));
    }

    #[test]
    fn test_invalid_transitions() {
        assert!(!DownloadStatus::Completed.can_transition_to(DownloadStatus::Downloading));
        assert!(!DownloadStatus::Canceled.can_transition_to(DownloadStatus::Downloading));
        assert!(!DownloadStatus::Queued.can_transition_to(DownloadStatus::Completed));
    }

    #[test]
    fn test_error_classification() {
        assert_eq!(
            DownloadErrorKind::from_http_status(StatusCode::FORBIDDEN),
            Some(DownloadErrorKind::LinkExpired)
        );
        assert_eq!(
            DownloadErrorKind::from_http_status(StatusCode::GONE),
            Some(DownloadErrorKind::LinkExpired)
        );
        assert_eq!(
            DownloadErrorKind::from_http_status(StatusCode::RANGE_NOT_SATISFIABLE),
            Some(DownloadErrorKind::RangeNotSatisfiable)
        );
        assert_eq!(
            DownloadErrorKind::from_http_status(StatusCode::TOO_MANY_REQUESTS),
            Some(DownloadErrorKind::RateLimited)
        );
    }
}
