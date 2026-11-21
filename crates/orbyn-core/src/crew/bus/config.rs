//! Lower-only policy from the lead's committed repository. Nothing here changes bus counters
//! or the host ceiling. Git reads happen before the bus writer lock is taken.

use std::fmt;
use std::path::Path;

use orbyn_contracts::{CrewError, Thread};
use serde::de::{self, IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

use super::{Limits, Sender};
use crate::app::Core;
use crate::crew::classes::{ClassFile, base_commit, pin, read_class_file};
use crate::crew::leases::internal;
use crate::crew::tool::invalid;
use crate::error::CoreError;

/// Only these two ceilings are repository-configurable. Values above the host are ignored.
#[derive(Clone, Debug, Default)]
pub(in crate::crew) struct RepoLimits {
    exchanges: Option<u64>,
    posts: Option<u64>,
    unknown: bool,
}

impl RepoLimits {
    pub(in crate::crew) fn apply(&self, host: Limits) -> Limits {
        host.lowered(
            self.exchanges
                .map(|n| n.min(u64::from(host.exchanges)) as u32),
            self.posts.map(|n| n.min(u64::from(host.posts)) as u32),
        )
    }

    fn tightened(self, other: Self) -> Self {
        let lower = |a: Option<u64>, b: Option<u64>| match (a, b) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        Self {
            exchanges: lower(self.exchanges, other.exchanges),
            posts: lower(self.posts, other.posts),
            unknown: self.unknown || other.unknown,
        }
    }

    fn note(&self, host: Limits) {
        if self.unknown {
            tracing::info!("crew bus: unknown repository bus fields were ignored");
        }
        if self
            .exchanges
            .is_some_and(|n| n > u64::from(host.exchanges))
            || self.posts.is_some_and(|n| n > u64::from(host.posts))
        {
            tracing::info!("crew bus: a repository limit above the host ceiling was ignored");
        }
    }
}

impl<'de> Deserialize<'de> for RepoLimits {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Section;
        impl<'de> Visitor<'de> for Section {
            type Value = RepoLimits;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a bus object")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut limits = RepoLimits::default();
                while let Some(key) = map.next_key::<String>()? {
                    let slot = match key.as_str() {
                        "exchanges" => &mut limits.exchanges,
                        "posts" => &mut limits.posts,
                        _ => {
                            limits.unknown = true;
                            map.next_value::<IgnoredAny>()?;
                            continue;
                        }
                    };
                    if slot.is_some() {
                        return Err(de::Error::custom("a bus limit is repeated"));
                    }
                    // Present null is invalid; it must not become an absent/default limit.
                    *slot = Some(map.next_value::<u64>()?);
                }
                Ok(limits)
            }
        }
        deserializer.deserialize_map(Section)
    }
}

/// Parse from the original text: converting to Value first would silently discard duplicate keys.
pub(in crate::crew) fn parse(source: &str) -> Result<RepoLimits, String> {
    let value: serde_json::Value =
        serde_json::from_str(source).map_err(|_| "crew.json is not valid JSON".to_owned())?;
    if crate::crew::find_secret(&value).is_some() {
        return Err("crew.json holds a secret, so it is refused".to_owned());
    }
    struct Document;
    impl<'de> Visitor<'de> for Document {
        type Value = RepoLimits;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a crew.json object")
        }
        fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
            let mut bus = None;
            while let Some(key) = map.next_key::<String>()? {
                if key == "bus" {
                    if bus.is_some() {
                        return Err(de::Error::custom("bus is repeated"));
                    }
                    bus = Some(map.next_value::<RepoLimits>()?);
                } else {
                    map.next_value::<IgnoredAny>()?;
                }
            }
            Ok(bus.unwrap_or_default())
        }
    }
    let mut reader = serde_json::Deserializer::from_str(source);
    let limits = reader.deserialize_map(Document);
    let bad =
        || "crew.json bus must be an object with unique nonnegative integer limits".to_owned();
    let limits = limits.map_err(|_| bad())?;
    reader.end().map_err(|_| bad())?;
    Ok(limits)
}

fn from_file(file: ClassFile) -> Result<RepoLimits, CrewError> {
    match file.bus {
        Some(limits) => limits.map_err(invalid),
        None if file.invalid() => Err(invalid(file.problem_text())),
        None => Ok(RepoLimits::default()),
    }
}

