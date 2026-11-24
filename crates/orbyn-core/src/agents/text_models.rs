//! The one-shot model thread titles and source-control writing use.
//!
//! One name cannot fit several engines, so `text_generation_models` names a model
//! per engine and `text_generation_model` remains the fallback. A named model is
//! used only by an engine that lists it; a name the engine does not list is passed
//! over, never sent. The same order ends at the engine's small model and then the
//! thread's own model, and both call paths go through [`text_model_for`].

use std::sync::Arc;

use orbyn_contracts::{
    ModelInfo, TextGeneratorEngine, TextGeneratorProject, TextGenerators, TextModelReason,
};
use serde_json::Value;

use crate::agents::engines::EngineManager;
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::settings::preferences::{stored_preference, stored_preferences};
use crate::store::Store;

/// The per-engine map: engine id -> model id.
pub(crate) const SETTING: &str = "text_generation_models";
/// The single-name setting kept as the fallback, with no migration.
pub(crate) const LEGACY_SETTING: &str = "text_generation_model";

const MAX_ENGINES: usize = 64;
const MAX_ENGINE_ID: usize = 64;
const MAX_MODEL_ID: usize = 256;
/// What one read returns per engine; a larger catalog still resolves every model.
const MODELS_PER_ENGINE: usize = 200;
/// Families engines use for their small models, preferred in this order.
const SMALL_MODELS: &[&str] = &["haiku", "luna", "mini", "flash", "small"];

/// A model and why it was picked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TextModel {
    pub model: String,
    pub reason: TextModelReason,
}

/// An engine id starts with a letter or digit and is otherwise letters, digits,
/// `-` and `_`, like a provider instance id.
fn plain_engine(id: &str) -> bool {
    let mut chars = id.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphanumeric())
        && id.len() <= MAX_ENGINE_ID
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// A model id is a short plain argument: no whitespace or control characters, and
/// never something an engine could read as a flag.
fn plain_model(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_MODEL_ID
        && !id.starts_with('-')
        && id.bytes().all(|byte| byte.is_ascii_graphic())
}

/// Whether the stored map is an object of at most 64 engine ids to plain model ids.
/// This is the check every write gets, and the one a synced value would get.
pub(crate) fn validate_syntax(value: &Value) -> Result<()> {
    let map = value.as_object().ok_or_else(|| {
        CoreError::Invalid("Text generator models are an object of engine ids to model ids.".into())
    })?;
    if map.len() > MAX_ENGINES {
        return Err(CoreError::Invalid(format!(
            "At most {MAX_ENGINES} text generator models."
        )));
    }
    for (engine, model) in map {
        if !plain_engine(engine) {
            return Err(CoreError::Invalid(
                "An engine id is up to 64 letters, digits, - and _, starting with a letter or digit."
                    .into(),
            ));
        }
        let Some(model) = model.as_str() else {
            return Err(CoreError::Invalid(
                "A text generator model is a model id string.".into(),
            ));
        };
        if !plain_model(model) {
            return Err(CoreError::Invalid(
                "A model id is up to 256 plain characters and does not start with -.".into(),
            ));
        }
    }
    Ok(())
}

/// Refuses a model an engine has listed and does not offer. An engine that has not
/// answered yet cannot say, so the write is accepted and the run's own start checks it.
pub(crate) fn check_offered(engines: &EngineManager, value: &Value) -> Result<()> {
    let map = value
        .as_object()
        .expect("syntax validation ran before the offer check");
    for (engine, model) in map {
        let Some(catalog) = engines.known_models(engine) else {
            continue;
        };
        // A stand-in, a failure or a documented list is not the engine's own answer.
        if catalog.source != "engine" || catalog.fallback || catalog.error.is_some() {
            continue;
        }
        let model = model
            .as_str()
            .expect("syntax validation ran before the offer check");
        if !catalog.models.iter().any(|known| known.id == model) {
            return Err(CoreError::Invalid(format!(
                "{} does not list {model} as a model.",
                engines.display_name(engine)
            )));
        }
    }
    Ok(())
}

