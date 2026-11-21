//! Subscription snapshots have no validated current account/workspace cache key.
//! Read afresh, and fence responses that a forget (account change or sign-out) has superseded.
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use orbyn_contracts::{EngineLimits, LimitsOverview, LimitsUnavailable};
use serde_json::Value;

use super::{limit_windows, pool_windows};
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::util::now;
use crate::util::swr::Notify;

#[derive(Default)]
pub(super) struct LimitsReads {
    generation: Arc<AtomicU64>,
    notify: OnceLock<Notify>,
}

impl LimitsReads {
    fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// Fences reads in flight across an account change or sign-out. Overlapping
    /// reads do not fence one another.
    pub(super) fn forget(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        if let Some(notify) = self.notify.get() {
            notify("limits");
        }
    }

    pub(super) fn set_notify(&self, notify: Notify) {
        let _ = self.notify.set(notify);
    }
}

impl Core {
    /// Reads the engine's current subscription report without keeping a fallback.
    /// Neither its signed-in boolean nor its plan proves the account/workspace.
    /// `force` remains accepted for compatibility; every read is fresh.
    pub async fn engine_limits(&self, id: &str, _force: bool) -> Result<EngineLimits> {
        // Observe invalidation before lookup can resolve an older configuration.
        let epoch = self.engines.change_epoch();
        let generation = self.usage.limits.generation();
        let driver = self.engines.driver(id)?;
        let report = driver.limits().await;
        if generation != self.usage.limits.generation() || epoch != self.engines.change_epoch() {
            return Err(CoreError::EngineUnavailable(
                "The engine's limits changed while they were being read. Read them again.".into(),
            ));
        }
        let report = report?.ok_or_else(|| {
            CoreError::Unsupported("This engine does not report its limits.".into())
        })?;
        Ok(EngineLimits {
            engine: id.to_owned(),
            checked_at: now(),
            windows: limit_windows(&report),
            plan: report
                .get("planType")
                .and_then(Value::as_str)
                .map(str::to_owned),
            limits: report,
        })
    }

    /// No supported driver can validate an account/workspace without asking its CLI.
    /// No-I/O callers therefore see not reported, rather than another account's limits.
    pub fn cached_limits(&self, _id: &str) -> Option<EngineLimits> {
        None
    }

    /// The overview never starts or awaits Claude, including its status probe.
    /// An explicit engine/thread limits read can still ask that driver.
    pub async fn limits_overview(&self, force: bool) -> Result<LimitsOverview> {
        let mut engines = Vec::new();
        let mut unavailable = Vec::new();
        let installed = self.engines.limits_engine_ids().await?;
        let reads = installed.iter().map(|id| self.engine_limits(id, force));
        for (id, read) in installed
            .iter()
            .zip(futures_util::future::join_all(reads).await)
        {
            match read {
                Ok(limits) => engines.push(limits),
                Err(CoreError::Unsupported(_)) => {}
                Err(error) => unavailable.push(LimitsUnavailable {
                    engine: id.clone(),
                    reason: error.public_message(),
                }),
            }
        }
        Ok(LimitsOverview {
            pooled: pool_windows(&engines),
            engines,
            unavailable,
        })
    }
}
