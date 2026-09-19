//! File: Executes the fixed synthetic job allowlist without invoking a shell or external command.
//! Function: `execute` maps a validated `JobKind` and bounded payload to a bounded result.
//! Variables: `payload` is untrusted input already size-checked at submission and checked again here.

use crate::domain::{JobKind, MAX_PAYLOAD_BYTES};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::time::{Duration, sleep};

/// Executes one harmless operation and returns a bounded UTF-8 result.
pub async fn execute(kind: &JobKind, payload: &str) -> Result<String, WorkerError> {
    kind.validate_payload(payload)?;
    let output = match kind {
        JobKind::Delay => {
            let milliseconds = payload
                .parse::<u64>()
                .map_err(|_| WorkerError::InvalidPayload)?;
            sleep(Duration::from_millis(milliseconds)).await;
            format!("slept:{milliseconds}ms")
        }
        JobKind::Checksum => hex::encode(Sha256::digest(payload.as_bytes())),
        JobKind::Uppercase => payload.to_uppercase(),
    };
    if output.len() > MAX_PAYLOAD_BYTES {
        return Err(WorkerError::ResultTooLarge);
    }
    Ok(output)
}

#[derive(Debug, Error)]
pub enum WorkerError {
    #[error(transparent)]
    Validation(#[from] crate::domain::ValidationError),
    #[error("worker payload is invalid")]
    InvalidPayload,
    #[error("worker result exceeds the output limit")]
    ResultTooLarge,
}

#[cfg(test)]
mod tests {
    use super::execute;
    use crate::domain::JobKind;

    #[allow(clippy::unwrap_used)]
    #[tokio::test]
    async fn executes_allowlisted_operations() {
        assert_eq!(
            execute(&JobKind::Uppercase, "safe job").await.unwrap(),
            "SAFE JOB"
        );
        assert_eq!(execute(&JobKind::Delay, "0").await.unwrap(), "slept:0ms");
        assert_eq!(execute(&JobKind::Checksum, "abc").await.unwrap().len(), 64);
    }
}