impl Core {
    /// Existing whole-file/class/pin failures belong to the router's needs_person/audit path.
    /// A malformed bus section in an otherwise valid document is refused before admission.
    pub(in crate::crew) async fn crew_start_bus_limits(
        &self,
        lead: &Thread,
    ) -> Result<(), CrewError> {
        fn validate(file: ClassFile, host: Limits) -> Result<(), CrewError> {
            if !file.invalid() {
                from_file(file)?.note(host);
            }
            Ok(())
        }
        self.bus_project(lead)?;
        let root = Path::new(&lead.cwd);
        let base = base_commit(root).await;
        validate(
            read_class_file(root, base.as_deref()).await,
            self.crew_bus.limits(),
        )?;
        let remembered = match self.store.crew_pinned_ref(&lead.project_id) {
            Ok(pin) => pin,
            // The class router records this specific damaged-pin refusal and starts no engine.
            Err(CoreError::StoredDataDamaged(_)) => return Ok(()),
            Err(error) => return Err(internal(error)),
        };
        if let pin::Decision::Read { file, .. } = pin::decide(root, remembered.as_ref()).await {
            validate(file, self.crew_bus.limits())?;
        }
        self.bus_project(lead)?;
        let current = match self.store.crew_pinned_ref(&lead.project_id) {
            Ok(pin) => pin,
            Err(CoreError::StoredDataDamaged(_)) => return Ok(()),
            Err(error) => return Err(internal(error)),
        };
        if current != remembered {
            return Err(invalid(
                "crew bus default-branch binding changed; try again",
            ));
        }
        Ok(())
    }

    fn bus_project(&self, lead: &Thread) -> Result<(), CrewError> {
        let saved = self.store.thread(&lead.id).map_err(internal)?;
        let project = self.store.project(&lead.project_id).map_err(internal)?;
        if saved.project_id != lead.project_id
            || saved.cwd != lead.cwd
            || saved.removed_project_name.is_some()
            || !project.trusted
        {
            return Err(invalid(
                "crew bus repository binding changed or is untrusted",
            ));
        }
        Ok(())
    }

    /// A single operation captures HEAD/default object ids. A later operation sees later commits;
    /// a working-copy edit is never policy. The remembered pin is checked but not advanced here.
    pub(in crate::crew) async fn crew_repo_limits(
        &self,
        lead: &Thread,
    ) -> Result<RepoLimits, CrewError> {
        self.bus_project(lead)?;
        let root = Path::new(&lead.cwd);
        let base = base_commit(root).await;
        let mut limits = from_file(read_class_file(root, base.as_deref()).await)?;
        limits.note(self.crew_bus.limits());
        let remembered = self
            .store
            .crew_pinned_ref(&lead.project_id)
            .map_err(internal)?;
        match pin::decide(root, remembered.as_ref()).await {
            pin::Decision::None => {}
            pin::Decision::Read { file, .. } => {
                let there = from_file(file)?;
                there.note(self.crew_bus.limits());
                limits = limits.tightened(there);
            }
            pin::Decision::Unreadable { file, .. } => return Err(invalid(file.problem_text())),
            pin::Decision::Blocked { reason, .. } => return Err(invalid(reason)),
        }
        // Git awaits must not let a changed binding apply another repository's policy.
        self.bus_project(lead)?;
        // A concurrent person/start changing the remembered pin invalidates this capture.
        if self
            .store
            .crew_pinned_ref(&lead.project_id)
            .map_err(internal)?
            != remembered
        {
            return Err(invalid(
                "crew bus default-branch binding changed; try again",
            ));
        }
        Ok(limits)
    }

    fn bus_worker_project(&self, who: &Sender, lead: &Thread) -> Result<(), CrewError> {
        let worker = self.store.thread(&who.thread).map_err(internal)?;
        if worker.project_id != lead.project_id || worker.removed_project_name.is_some() {
            return Err(invalid(
                "crew bus worker and lead belong to different projects",
            ));
        }
        Ok(())
    }

    pub(in crate::crew) async fn crew_limits(&self, who: &Sender) -> Result<RepoLimits, CrewError> {
        let lead = self.store.thread(&who.lead_thread).map_err(internal)?;
        self.bus_worker_project(who, &lead)?;
        let limits = self.crew_repo_limits(&lead).await?;
        self.bus_worker_project(who, &lead)?;
        Ok(limits)
    }
}
