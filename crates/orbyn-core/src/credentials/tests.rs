use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use zeroize::Zeroizing;

use super::*;

#[derive(Default)]
struct Memory {
    items: Mutex<HashMap<(String, String), Vec<u8>>>,
    fail: Mutex<Option<Failure>>,
}

impl SecretBackend for Memory {
    fn get(&self, service: &str, account: &str) -> Result<Option<Zeroizing<Vec<u8>>>, Failure> {
        if let Some(f) = *self.fail.lock().unwrap() {
            return Err(f);
        }
        Ok(self
            .items
            .lock()
            .unwrap()
            .get(&(service.into(), account.into()))
            .cloned()
            .map(Zeroizing::new))
    }
    fn set(&self, service: &str, account: &str, bytes: &[u8]) -> Result<(), Failure> {
        if let Some(f) = *self.fail.lock().unwrap() {
            return Err(f);
        }
        self.items
            .lock()
            .unwrap()
            .insert((service.into(), account.into()), bytes.to_vec());
        Ok(())
    }
    fn delete(&self, service: &str, account: &str) -> Result<(), Failure> {
        if let Some(f) = *self.fail.lock().unwrap() {
            return Err(f);
        }
        self.items
            .lock()
            .unwrap()
            .remove(&(service.into(), account.into()));
        Ok(())
    }
}

fn store() -> (Credentials, Arc<Memory>) {
    let memory = Arc::new(Memory::default());
    (Credentials::new(memory.clone(), DEV_SERVICE), memory)
}

#[test]
fn off_by_default_reads_and_writes_nothing() {
    let (c, m) = store();
    assert_eq!(c.status("cursor", false), Ok(Status::Off));
    assert_eq!(
        c.save("cursor", false, "key_123"),
        Err(CredentialError::Off)
    );
    assert!(m.items.lock().unwrap().is_empty());
    assert!(c.take_for_spawn("cursor", false).unwrap().is_none());
}

#[test]
fn only_listed_engines_keep_a_key() {
    let (c, m) = store();
    assert_eq!(
        c.save("claude", true, "key_123"),
        Err(CredentialError::UnknownEngine)
    );
    assert_eq!(
        c.status("claude", true),
        Err(CredentialError::UnknownEngine)
    );
    assert_eq!(c.remove("claude"), Err(CredentialError::UnknownEngine));
    assert!(m.items.lock().unwrap().is_empty());
}

#[test]
fn save_status_spawn_remove() {
    let (c, _) = store();
    assert_eq!(c.status("cursor", true), Ok(Status::NotSaved));
    c.save("cursor", true, "  key_abc123  ").unwrap();
    assert_eq!(c.status("cursor", true), Ok(Status::Saved));
    let secret = c.take_for_spawn("cursor", true).unwrap().unwrap();
    assert_eq!(secret.env, "CURSOR_API_KEY");
    assert_eq!(secret.expose(), "key_abc123");
    c.remove("cursor").unwrap();
    assert_eq!(c.status("cursor", true), Ok(Status::NotSaved));
    assert!(c.take_for_spawn("cursor", true).unwrap().is_none());
    assert!(c.remove("cursor").is_ok(), "removing nothing is fine");
}

#[test]
fn replace_overwrites() {
    let (c, _) = store();
    c.save("cursor", true, "key_one").unwrap();
    c.save("cursor", true, "key_two").unwrap();
    assert_eq!(
        c.take_for_spawn("cursor", true).unwrap().unwrap().expose(),
        "key_two"
    );
}

#[test]
fn bad_keys_are_refused_before_the_keychain() {
    let (c, m) = store();
    for bad in [
        "",
        "   ",
        "two words",
        "line\nbreak",
        "tab\tkey",
        &"k".repeat(MAX_KEY_LEN + 1),
    ] {
        assert_eq!(
            c.save("cursor", true, bad),
            Err(CredentialError::InvalidKey),
            "{bad:?}"
        );
    }
    assert!(m.items.lock().unwrap().is_empty());
}

#[test]
fn removal_works_with_the_opt_in_off() {
    let (c, _) = store();
    c.save("cursor", true, "key_abc").unwrap();
    c.remove("cursor").unwrap();
    assert_eq!(c.status("cursor", true), Ok(Status::NotSaved));
}

#[test]
fn keychain_failures_map_to_what_the_person_is_told() {
    let (c, m) = store();
    *m.fail.lock().unwrap() = Some(Failure::Unusable);
    assert_eq!(c.status("cursor", true), Ok(Status::Unavailable));
    assert_eq!(
        c.save("cursor", true, "key_abc"),
        Err(CredentialError::Unavailable)
    );
    *m.fail.lock().unwrap() = Some(Failure::Blocked);
    assert_eq!(c.status("cursor", true), Ok(Status::Blocked));
    assert_eq!(
        c.take_for_spawn("cursor", true).unwrap_err(),
        CredentialError::Blocked
    );
    *m.fail.lock().unwrap() = Some(Failure::Other);
    assert_eq!(c.status("cursor", true), Err(CredentialError::Failed));
}

#[test]
fn nothing_prints_the_key() {
    let (c, _) = store();
    c.save("cursor", true, "key_secretvalue").unwrap();
    let secret = c.take_for_spawn("cursor", true).unwrap().unwrap();
    for text in [
        format!("{secret:?}"),
        format!("{c:?}"),
        format!("{:?}", Status::Saved),
        CredentialError::Failed.to_string(),
    ] {
        assert!(!text.contains("secretvalue"), "{text}");
    }
}

#[test]
fn the_service_name_separates_dev_and_release_items() {
    let memory = Arc::new(Memory::default());
    let dev = Credentials::new(memory.clone(), DEV_SERVICE);
    let release = Credentials::new(memory, "app.orbyn.engineering.engine-keys");
    dev.save("cursor", true, "key_abc").unwrap();
    assert_eq!(release.status("cursor", true), Ok(Status::NotSaved));
}

#[test]
fn a_platform_with_no_backend_says_unavailable_and_stores_nothing() {
    #[cfg(not(all(target_os = "macos", feature = "engine-client")))]
    {
        let c = Credentials::system(DEV_SERVICE);
        assert_eq!(c.status("cursor", true), Ok(Status::Unavailable));
        assert_eq!(
            c.save("cursor", true, "key_abc"),
            Err(CredentialError::Unavailable)
        );
    }
}