/// The map in two layers: the global one and the project's own (empty when the project stores none), and the map a
/// project call resolves with: the global entries, each replaced by the project's own entry for the same engine. A project
/// that does not name an engine inherits the global choice for it. Clearing is a write without the key.
fn layers(store: &Store, project_id: Option<&str>) -> Result<(Value, Value, Value)> {
    let global = stored_preference(store, SETTING, None)?;
    let Some(id) = project_id else {
        return Ok((global.clone(), Value::Object(Default::default()), global));
    };
    let own = stored_preferences(store, Some(id))?
        .into_iter()
        .find(|view| view.key == SETTING && view.source == "project")
        .map_or_else(|| Value::Object(Default::default()), |view| view.value);
    let mut effective = global.as_object().cloned().unwrap_or_default();
    if let Some(own) = own.as_object() {
        effective.extend(
            own.iter()
                .map(|(engine, model)| (engine.clone(), model.clone())),
        );
    }
    Ok((global, own, Value::Object(effective)))
}

fn named<'a>(map: &'a Value, engine: &str) -> Option<&'a str> {
    map.get(engine)
        .and_then(Value::as_str)
        .filter(|model| !model.is_empty())
}

fn offered(models: &[ModelInfo], model: &str) -> bool {
    models.iter().any(|known| known.id == model)
}

/// The small model in a list, if one is recognisable by name.
fn smallest(models: &[ModelInfo]) -> Option<String> {
    SMALL_MODELS.iter().find_map(|family| {
        models
            .iter()
            .find(|model| model.id.to_lowercase().contains(family))
            .map(|model| model.id.clone())
    })
}

/// The resolver, over already-read values: the engine's own map entry, then the
/// legacy name, then its small model, then the thread's model. Only names the engine
/// lists are used; the thread's own model is the last resort and is not checked.
fn choose(
    engine: &str,
    per_engine: &Value,
    legacy: &str,
    models: &[ModelInfo],
    thread_model: Option<&str>,
) -> Option<TextModel> {
    if let Some(model) = named(per_engine, engine).filter(|model| offered(models, model)) {
        return Some(TextModel {
            model: model.to_owned(),
            reason: TextModelReason::Chosen,
        });
    }
    let legacy = (!legacy.trim().is_empty()).then_some(legacy);
    if let Some(model) = legacy.filter(|model| offered(models, model)) {
        return Some(TextModel {
            model: model.to_owned(),
            reason: TextModelReason::Legacy,
        });
    }
    if let Some(model) = smallest(models) {
        return Some(TextModel {
            model,
            reason: TextModelReason::Small,
        });
    }
    thread_model
        .filter(|model| !model.trim().is_empty())
        .map(|model| TextModel {
            model: model.to_owned(),
            reason: TextModelReason::Thread,
        })
}

/// The one-shot model for a thread's engine in a project. A failed model read is an
/// empty list, so the thread's own model still applies instead of an error.
pub(crate) async fn text_model_for(
    store: &Store,
    engines: &Arc<EngineManager>,
    engine: &str,
    project_id: Option<&str>,
    thread_model: Option<&str>,
) -> Result<Option<TextModel>> {
    let (_, _, per_engine) = layers(store, project_id)?;
    let legacy = stored_preference(store, LEGACY_SETTING, project_id)?;
    let legacy = legacy.as_str().unwrap_or_default();
    let models = engines
        .models(engine, false)
        .await
        .map(|catalog| catalog.models)
        .unwrap_or_default();
    Ok(choose(engine, &per_engine, legacy, &models, thread_model))
}

