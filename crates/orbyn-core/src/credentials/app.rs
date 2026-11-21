//! The core's side of the engine API-key setting: the per-engine opt-in lives in the local settings
//! table (`engine_credentials.<engine>`, never synced, default off); the key lives only in the keychain
//! through [`Credentials`]. The routes and a driver's spawn call these and nothing else.

use orbyn_contracts::{EngineCredentialsState, EngineCredentialsStatus};

use super::{CredentialError, Credentials, ENGINES, SpawnSecret, Status};
use crate::app::Core;

fn setting_key(engine: &str) -> String {
    format!("engine_credentials.{engine}")
}

impl Core {
    /// Replaces the keychain store, for tests: they use memory and never the real keychain.
    #[cfg(feature = "test-support")]
    pub fn with_credentials(mut self, credentials: Credentials) -> Self {
        self.credentials = credentials;
        self
    }

    /// The store a normal build opens: the macOS keychain, or nothing where there is none.
    pub(crate) fn system_credentials() -> Credentials {
        Credentials::system(super::default_service())
    }

    fn credentials_opt_in(&self, engine: &str) -> Result<bool, CredentialError> {
        if !ENGINES.iter().any(|e| e.engine == engine) {
            return Err(CredentialError::UnknownEngine);
        }
        match self.store.setting(&setting_key(engine)) {
            Ok(value) => Ok(value.as_deref() == Some("1")),
            Err(error) => {
                tracing::error!(%error, "engine credentials opt-in could not be read");
                Err(CredentialError::Failed)
            }
        }
    }

    /// What the person may be told: the engine, the opt-in and whether a key is saved. Never the key.
    pub fn engine_credentials_status(
        &self,
        engine: &str,
    ) -> Result<EngineCredentialsStatus, CredentialError> {
        let opted_in = self.credentials_opt_in(engine)?;
        let state = match self.credentials.status(engine, opted_in)? {
            Status::Off => EngineCredentialsState::Off,
            Status::Unavailable => EngineCredentialsState::Unavailable,
            Status::Blocked => EngineCredentialsState::Blocked,
            Status::NotSaved => EngineCredentialsState::NotSaved,
            Status::Saved => EngineCredentialsState::Saved,
        };
        Ok(EngineCredentialsStatus {
            engine: engine.to_owned(),
            opted_in,
            present: state == EngineCredentialsState::Saved,
            state,
        })
    }

    /// Turns the opt-in on or off. Off keeps any saved item (status then reads off); remove deletes it.
    pub fn set_engine_credentials_opt_in(
        &self,
        engine: &str,
        opted_in: bool,
    ) -> Result<EngineCredentialsStatus, CredentialError> {
        self.credentials_opt_in(engine)?;
        if let Err(error) = self
            .store
            .set_setting(&setting_key(engine), if opted_in { "1" } else { "0" })
        {
            tracing::error!(%error, "engine credentials opt-in could not be saved");
            return Err(CredentialError::Failed);
        }
        self.engine_credentials_status(engine)
    }

    /// Saves or replaces the key in the keychain, only while the opt-in is on.
    pub fn save_engine_credentials_key(
        &self,
        engine: &str,
        key: &str,
    ) -> Result<EngineCredentialsStatus, CredentialError> {
        let opted_in = self.credentials_opt_in(engine)?;
        self.credentials.save(engine, opted_in, key)?;
        self.engine_credentials_status(engine)
    }

    /// Deletes the saved key. Works with the opt-in off, so turning it off can always be followed by a clean-up.
    ///
    /// Revoking also stops that engine's running threads through the ordinary cancel path, after the item is
    /// gone, and answers how many runs were stopped. A failed removal stops nothing and says so.
    pub fn remove_engine_credentials_key(
        &self,
        engine: &str,
    ) -> Result<(EngineCredentialsStatus, usize), CredentialError> {
        self.credentials_opt_in(engine)?;
        self.credentials.remove(engine)?;
        let stopped = self.stop_engine_runs(engine);
        Ok((self.engine_credentials_status(engine)?, stopped))
    }

    /// Cancels every run that has not ended on an engine of this family, by the same path as the Stop
    /// button, and counts the cancellations that were accepted. A run already over is not counted.
    pub(crate) fn stop_engine_runs(&self, family: &str) -> usize {
        let runs = match self.store.active_run_engines() {
            Ok(runs) => runs,
            Err(error) => {
                tracing::error!(%error, "could not list the runs to stop");
                return 0;
            }
        };
        let mut stopped = 0;
        for (run_id, engine) in runs {
            let run_family = self
                .engines
                .driver(&engine)
                .map_or_else(|_| engine.clone(), |driver| driver.family());
            if run_family == family && self.runs.cancel(&run_id).is_ok() {
                stopped += 1;
            }
        }
        stopped
    }

    /// The key for a child's environment, honouring the opt-in setting: `None` when it is off or nothing
    /// is saved. A driver calls this at spawn and puts [`SpawnSecret::expose`] in `SpawnSecret::env` of the
    /// child, and nowhere else. The engine host's pools reach it through `agents::driver::host::secrets_from`,
    /// so a host's spawn environment gets a key from here and from no other place.
    pub fn engine_spawn_secret(
        &self,
        engine: &str,
    ) -> Result<Option<SpawnSecret>, CredentialError> {
        let opted_in = self.credentials_opt_in(engine)?;
        self.credentials.take_for_spawn(engine, opted_in)
    }
}
