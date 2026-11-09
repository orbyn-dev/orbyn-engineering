//! Work that must finish even when the client goes away.

use orbyn_core::CoreError;

/// Runs an update to the end even when the client goes away, because stopping an
/// installer partway could leave a half-written engine.
pub async fn to_the_end<T: Send + 'static>(
    work: impl std::future::Future<Output = orbyn_core::Result<T>> + Send + 'static,
) -> orbyn_core::Result<T> {
    tokio::spawn(work)
        .await
        .map_err(|error| CoreError::Internal(format!("The update stopped: {error}")))?
}