impl Core {
    /// Every known engine that is not turned off for the scope, with its models
    /// and the global and project choices. A read: it starts no run and spends nothing.
    pub async fn text_generators(&self, project_id: Option<&str>) -> Result<TextGenerators> {
        let (global_map, _, _) = layers(&self.store, None)?;
        let global_legacy = self.preference(LEGACY_SETTING, None)?;
        let project_scope = match project_id {
            Some(id) => {
                let (_, own, effective) = layers(&self.store, Some(id))?;
                Some((
                    id.to_owned(),
                    effective,
                    own,
                    self.preference(LEGACY_SETTING, Some(id))?,
                ))
            }
            None => None,
        };
        let disabled = self.preference("disabled_engines", project_id)?;
        let off = |id: &str| {
            disabled
                .as_array()
                .is_some_and(|list| list.iter().any(|item| item.as_str() == Some(id)))
        };
        let mut engines = Vec::new();
        for status in self.engines.statuses(false).await? {
            if off(&status.id) {
                continue;
            }
            let all = self
                .engines
                .models(&status.id, false)
                .await
                .map(|catalog| catalog.models)
                .unwrap_or_default();
            let global = choose(
                &status.id,
                &global_map,
                global_legacy.as_str().unwrap_or_default(),
                &all,
                None,
            );
            let chosen = named(&global_map, &status.id).map(str::to_owned);
            let project = project_scope.as_ref().map(|(id, map, own, legacy)| {
                let choice = choose(
                    &status.id,
                    map,
                    legacy.as_str().unwrap_or_default(),
                    &all,
                    None,
                );
                TextGeneratorProject {
                    project_id: id.clone(),
                    chosen: named(map, &status.id).map(str::to_owned),
                    own: named(own, &status.id).is_some(),
                    resolved: choice.as_ref().map(|choice| choice.model.clone()),
                    reason: choice.map(|choice| choice.reason),
                }
            });
            let mut models = all;
            models.truncate(MODELS_PER_ENGINE);
            engines.push(TextGeneratorEngine {
                engine: status.id,
                name: status.name,
                models,
                chosen,
                resolved: global.as_ref().map(|choice| choice.model.clone()),
                reason: global.map(|choice| choice.reason),
                project,
            });
        }
        Ok(TextGenerators { engines })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use async_trait::async_trait;
    use orbyn_contracts::{
        AuthState, EngineCapabilities, EngineTier, ModelInfo, PermissionMode, TextModelReason,
    };
    use serde_json::json;

    use super::*;
    use crate::agents::driver::{
        EngineDescriptor, EngineDriver, EngineEvent, EngineProbe, RunHandle, RunSpec,
    };
    use crate::app::CoreConfig;
    use crate::store::NewProject;
    use crate::testing::TempDir;

    fn model(id: &str) -> ModelInfo {
        ModelInfo {
            id: id.to_owned(),
            label: id.to_owned(),
            description: None,
            is_default: false,
            custom: false,
            options: Vec::new(),
            input_modalities: Vec::new(),
            upgrade: None,
        }
    }

    /// A driver that answers with its own catalog, or fails to list.
    struct CatalogDriver {
        id: String,
        models: Vec<ModelInfo>,
        failing: bool,
    }

    #[async_trait]
    impl EngineDriver for CatalogDriver {
        fn descriptor(&self) -> EngineDescriptor {
            EngineDescriptor {
                id: self.id.clone(),
                name: self.id.to_uppercase(),
                tier: EngineTier::Terminal,
                capabilities: EngineCapabilities {
                    streaming: false,
                    approvals: false,
                    resume: false,
                    mcp: false,
                    sandbox: false,
                    managed: true,
                    skill_delivery: Vec::new(),
                    goal_control: false,
                    decline_feedback: false,
                    permission_modes: PermissionMode::ALL.to_vec(),
                },
            }
        }

        async fn probe(&self) -> EngineProbe {
            EngineProbe {
                path: Some(PathBuf::from("test-engine")),
                auth: Some(AuthState::Unknown),
                ..EngineProbe::default()
            }
        }

        async fn start_run(
            &self,
            _spec: RunSpec,
            _events: tokio::sync::mpsc::Sender<EngineEvent>,
        ) -> Result<RunHandle> {
            Err(CoreError::Unsupported("no test run".into()))
        }

        async fn models(&self) -> Result<Option<crate::agents::driver::ModelCatalog>> {
            if self.failing {
                return Err(CoreError::EngineUnavailable(
                    "the test engine failed".into(),
                ));
            }
            Ok(Some(crate::agents::driver::ModelCatalog::engine(
                self.models.clone(),
            )))
        }
    }

    fn core(dir: &TempDir) -> Core {
        let mut config = CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
        config.engines.lookup_latest = false;
        config.engines.known_engine_ids = Some(Vec::new());
        let absent = dir.path().join("absent-native");
        config.engines.claude_program = absent.join("claude").to_string_lossy().into_owned();
        config.engines.codex_program = absent.join("codex").to_string_lossy().into_owned();
        Core::open(config).unwrap()
    }

    fn project(core: &Core, dir: &TempDir) -> String {
        let path = dir.path().join("project");
        std::fs::create_dir_all(&path).unwrap();
        core.store
            .add_project(NewProject {
                path: path.to_str().unwrap(),
                name: "project",
                is_git: false,
                managed: false,
                scratch: false,
            })
            .unwrap()
            .id
    }

    fn set(core: &Core, project: Option<&str>, value: Value) {
        core.set_preferences(project, &BTreeMap::from([(SETTING.to_owned(), value)]))
            .unwrap();
    }

    fn resolved(model: &str, reason: TextModelReason) -> Option<TextModel> {
        Some(TextModel {
            model: model.to_owned(),
            reason,
        })
    }

    #[test]
    fn the_order_is_chosen_legacy_small_then_thread() {
        let models = [model("big"), model("small")];
        assert_eq!(
            choose("e", &json!({"e": "big"}), "", &models, Some("thread")),
            resolved("big", TextModelReason::Chosen)
        );
        assert_eq!(
            choose("e", &json!({}), "big", &models, Some("thread")),
            resolved("big", TextModelReason::Legacy)
        );
        assert_eq!(
            choose(
                "e",
                &json!({"e": "other"}),
                "other",
                &models,
                Some("thread")
            ),
            resolved("small", TextModelReason::Small)
        );
        assert_eq!(
            choose(
                "e",
                &json!({"e": "other"}),
                "",
                &[model("big")],
                Some("thread")
            ),
            resolved("thread", TextModelReason::Thread)
        );
        assert_eq!(choose("e", &json!({}), "", &[model("big")], None), None);
    }

    #[test]
    fn an_entry_belongs_to_its_own_engine_only() {
        // One map entry for `one`; `two` has a different catalog and must never
        // receive `one`'s unknown name.
        let map = json!({"one": "one-model"});
        let two = [model("two-model"), model("two-small")];
        assert_eq!(
            choose("two", &map, "", &two, Some("thread")),
            resolved("two-small", TextModelReason::Small)
        );
        // A shared name is still only taken from `two`'s own catalog.
        let shared = [model("one-model"), model("two-small")];
        assert_eq!(
            choose("two", &map, "", &shared, Some("thread")),
            resolved("two-small", TextModelReason::Small)
        );
        assert_eq!(
            choose("one", &map, "", &[model("one-model")], None),
            resolved("one-model", TextModelReason::Chosen)
        );
    }

    #[test]
    fn the_map_is_an_object_of_at_most_64_plain_ids() {
        assert!(validate_syntax(&json!({"one": "big"})).is_ok());
        assert!(validate_syntax(&json!({})).is_ok());
        assert!(validate_syntax(&json!(["one", "big"])).is_err());
        assert!(validate_syntax(&json!({"one": 1})).is_err());
        assert!(validate_syntax(&json!({"a/b": "big"})).is_err());
        assert!(validate_syntax(&json!({"": "big"})).is_err());
        assert!(validate_syntax(&json!({"one": ""})).is_err());
        assert!(validate_syntax(&json!({"one": "has space"})).is_err());
        assert!(validate_syntax(&json!({"one": "-flag"})).is_err());
        assert!(validate_syntax(&json!({"one": "x".repeat(MAX_MODEL_ID + 1)})).is_err());
        let many: serde_json::Map<String, Value> = (0..MAX_ENGINES)
            .map(|index| (format!("engine{index}"), json!("model")))
            .collect();
        assert!(validate_syntax(&Value::Object(many.clone())).is_ok());
        let mut too_many = many;
        too_many.insert("one-more".into(), json!("model"));
        assert!(validate_syntax(&Value::Object(too_many)).is_err());
    }

    #[tokio::test]
    async fn a_write_refuses_a_model_the_engine_lists_none_of() {
        let dir = TempDir::new();
        let core = core(&dir);
        core.engines.register(Arc::new(CatalogDriver {
            id: "one".into(),
            models: vec![model("big")],
            failing: false,
        }));
        core.engines.models("one", false).await.unwrap();

        for (name, value) in [
            ("not an object", json!(["one", "big"])),
            (
                "too many",
                Value::Object(
                    (0..=MAX_ENGINES)
                        .map(|index| (format!("engine{index}"), json!("big")))
                        .collect(),
                ),
            ),
            ("bad engine", json!({"a/b": "big"})),
            ("bad model", json!({"one": "has space"})),
            ("unlisted", json!({"one": "other"})),
        ] {
            let error = core
                .set_preferences(None, &BTreeMap::from([(SETTING.to_owned(), value)]))
                .unwrap_err();
            assert!(matches!(error, CoreError::Invalid(_)), "{name}: {error}");
        }
        set(&core, None, json!({"one": "big"}));
        assert_eq!(
            core.preference(SETTING, None).unwrap(),
            json!({"one": "big"})
        );
    }

    #[tokio::test]
    async fn a_project_entry_wins_over_the_global_one() {
        let dir = TempDir::new();
        let core = core(&dir);
        core.engines.register(Arc::new(CatalogDriver {
            id: "one".into(),
            models: vec![model("big"), model("small")],
            failing: false,
        }));
        core.engines.models("one", false).await.unwrap();
        let project = project(&core, &dir);
        set(&core, None, json!({"one": "big"}));
        set(&core, Some(&project), json!({"one": "small"}));

        let global = text_model_for(&core.store, &core.engines, "one", None, None)
            .await
            .unwrap();
        assert_eq!(global, resolved("big", TextModelReason::Chosen));
        let scoped = text_model_for(&core.store, &core.engines, "one", Some(&project), None)
            .await
            .unwrap();
        assert_eq!(scoped, resolved("small", TextModelReason::Chosen));
    }

    #[tokio::test]
    async fn the_read_lists_an_engines_models_choice_reason_and_project() {
        let dir = TempDir::new();
        let core = core(&dir);
        let mut many = vec![model("big"), model("small")];
        many.extend((0..203).map(|index| model(&format!("extra{index}"))));
        core.engines.register(Arc::new(CatalogDriver {
            id: "one".into(),
            models: many,
            failing: false,
        }));
        core.engines.register(Arc::new(CatalogDriver {
            id: "two".into(),
            models: Vec::new(),
            failing: true,
        }));
        core.engines.models("one", false).await.unwrap();
        let project = project(&core, &dir);
        set(&core, None, json!({"one": "big", "two": "missing"}));
        set(&core, Some(&project), json!({"one": "small"}));

        let read = core.text_generators(Some(&project)).await.unwrap();
        let one = read.engines.iter().find(|e| e.engine == "one").unwrap();
        assert_eq!(one.name, "ONE");
        assert_eq!(one.models.len(), MODELS_PER_ENGINE);
        assert_eq!(one.models[0].id, "big");
        assert_eq!(one.chosen.as_deref(), Some("big"));
        assert_eq!(one.resolved.as_deref(), Some("big"));
        assert_eq!(one.reason, Some(TextModelReason::Chosen));
        let scoped = one.project.as_ref().unwrap();
        assert_eq!(scoped.project_id, project);
        assert_eq!(scoped.chosen.as_deref(), Some("small"));
        assert_eq!(scoped.resolved.as_deref(), Some("small"));
        assert_eq!(scoped.reason, Some(TextModelReason::Chosen));
        assert!(scoped.own, "the project's own map names this engine");

        // A failed read is an empty list, never an error.
        let two = read.engines.iter().find(|e| e.engine == "two").unwrap();
        assert!(two.models.is_empty());
        assert_eq!(two.chosen.as_deref(), Some("missing"));
        assert_eq!(two.resolved, None);
        assert_eq!(two.reason, None);
        // An engine the project does not name inherits the global choice, and says it is not the project's own.
        let inherited = two.project.as_ref().unwrap();
        assert_eq!(inherited.chosen.as_deref(), Some("missing"));
        assert!(!inherited.own);

        // Clearing is a write without the key: the project then inherits the global choice for that engine.
        set(&core, Some(&project), json!({}));
        let read = core.text_generators(Some(&project)).await.unwrap();
        let one = read.engines.iter().find(|e| e.engine == "one").unwrap();
        let scoped = one.project.as_ref().unwrap();
        assert_eq!(scoped.chosen.as_deref(), Some("big"));
        assert!(!scoped.own);
        assert_eq!(scoped.resolved.as_deref(), Some("big"));
        set(&core, Some(&project), json!({"one": "small"}));

        // The wire shape keeps nulls and the nested project object.
        let value = serde_json::to_value(&read).unwrap();
        let engines = value["engines"].as_array().unwrap();
        assert!(engines.iter().any(|entry| entry["reason"].is_null()));
        assert!(
            engines
                .iter()
                .any(|entry| entry["project"]["project_id"] == project)
        );

        core.set_preferences(
            Some(&project),
            &BTreeMap::from([("disabled_engines".to_owned(), json!(["one"]))]),
        )
        .unwrap();
        let read = core.text_generators(Some(&project)).await.unwrap();
        assert!(!read.engines.iter().any(|entry| entry.engine == "one"));
        assert!(read.engines.iter().any(|entry| entry.engine == "two"));
    }
}
