//! A scripted Engine server for the adapter tests, in memory, behind the device client's own
//! transport trait: no network. It holds just the HTTP surface the client uses (devices, the
//! approval steps, the key envelopes, zones and their records), and like the Engine's server it
//! only stores and relays: every signature, the digits and every seal are the clients' own, so the
//! real client and the real Engine state run against it. Sign-up and sign-in need the server side
//! of OPAQUE and are not here; the devices start from a saved identity.

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use orbyn_device_client as dc;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const ACCOUNT: &str = "0a0a0a0a-0000-4000-8000-0000000000aa";
pub const EMAIL: &str = "person@example.com";

#[derive(Clone)]
struct Dev {
    id: String,
    name: String,
    app: Option<String>,
    signing_key: String,
    agreement_key: String,
    trusted: bool,
    created_at: String,
    /// `approved_by`, `approval_signature`, `approval_id`, `approval_created_at`, the hash.
    approval: Option<[Value; 5]>,
}

#[derive(Default)]
struct Zone {
    seq: i64,
    records: BTreeMap<String, Value>,
    operations: BTreeMap<String, Value>,
}

#[derive(Default)]
struct State {
    devices: Vec<Dev>,
    /// Session token to device id.
    tokens: BTreeMap<String, String>,
    approval: Option<Value>,
    zones: BTreeMap<String, Zone>,
    envelopes: Vec<(String, Value)>,
    offline: bool,
    refuse: Option<(String, u16, String)>,
    log: Vec<String>,
    removed: Vec<String>,
}

#[derive(Default)]
pub struct Server {
    state: Mutex<State>,
}

fn rfc3339_now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

fn in_ten_minutes() -> String {
    (time::OffsetDateTime::now_utc() + time::Duration::minutes(10))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

fn answer(status: u16, body: &Value) -> Result<dc::Response, dc::DeviceError> {
    Ok(dc::Response {
        status,
        retry_after: None,
        body: dc::Zeroizing::new(serde_json::to_vec(body).unwrap_or_default()),
    })
}

fn failure(status: u16, code: &str) -> Result<dc::Response, dc::DeviceError> {
    answer(status, &json!({ "code": code, "message": "" }))
}

impl Server {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Adds a device the server knows, with a session token the client holds. Returns the token.
    pub fn add_device(
        &self,
        id: &str,
        name: &str,
        app: Option<&str>,
        public: &dc::DevicePublic,
        trusted: bool,
    ) -> String {
        let token = crate::util::new_id();
        let mut state = self.lock();
        state.tokens.insert(token.clone(), id.to_owned());
        state.devices.push(Dev {
            id: id.to_owned(),
            name: name.to_owned(),
            app: app.map(str::to_owned),
            signing_key: B64.encode(public.signing),
            agreement_key: B64.encode(public.agreement),
            trusted,
            created_at: rfc3339_now(),
            approval: None,
        });
        token
    }

    pub fn set_offline(&self, offline: bool) {
        self.lock().offline = offline;
    }

    /// Every request whose path contains `part` is answered `status` `code`, until cleared.
    pub fn refuse(&self, part: &str, status: u16, code: &str) {
        self.lock().refuse = Some((part.to_owned(), status, code.to_owned()));
    }

    pub fn clear_refusal(&self) {
        self.lock().refuse = None;
    }

    /// How many requests went to a path that contains `part`.
    pub fn calls(&self, part: &str) -> usize {
        self.lock().log.iter().filter(|p| p.contains(part)).count()
    }

    pub fn approval_open(&self) -> bool {
        self.lock().approval.is_some()
    }

    pub fn is_trusted(&self, id: &str) -> bool {
        self.lock().devices.iter().any(|d| d.id == id && d.trusted)
    }

    pub fn was_removed(&self, id: &str) -> bool {
        self.lock().removed.iter().any(|r| r == id)
    }

    fn device_json(dev: &Dev, current: bool) -> Value {
        let approval = dev.approval.clone().unwrap_or([
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
        ]);
        json!({
            "id": dev.id, "name": dev.name, "platform": "macos", "app": dev.app,
            "state": if dev.trusted { "trusted" } else { "pending" },
            "signing_key": dev.signing_key, "agreement_key": dev.agreement_key,
            "created_at": dev.created_at,
            "approved_by": approval[0], "approval_signature": approval[1],
            "approval_id": approval[2], "approval_created_at": approval[3],
            "approval_endorsement_hash": approval[4],
            "current": current,
        })
    }

    pub fn handle(&self, request: &dc::Request) -> Result<dc::Response, dc::DeviceError> {
        let mut state = self.lock();
        let (path, query) = request
            .path
            .split_once('?')
            .unwrap_or((request.path.as_str(), ""));
        state.log.push(format!("{:?} {path}", request.method));
        if state.offline {
            return Err(dc::DeviceError::Transport);
        }
        if let Some((part, status, code)) = state.refuse.clone()
            && path.contains(&part)
        {
            return failure(status, &code);
        }
        if path == "/v1/accounts/status" {
            return answer(200, &json!({ "deleted_at": "" }));
        }
        let me = request
            .bearer
            .as_ref()
            .and_then(|token| state.tokens.get(token.as_str()).cloned());
        let Some(me) = me else {
            return failure(401, "unauthorized");
        };
        let body: Value = request
            .body
            .as_ref()
            .and_then(|bytes| serde_json::from_slice(bytes).ok())
            .unwrap_or(Value::Null);
        let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        let method = format!("{:?}", request.method);
        match (method.as_str(), parts.as_slice()) {
            ("Get", ["v1", "devices"]) => {
                let list: Vec<Value> = state
                    .devices
                    .iter()
                    .map(|d| Self::device_json(d, d.id == me))
                    .collect();
                answer(200, &json!({ "devices": list, "removals": [] }))
            }
            ("Get", ["v1", "devices", "current"]) => {
                let Some(dev) = state.devices.iter().find(|d| d.id == me) else {
                    return failure(401, "unauthorized");
                };
                let approval = state
                    .approval
                    .clone()
                    .filter(|a| a["device_id"] == json!(me))
                    .unwrap_or(Value::Null);
                answer(
                    200,
                    &json!({
                        "device": Self::device_json(dev, true), "approval": approval,
                        "totp_enabled": false, "totp_pending": false,
                    }),
                )
            }
            ("Post", ["v1", "devices", "approvals"]) => {
                let Some(approver) = state.devices.iter().find(|d| d.id == me).cloned() else {
                    return failure(401, "unauthorized");
                };
                let target = body["device_id"].as_str().unwrap_or_default();
                let Some(new) = state.devices.iter().find(|d| d.id == target).cloned() else {
                    return failure(404, "not_found");
                };
                let id = body["approval_id"].clone();
                state.approval = Some(json!({
                    "id": id, "expires_at": in_ten_minutes(), "created_at": body["created_at"],
                    "approver_id": approver.id, "approver_signing_key": approver.signing_key,
                    "approver_agreement_key": approver.agreement_key,
                    "device_id": new.id, "device_signing_key": new.signing_key,
                    "device_agreement_key": new.agreement_key,
                    "commitment": body["commitment"],
                    "device_nonce": null, "approver_nonce": null,
                    "signature": null, "endorsement": null,
                }));
                answer(200, &json!({ "id": id, "expires_at": in_ten_minutes() }))
            }
            (verb, ["v1", "devices", "approvals", id, rest @ ..]) => {
                let live = state
                    .approval
                    .as_ref()
                    .is_some_and(|a| a["id"] == json!(id));
                if !live {
                    return failure(410, "approval_expired");
                }
                let step = rest.first().copied();
                if let ("Get", None) = (verb, step) {
                    return answer(200, &state.approval.clone().unwrap_or_default());
                }
                let Some(approval) = state.approval.as_mut() else {
                    return failure(410, "approval_expired");
                };
                match step {
                    Some("nonce") => approval["device_nonce"] = body["nonce"].clone(),
                    Some("reveal") => {
                        if approval["device_nonce"].is_null() {
                            return failure(409, "approval_out_of_order");
                        }
                        approval["approver_nonce"] = body["nonce"].clone();
                    }
                    Some("confirm") => {
                        if approval["approver_nonce"].is_null() {
                            return failure(409, "approval_out_of_order");
                        }
                        approval["signature"] = body["signature"].clone();
                        approval["endorsement"] = body["endorsement"].clone();
                    }
                    Some("code") => {
                        if approval["signature"].is_null() {
                            return failure(409, "approval_not_confirmed");
                        }
                        let approval = approval.clone();
                        let hash = approval["endorsement"]
                            .as_str()
                            .and_then(|e| B64.decode(e).ok())
                            .map_or(Value::Null, |e| json!(B64.encode(Sha256::digest(e))));
                        let device_id = approval["device_id"].as_str().unwrap_or_default();
                        if let Some(dev) = state.devices.iter_mut().find(|d| d.id == device_id) {
                            dev.trusted = true;
                            dev.approval = Some([
                                approval["approver_id"].clone(),
                                approval["signature"].clone(),
                                approval["id"].clone(),
                                approval["created_at"].clone(),
                                hash,
                            ]);
                        }
                        state.approval = None;
                    }
                    Some("cancel") => state.approval = None,
                    _ => return failure(404, "not_found"),
                }
                answer(200, &json!({}))
            }
            ("Delete", ["v1", "devices", id]) => {
                let id = (*id).to_owned();
                state.devices.retain(|d| d.id != id);
                state.tokens.retain(|_, device| *device != id);
                state.removed.push(id);
                answer(200, &json!({}))
            }
            ("Get", ["v1", "keychain", "envelopes"]) => {
                let mine: Vec<Value> = state
                    .envelopes
                    .iter()
                    .filter(|(recipient, _)| *recipient == me)
                    .map(|(_, envelope)| envelope.clone())
                    .collect();
                answer(200, &json!({ "envelopes": mine }))
            }
            ("Post", ["v1", "keychain", "envelopes"]) => {
                for sent in body["envelopes"].as_array().into_iter().flatten() {
                    state.envelopes.push((
                        sent["recipient_device_id"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned(),
                        json!({
                            "sender_device_id": me, "zone": sent["zone"], "key_id": sent["key_id"],
                            "envelope": sent["envelope"], "created_at": rfc3339_now(),
                            "rotation": sent["rotation"],
                        }),
                    ));
                }
                answer(200, &json!({}))
            }
            ("Get", ["v1", "zones"]) => {
                let zones: Vec<Value> = state
                    .zones
                    .iter()
                    .map(|(name, zone)| {
                        json!({
                            "zone": name, "token": format!("t{}", zone.seq),
                            "deleted": false, "created_at": rfc3339_now(),
                        })
                    })
                    .collect();
                answer(200, &json!({ "zones": zones, "next": null }))
            }
            ("Put", ["v1", "zones", zone]) => {
                state.zones.entry((*zone).to_owned()).or_default();
                answer(200, &json!({}))
            }
            ("Get", ["v1", "zones", zone, "changes"]) => {
                let Some(zone) = state.zones.get(*zone) else {
                    return failure(404, "not_found");
                };
                let since: i64 = query
                    .split('&')
                    .find_map(|pair| pair.strip_prefix("since=t"))
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0);
                let mut records: Vec<&Value> = zone
                    .records
                    .values()
                    .filter(|r| r["version"].as_i64().unwrap_or(0) > since)
                    .collect();
                records.sort_by_key(|r| r["version"].as_i64());
                answer(
                    200,
                    &json!({
                        "records": records, "token": format!("t{}", zone.seq),
                        "more": false, "zone_deleted": false,
                    }),
                )
            }
            ("Post", ["v1", "zones", zone, "changes"]) => {
                let Some(zone) = state.zones.get_mut(*zone) else {
                    return failure(404, "not_found");
                };
                let operation = body["operation_id"].as_str().unwrap_or_default();
                if let Some(done) = zone.operations.get(operation) {
                    let mut replay = done.clone();
                    replay["replayed"] = json!(true);
                    return answer(200, &replay);
                }
                let changes = body["changes"].as_array().cloned().unwrap_or_default();
                let conflicts: Vec<Value> = changes
                    .iter()
                    .filter_map(|change| {
                        let id = change["record_id"].as_str()?;
                        let held = zone.records.get(id);
                        let has = held.map(|r| r["version"].clone());
                        let wants = change["expected_version"].clone();
                        (has.clone().unwrap_or(Value::Null) != wants)
                            .then(|| json!({ "record_id": id, "server_record": held }))
                    })
                    .collect();
                if !conflicts.is_empty() {
                    return answer(
                        409,
                        &json!({ "code": "record_changed", "message": "", "conflicts": conflicts }),
                    );
                }
                let mut saved = Vec::new();
                for change in &changes {
                    zone.seq += 1;
                    let id = change["record_id"].as_str().unwrap_or_default().to_owned();
                    let deleted = change["op"] == json!("delete");
                    let kind = change["record_type"].clone();
                    let kind = if kind.is_null() {
                        zone.records
                            .get(&id)
                            .map_or(Value::Null, |r| r["record_type"].clone())
                    } else {
                        kind
                    };
                    zone.records.insert(
                        id.clone(),
                        json!({
                            "record_id": id, "record_type": kind, "version": zone.seq,
                            "ciphertext": change["ciphertext"], "wrapped_key": change["wrapped_key"],
                            "author_device_id": me, "modified_at": rfc3339_now(),
                            "deleted": deleted, "delete_proof": change["proof"],
                        }),
                    );
                    saved.push(json!({
                        "record_id": id, "record_type": kind, "version": zone.seq, "deleted": deleted,
                    }));
                }
                let result = json!({
                    "token": format!("t{}", zone.seq), "modified_at": rfc3339_now(),
                    "records": saved, "replayed": false,
                });
                zone.operations.insert(operation.to_owned(), result.clone());
                answer(200, &result)
            }
            _ => failure(404, "not_found"),
        }
    }
}

/// The device client's transport over a shared [`Server`].
pub struct Wire(pub Arc<Server>);

impl dc::DeviceApi for Wire {
    fn send(
        &self,
        request: dc::Request,
    ) -> impl Future<Output = Result<dc::Response, dc::DeviceError>> + Send {
        let outcome = self.0.handle(&request);
        async move { outcome }
    }
}
