//! Running an Orbyn tool: the approval gate, then the call, then the audit record.

use super::definitions::TOOLS;
use super::guide::{guide_head, guide_stops};
use super::preview::{page_act, wait_target};
use super::render::{render_height, render_title};
use super::{
    ISSUE_NOTE, MAX_BATCH, MAX_HITS, MAX_SEARCH_CHARS, NO_ISSUE_READER, PULL_NOTE, device_button,
    device_orientation, device_tool_summary, hinge_angle, hinge_preset, is_device_tool,
    issue_query, issue_ref_value, issue_summary_value, matches_words, number, optional_text,
    pull_check_name, pull_checks_value, pull_comment_body, pull_detail_value, pull_filter,
    pull_limit, pull_list_value, pull_merge_args, pull_merge_refusal, pull_number, pull_reply_to,
    pull_review, pull_update_args, strings, text, wait_seconds,
};
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::governance::approvals::{ActionAsk, Verdict};
use crate::issues::ProjectReader;
use crate::store::AuditRecord;
use crate::tools::mcp::McpBinding;
use orbyn_contracts::{
    ActionKind, AddPullComment, CommentIssue, CreateIssue, CreateThread, DelegateTask,
    DeviceActionInput, DeviceActionOutput, IssueKey, MergePull, PermissionMode, TaskSchedule,
    Thread, UpdateThread, UpsertTask,
};
use serde_json::{Value, json};
use std::path::Path;

/// The key an issue tool asks for, in the tracker and scope its project's own reader named.
/// The request cannot supply either, so a key outside this project's scope is impossible by
/// construction.
fn issue_key(args: &Value, reader: &ProjectReader) -> Result<IssueKey> {
    Ok(IssueKey {
        tracker: reader.tracker,
        scope: reader.scope.clone(),
        key: text(args, "key")?.to_owned(),
    })
}

/// The most base64 image one tool reply can carry under its 256 KiB limit, with room for the
/// device row and the rest of the answer. A cut reply would not parse.
const MAX_TOOL_IMAGE_BASE64_BYTES: usize = 192 * 1024;

/// The screenshot tool's answer: the device route's own output. A screenshot over the reply
/// limit was still taken and read back, so the answer keeps the device and says the image's
/// size instead of carrying bytes that a cut reply would lose.
fn screenshot_value(output: DeviceActionOutput) -> Result<Value> {
    let mut answer = serde_json::to_value(&output)?;
    let Some(image) = output.image else {
        return Ok(answer);
    };
    if image.len() <= MAX_TOOL_IMAGE_BASE64_BYTES {
        return Ok(answer);
    }
    let map = answer
        .as_object_mut()
        .expect("a device action output is an object");
    let _ = map.remove("image");
    map.insert("image_base64_bytes".into(), json!(image.len()));
    map.insert(
        "note".into(),
        json!(format!(
            "The screenshot is {} bytes as base64. It is over the tool's 192 KiB image cap, so the image is left out.",
            image.len()
        )),
    );
    Ok(answer)
}

/// The device id an audit row may hold. The target is the caller's own text, so only a short
/// id with no path separator or control character is kept; anything else is recorded as "-" so a
/// path can never reach the audit row. The call itself still gets the id as given.
fn audit_device_target(id: &str) -> &str {
    if (1..=128).contains(&id.chars().count())
        && !id
            .chars()
            .any(|character| matches!(character, '/' | '\\') || character.is_control())
    {
        id
    } else {
        "-"
    }
}

impl Core {
    /// The caller project's issue reader, or the one refusal every issue tool shares.
    async fn issue_reader(&self, own: &Thread) -> Result<ProjectReader> {
        let project = self.store.project(&own.project_id)?;
        self.project_issue_reader(&project)
            .await?
            .ok_or_else(|| CoreError::Invalid(NO_ISSUE_READER.into()))
    }

    /// Runs one of Orbyn's tools for `binding`'s run. A tool with an effect is
    /// refused unless the gate allows it, and the gate's reason comes back.
    pub async fn call_orbyn_tool(
        &self,
        binding: &McpBinding,
        name: &str,
        args: &Value,
    ) -> Result<Value> {
        let tool = TOOLS
            .iter()
            .find(|tool| tool.name == name)
            .ok_or_else(|| CoreError::NotFound(format!("tool {name}")))?;
        // The device tools are listed only where this machine has platform tools, so
        // elsewhere the name is unknown, as if the table never held it.
        if is_device_tool(name) && !self.devices.tools_available() {
            return Err(CoreError::NotFound(format!("tool {name}")));
        }
        self.refuse_for_crew_worker(binding, name, args)?;
        let preview = self.project_tool_preview(binding, name, args).await?;
        let args = preview.as_ref().unwrap_or(args);
        if tool.effect {
            let summary = match name {
                "orbyn_project_create" => format!(
                    "Add existing folder {} to Orbyn as untrusted; keep all files",
                    text(args, "path")?
                ),
                "orbyn_project_delete" => format!(
                    "Remove project {} at {} from Orbyn; keep all files",
                    text(args, "project_id")?,
                    text(args, "path")?
                ),
                "orbyn_pull_request_comment" => format!(
                    "Comment on pull request #{} of this project",
                    number(args, "number")?
                ),
                "orbyn_issue_create" => format!(
                    "Create issue \"{}\" in this project's tracker",
                    text(args, "title")?
                ),
                "orbyn_issue_comment" => format!(
                    "Comment on issue {} in this project's tracker",
                    text(args, "key")?
                ),
                "orbyn_pull_request_update" => {
                    let named: Vec<&str> = ["title", "body", "draft"]
                        .into_iter()
                        .filter(|field| args.get(*field).is_some_and(|value| !value.is_null()))
                        .collect();
                    format!(
                        "Update pull request #{} of this project ({})",
                        number(args, "number")?,
                        named.join(", ")
                    )
                }
                "orbyn_pull_request_review" => {
                    let review = pull_review(args)?;
                    format!(
                        "Submit a {} review on pull request #{} of this project",
                        review.event.replace('_', " "),
                        number(args, "number")?
                    )
                }
                "orbyn_pull_request_merge" => {
                    let (method, _, delete_branch) = pull_merge_args(args)?;
                    let mut summary = format!(
                        "Merge pull request #{} of this project ({method})",
                        number(args, "number")?
                    );
                    if delete_branch {
                        summary.push_str(" and delete its branch");
                    }
                    summary
                }
                "orbyn_pull_request_ci_rerun" => format!(
                    "Re-run the failed jobs of check {} on pull request #{} of this project",
                    pull_check_name(args)?,
                    number(args, "number")?
                ),
                // A device tool checks its own arguments here, before the gate asks, so a
                // value the service would refuse never becomes an approval request.
                name if is_device_tool(name) => device_tool_summary(name, args)?,
                _ => format!("Use {name}"),
            };
            let ask = ActionAsk {
                kind: ActionKind::Tool,
                target: name.to_owned(),
                summary,
                detail: args.clone(),
                paths: Vec::new(),
            };
            if let Verdict::Deny(reason) = self.gate.request(&binding.scope, ask).await {
                return Err(CoreError::PolicyDenied(reason));
            }
        }
        // The device tools carry the deepest chain on the tool surface, so they run before the
        // general dispatcher's large frame exists, and their own future lives on the heap: a
        // debug build's transport call must fit a normal thread's stack (#1515).
        if is_device_tool(name) {
            return Box::pin(self.run_device_tool(binding, name, args)).await;
        }
        self.run_orbyn_tool(binding, name, args).await
    }

    /// A new thread in the caller's project with a first message.
    pub(super) async fn launch(
        &self,
        own: &Thread,
        spec: &Value,
        worktree: Option<String>,
    ) -> Result<Value> {
        let string = |key: &str| spec.get(key).and_then(Value::as_str).map(str::to_owned);
        let thread = self
            .create_thread(CreateThread {
                project_id: Some(own.project_id.clone()),
                engine: string("engine").unwrap_or_else(|| own.engine.clone()),
                model: None,
                title: string("title"),
                worktree,
                skill_profile: own.skill_profile.clone(),
                base_branch: string("base_branch"),
                effort: None,
                options: std::collections::BTreeMap::new(),
            })
            .await?;
        self.runs.enqueue(&thread.id, text(spec, "prompt")?, &[])?;
        Ok(json!({ "thread_id": thread.id }))
    }

    /// A checkout file as an attachment. Trust, `..`, links and `.git` are the same
    /// checks as any file route, and a thread can only draw on its own checkout.
    pub(super) fn attach_from_checkout(
        &self,
        own: &Thread,
        target: &str,
        path: &str,
    ) -> Result<orbyn_contracts::Attachment> {
        let bytes = self.open_raw_file(&own.id, path, true)?;
        let mut body = Vec::new();
        std::io::Read::read_to_end(&mut &bytes.file, &mut body)?;
        self.add_attachment(target, &bytes.name, "application/octet-stream", &body)
    }

    #[allow(clippy::too_many_lines)]
    pub(super) async fn run_orbyn_tool(
        &self,
        binding: &McpBinding,
        name: &str,
        args: &Value,
    ) -> Result<Value> {
        let actor = format!("engine:{}", binding.scope.engine);
        let own = self.store.thread(&binding.scope.thread_id)?;
        let thread_arg = || self.project_thread(binding, text(args, "thread_id")?);
        if name.starts_with("orbyn_preview")
            && self.preference("enable_agent_browser_access", Some(&own.project_id))?
                == json!(false)
        {
            return Err(CoreError::Invalid(
                "Agents can't use the browser here. It's turned off in Settings.".into(),
            ));
        }
        Ok(match name {
            "orbyn_environment_read" => json!({
                "version": env!("CARGO_PKG_VERSION"),
                "platform": std::env::consts::OS,
                "thread_id": own.id,
                "project_id": own.project_id,
                "checkout": own.cwd,
                "engine": own.engine,
            }),
            "orbyn_project_read" => json!({
                "project": self.store.project(&own.project_id)?,
                "file": self.project_file(&own.id)?,
            }),
            "orbyn_project_list" => json!([self.store.project(&own.project_id)?]),
            "orbyn_project_create" => {
                let approved = text(args, "path")?;
                let root = crate::project::workspace::resolve_project(
                    &self.workspace.root()?,
                    Path::new(approved),
                )
                .await?;
                if root.path != Path::new(approved) {
                    return Err(CoreError::Invalid(
                        "The approved folder changed; ask again.".into(),
                    ));
                }
                let name = root.path.file_name().map_or_else(
                    || "Project".to_owned(),
                    |name| name.to_string_lossy().into_owned(),
                );
                json!(self.store.insert_agent_project(
                    approved,
                    &name,
                    root.is_git,
                    &binding.scope
                )?)
            }
            "orbyn_project_delete" => {
                let id = text(args, "project_id")?;
                let threads: Vec<String> = serde_json::from_value(args["thread_ids"].clone())?;
                let removed = self.store.remove_agent_project(
                    id,
                    text(args, "path")?,
                    &threads,
                    &self.workspace.root()?.to_string_lossy(),
                    &binding.scope,
                )?;
                for thread in removed {
                    self.setups.forget(&thread);
                    self.terminals.close(&thread);
                }
                json!({ "project_id": id, "removed": true, "files_removed": false, "kept_path": args["path"] })
            }
            "orbyn_worktree_list" => {
                let project = self.store.project(&own.project_id)?;
                let found =
                    crate::source_control::git::linked_worktrees(Path::new(&project.path)).await;
                let listed: Vec<Value> = found
                    .unwrap_or_default()
                    .into_iter()
                    .map(|worktree| json!({ "path": worktree.path, "branch": worktree.branch }))
                    .collect();
                json!(listed)
            }
            "orbyn_thread_list" => json!(self.store.list_threads(Some(&own.project_id), false)?),
            "orbyn_thread_read" => {
                let thread = thread_arg()?;
                let text = self.store.recovery_handoff(&thread.id)?;
                json!({ "thread": thread, "text": text })
            }
            "orbyn_thread_search" => {
                let query = text(args, "query")?;
                let mut hits = Vec::new();
                for thread in self.store.list_threads(Some(&own.project_id), false)? {
                    let body = self.store.transcript(&thread.id, MAX_SEARCH_CHARS)?;
                    if matches_words(&format!("{} {body}", thread.title), query) {
                        hits.push(json!({ "id": thread.id, "title": thread.title }));
                        if hits.len() == MAX_HITS {
                            break;
                        }
                    }
                }
                json!(hits)
            }
            "orbyn_thread_wait" => {
                let thread = thread_arg()?;
                if thread.id == own.id {
                    return Err(CoreError::Invalid(
                        "A thread cannot wait for itself.".into(),
                    ));
                }
                let seconds = wait_seconds(args)?;
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
                loop {
                    let thread = self.store.thread(&thread.id)?;
                    let ended = thread.active_run.is_none();
                    if ended || std::time::Instant::now() >= deadline {
                        break json!({
                            "ended": ended,
                            "last_run_state": thread.last_run_state,
                        });
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                }
            }
            "orbyn_queue_list" => json!(self.store.list_queue(&thread_arg()?.id)?),
            "orbyn_pending_requests" => {
                let thread = thread_arg()?;
                json!({
                    "approvals": self.store.pending_for_thread(&thread.id)?,
                    "questions": self.store.list_questions(
                        Some(&thread.id),
                        Some(orbyn_contracts::QuestionState::Pending),
                    )?,
                })
            }
            "orbyn_pull_list" => json!({
                "linked": self.store.linked_pulls(&own.id, &own.project_id)?,
                "watched": self.thread_watches(&own.id)?,
            }),
            "orbyn_pull_request_list" => {
                let filter = pull_filter(args)?;
                let limit = pull_limit(args)?;
                let pulls = self.project_pulls(&own.project_id, &filter, false).await?;
                pull_list_value(&pulls, limit)
            }
            "orbyn_pull_request_read" => {
                let number = pull_number(args)?;
                let detail = self.project_pull(&own.project_id, number, false).await?;
                pull_detail_value(&detail, self.issue_body_for_tool(&detail.body))
            }
            "orbyn_pull_request_checks" => {
                let number = pull_number(args)?;
                let checks = self.pull_checks(&own.project_id, number, false).await?;
                pull_checks_value(number, &checks)
            }
            "orbyn_pull_request_ci_logs" => {
                let number = pull_number(args)?;
                let check = pull_check_name(args)?;
                let log = self.pull_check_log(&own.project_id, number, check).await?;
                json!({
                    "number": number,
                    "check": check,
                    "log": log,
                    "note": PULL_NOTE,
                })
            }
            "orbyn_pull_request_comment" => {
                let number = pull_number(args)?;
                let request = AddPullComment {
                    body: pull_comment_body(args)?.to_owned(),
                    path: None,
                    line: None,
                    reply_to: pull_reply_to(args)?.map(str::to_owned),
                };
                self.comment_pull(&own.project_id, number, &request, &actor)
                    .await?;
                json!({ "commented": true, "number": number })
            }
            "orbyn_pull_request_update" => {
                let number = pull_number(args)?;
                let update = pull_update_args(args)?;
                let detail = self
                    .update_pull(&own.project_id, number, &update, &actor)
                    .await?;
                pull_detail_value(&detail, self.issue_body_for_tool(&detail.body))
            }
            "orbyn_pull_request_review" => {
                let number = pull_number(args)?;
                let review = pull_review(args)?;
                self.submit_pull_review(&own.project_id, number, &review, &actor)
                    .await?;
                json!({ "reviewed": true, "number": number, "event": review.event })
            }
            "orbyn_pull_request_merge" => {
                let number = pull_number(args)?;
                let (method, head_sha, delete_branch) = pull_merge_args(args)?;
                let detail = self.project_pull(&own.project_id, number, true).await?;
                if let Some(refusal) = pull_merge_refusal(&detail, &head_sha) {
                    return Err(CoreError::Invalid(refusal.into()));
                }
                let options = MergePull {
                    method: method.clone(),
                    auto: false,
                    delete_branch,
                    subject: None,
                    body: None,
                    strip_credits: false,
                    remove_agent_credits: None,
                    expected_head: detail.head_sha.clone(),
                };
                self.merge_project_pull(&own.project_id, number, &options, &actor)
                    .await?;
                json!({ "merged": true, "number": number, "method": method })
            }
            "orbyn_pull_request_ci_rerun" => {
                let number = pull_number(args)?;
                let check = pull_check_name(args)?;
                self.rerun_pull_checks(&own.project_id, number, check, &actor)
                    .await?;
                json!({ "rerun": true, "number": number, "check": check })
            }
            "orbyn_schedule_list" => {
                let tasks = self.store.tasks()?;
                let tasks: Vec<_> = tasks
                    .into_iter()
                    .filter(|task| task.project_id == own.project_id)
                    .collect();
                json!(tasks)
            }
            "orbyn_task_status" => {
                let thread = thread_arg()?;
                json!({
                    "active": thread.active_run.is_some(),
                    "last_run_state": thread.last_run_state,
                })
            }
            "orbyn_track_files" => {
                let paths: Vec<String> = args
                    .get("paths")
                    .and_then(Value::as_array)
                    .ok_or_else(|| CoreError::Invalid("paths is required.".into()))?
                    .iter()
                    .filter_map(|path| path.as_str().map(str::to_owned))
                    .take(200)
                    .collect();
                let done = args.get("done").and_then(Value::as_bool).unwrap_or(false);
                crate::conversation::file_changes::record(
                    &self.store,
                    &binding.scope,
                    &paths,
                    done,
                );
                json!({ "tracked": paths.len() })
            }
            "orbyn_thread_launch" => self.launch(&own, args, None).await?,
            "orbyn_threads_create" => {
                let specs = args
                    .get("threads")
                    .and_then(Value::as_array)
                    .filter(|specs| specs.len() <= MAX_BATCH)
                    .ok_or_else(|| {
                        CoreError::Invalid(format!("threads is a list of up to {MAX_BATCH}."))
                    })?;
                let mut results = Vec::with_capacity(specs.len());
                for spec in specs {
                    results.push(match self.launch(&own, spec, None).await {
                        Ok(started) => started,
                        Err(error) => json!({ "error": error.public_message() }),
                    });
                }
                json!(results)
            }
            "orbyn_worktree_handoff" => {
                let purpose = text(args, "purpose")?.to_owned();
                self.launch(&own, args, Some(purpose)).await?
            }
            "orbyn_worktree_status" => {
                let checkout = Path::new(&own.cwd);
                let status =
                    crate::source_control::git::git(checkout, &["status", "--porcelain", "-b"])
                        .await?;
                let status = String::from_utf8_lossy(&status);
                let mut lines = status.lines();
                let branch = lines.next().and_then(|line| line.strip_prefix("## "));
                let project = self.store.project(&own.project_id)?;
                json!({
                    "checkout": own.cwd,
                    "branch": branch,
                    "worktree": Path::new(&project.path) != checkout,
                    "changed": lines.map(|line| line.get(3..).unwrap_or(line)).collect::<Vec<_>>(),
                })
            }
            "orbyn_orchestrator_capabilities" => {
                let nested =
                    own.lifecycle.as_ref().and_then(|l| l.relation.as_deref()) == Some("subagent");
                let engines: Vec<Value> = self
                    .engines
                    .statuses(false)
                    .await?
                    .into_iter()
                    .map(|engine| json!({ "id": engine.id, "name": engine.name }))
                    .collect();
                json!({
                    "engines": engines,
                    "can_delegate": !nested,
                    "max_batch": MAX_BATCH,
                })
            }
            "orbyn_environment_preferences_update" => {
                let values = args
                    .get("values")
                    .and_then(Value::as_object)
                    .ok_or_else(|| CoreError::Invalid("values is required.".into()))?
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect();
                self.set_preferences(Some(&own.project_id), &values)?;
                json!({ "updated": values.len() })
            }
            "orbyn_project_update" => {
                json!(self.rename_project(&own.project_id, text(args, "name")?)?)
            }
            "orbyn_thread_send" => {
                let thread = thread_arg()?;
                let attachments: Vec<String> = args
                    .get("attachment_ids")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|id| id.as_str().map(str::to_owned))
                    .collect();
                json!(
                    self.runs
                        .enqueue(&thread.id, text(args, "prompt")?, &attachments)?
                )
            }
            "orbyn_attachment_add" => {
                let thread = thread_arg()?;
                json!(self.attach_from_checkout(&own, &thread.id, text(args, "path")?)?)
            }
            "orbyn_attachment_discard" => {
                let thread = thread_arg()?;
                let id = text(args, "attachment_id")?;
                let held = self.store.list_attachments(&thread.id)?;
                if !held.iter().any(|(attachment, _)| attachment.id == id) {
                    return Err(CoreError::NotFound("attachment".into()));
                }
                self.remove_attachment(id)?;
                json!({ "discarded": true })
            }
            "orbyn_thread_organize" => {
                let thread = thread_arg()?;
                let flag = |key: &str| args.get(key).and_then(Value::as_bool);
                let patch = UpdateThread {
                    pinned: flag("pinned"),
                    settled: flag("settled"),
                    archived: flag("archived"),
                    unread: flag("unread"),
                    snoozed_until: args
                        .get("snoozed_until")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    ..UpdateThread::default()
                };
                json!(self.update_thread(&thread.id, &patch, &actor)?)
            }
            "orbyn_thread_configure" => {
                let thread = thread_arg()?;
                let string = |key: &str| args.get(key).and_then(Value::as_str).map(str::to_owned);
                let mode: Option<PermissionMode> = args
                    .get("permission_mode")
                    .filter(|value| !value.is_null())
                    .map(|value| serde_json::from_value(value.clone()))
                    .transpose()
                    .map_err(|_| CoreError::Invalid("That is not a permission mode.".into()))?;
                // A looser mode grants an engine more, so only the person may choose it, in the app (#441).
                if let Some(wanted) = mode {
                    let current = thread
                        .lifecycle
                        .as_ref()
                        .map_or(PermissionMode::Supervised, |lifecycle| {
                            lifecycle.permission_mode
                        });
                    if wanted.looser_than(current) {
                        return Err(CoreError::Invalid(
                            "An agent can't loosen a thread's permission mode. Ask the person to change it in the app."
                                .into(),
                        ));
                    }
                }
                let patch = UpdateThread {
                    model: string("model"),
                    effort: string("effort"),
                    permission_mode: mode,
                    plan_mode: args.get("plan_mode").and_then(Value::as_bool),
                    ..UpdateThread::default()
                };
                json!(self.update_thread(&thread.id, &patch, &actor)?)
            }
            "orbyn_preview_list" => json!(self.previews.list(&own.id)),
            "orbyn_preview_open" => {
                let size = |key: &str| {
                    args.get(key)
                        .and_then(Value::as_u64)
                        .and_then(|value| u32::try_from(value).ok())
                };
                let url = text(args, "url")?;
                let opened = self.open_preview(
                    &own.id,
                    url,
                    (size("width"), size("height")),
                    &actor,
                    crate::browser::preview::OpenHow::default(),
                );
                json!(opened.await?)
            }
            "orbyn_preview_navigate" => {
                let tab = self.own_preview(&own, args)?;
                if let Some(url) = args.get("url").and_then(Value::as_str) {
                    json!(self.previews.navigate(&own.id, &tab, url).await?)
                } else {
                    let back = args.get("back").and_then(Value::as_bool);
                    let back = back.ok_or_else(|| {
                        CoreError::Invalid("Name a url, or back true or false.".into())
                    })?;
                    json!(self.previews.step(&own.id, &tab, back).await?)
                }
            }
            "orbyn_preview_resize" => {
                let tab = self.own_preview(&own, args)?;
                let side = |key: &str| {
                    u32::try_from(number(args, key)?)
                        .map_err(|_| CoreError::Invalid(format!("{key} is too large.")))
                };
                let (width, height) = (side("width")?, side("height")?);
                json!(self.previews.resize(&own.id, &tab, width, height).await?)
            }
            "orbyn_preview_status" => {
                let tab = self.own_preview(&own, args)?;
                let tabs = self.previews.list(&own.id);
                json!(tabs.into_iter().find(|view| view.id == tab))
            }
            "orbyn_preview_snapshot" => {
                let tab = self.own_preview(&own, args)?;
                let max_elements = args.get("max_elements").and_then(Value::as_u64);
                json!(self.previews.snapshot(&own.id, &tab, max_elements).await?)
            }
            "orbyn_preview_wait_for" => {
                let tab = self.own_preview(&own, args)?;
                let (selector, wanted) = wait_target(args)?;
                let timeout = args.get("timeout_ms").and_then(Value::as_u64);
                json!(
                    self.previews
                        .wait_for(
                            &own.id,
                            &tab,
                            selector.as_deref(),
                            wanted.as_deref(),
                            timeout
                        )
                        .await?
                )
            }
            "orbyn_preview_act" => {
                let tab = self.own_preview(&own, args)?;
                let step = page_act(args)?;
                json!(self.previews.act(&own.id, &tab, &step).await?)
            }
            "orbyn_preview_evaluate" => {
                let tab = self.own_preview(&own, args)?;
                let expression = text(args, "expression")?;
                json!(self.previews.evaluate(&own.id, &tab, expression).await?)
            }
            "orbyn_preview_set_appearance" => {
                let tab = self.own_preview(&own, args)?;
                let view = orbyn_contracts::PreviewView {
                    zoom: None,
                    appearance: Some(text(args, "appearance")?.to_owned()),
                    mobile: None,
                };
                json!(self.previews.set_view(&own.id, &tab, &view).await?)
            }
            "orbyn_preview_dialog" => {
                let tab = self.own_preview(&own, args)?;
                let accept = args
                    .get("accept")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| CoreError::Invalid("Say whether to accept.".into()))?;
                let answer = args.get("text").and_then(Value::as_str);
                let handled = self.previews.answer_dialog(&own.id, &tab, accept, answer);
                json!({ "handled": handled.await? })
            }
            "orbyn_preview_close" => {
                let tab = self.own_preview(&own, args)?;
                self.previews.close(&own.id, &tab)?;
                json!({ "closed": true })
            }
            "orbyn_preview_recording_start" => {
                let tab = self.own_preview(&own, args)?;
                self.start_preview_recording(&own.id, &tab, Default::default())?;
                json!({ "recording": true })
            }
            "orbyn_preview_recording_stop" => {
                let tab = self.own_preview(&own, args)?;
                json!(self.previews.stop_recording(&own.id, &tab).await?)
            }
            "orbyn_html_render" => {
                // The caller's own thread, so a page lands in the timeline that asked
                // for it. The route stays a path: the client adds `?render=<token>`.
                let render = self.publish_render(&own.id, text(args, "html")?)?;
                json!({
                    "id": render.id,
                    "url": render.url,
                    "frame_token": render.frame_token,
                    "title": render_title(args),
                    "images": render.images,
                    "notes": render.notes,
                    "bytes": render.bytes,
                    "height": render_height(args),
                })
            }
            "orbyn_thread_update" => {
                let thread = thread_arg()?;
                let patch = UpdateThread {
                    title: args.get("title").and_then(Value::as_str).map(str::to_owned),
                    pinned: args.get("pinned").and_then(Value::as_bool),
                    settled: args.get("settled").and_then(Value::as_bool),
                    ..UpdateThread::default()
                };
                json!(self.update_thread(&thread.id, &patch, &actor)?)
            }
            "orbyn_workflow_stop" => {
                let id = text(args, "thread_id")?;
                if id.len() > 128 {
                    return Err(CoreError::Invalid("thread_id is at most 128 bytes.".into()));
                }
                let thread = self.project_thread(binding, id)?;
                if !thread.lifecycle.as_ref().is_some_and(|lifecycle| {
                    lifecycle.relation.as_deref() == Some("subagent")
                        && lifecycle.parent_thread_id.as_deref() == Some(own.id.as_str())
                }) {
                    return Err(CoreError::NotFound("delegated thread".into()));
                }
                let stopped = if let Some(run) = thread.active_run {
                    self.runs.cancel(&run.id)?;
                    true
                } else {
                    false
                };
                json!({ "stopped": stopped })
            }
            "orbyn_thread_interrupt" | "orbyn_task_cancel" => {
                let thread = thread_arg()?;
                if thread.id == own.id {
                    return Err(CoreError::Invalid("A thread cannot stop itself.".into()));
                }
                let stopped = thread.active_run.map(|run| self.runs.cancel(&run.id));
                json!({ "stopped": matches!(stopped, Some(Ok(()))) })
            }
            "orbyn_pull_link" => json!(self.link_pull(&own.id, number(args, "number")?).await?),
            "orbyn_pull_unlink" => {
                self.store
                    .unlink_pull(&own.id, &own.project_id, number(args, "number")?)?;
                json!({ "unlinked": true })
            }
            "orbyn_pull_watch" => json!(self.watch_pull(&own.id, number(args, "number")?)?),
            "orbyn_pull_unwatch" => {
                self.store.unwatch_pull(&own.id, number(args, "number")?)?;
                json!({ "unwatched": true })
            }
            "orbyn_pull_guide_read" => {
                let number = pull_number(args)?;
                json!({
                    "number": number,
                    "guide": self.store.pull_guide(&own.project_id, number)?,
                })
            }
            "orbyn_pull_guide_write" => {
                let number = pull_number(args)?;
                let head_sha = guide_head(args)?;
                let (stops, count) = guide_stops(args)?;
                if self.preference("review_guides_enabled", Some(&own.project_id))? != json!(true) {
                    return Err(CoreError::Invalid(
                        "Review guides are off for this project.".into(),
                    ));
                }
                let guide = crate::store::PullGuideWrite {
                    head_sha: head_sha.as_deref(),
                    stops: &stops,
                    thread_id: &own.id,
                };
                let written_at = self
                    .store
                    .write_pull_guide(&own.project_id, number, &guide)?;
                json!({
                    "number": number,
                    "head_sha": head_sha,
                    "stops": count,
                    "written_at": written_at,
                })
            }
            "orbyn_issue_list" => {
                let reader = self.issue_reader(&own).await?;
                let query = issue_query(args, reader.tracker)?;
                let page = self
                    .scope_issues(reader.tracker, &reader.scope, &query)
                    .await?;
                json!({
                    "issues": page.issues.iter().map(issue_summary_value).collect::<Vec<_>>(),
                    "note": ISSUE_NOTE,
                })
            }
            "orbyn_issue_read" => {
                let reader = self.issue_reader(&own).await?;
                let key = issue_key(args, &reader)?;
                let detail = self.issue(&key).await?;
                let mut issue = issue_summary_value(&detail.summary);
                issue["body"] = json!(self.issue_body_for_tool(&detail.body));
                issue["note"] = json!(ISSUE_NOTE);
                issue
            }
            "orbyn_issue_create" => {
                let reader = self.issue_reader(&own).await?;
                let detail = self
                    .create_issue(CreateIssue {
                        tracker: reader.tracker,
                        scope: reader.scope.clone(),
                        title: text(args, "title")?.to_owned(),
                        body: optional_text(args, "body")?,
                        labels: strings(args, "labels")?,
                        assignees: strings(args, "assignees")?,
                        template: optional_text(args, "template")?,
                    })
                    .await?;
                let mut issue = issue_summary_value(&detail.summary);
                issue["body"] = json!(self.issue_body_for_tool(&detail.body));
                issue["note"] = json!(ISSUE_NOTE);
                issue
            }
            "orbyn_issue_comment" => {
                let reader = self.issue_reader(&own).await?;
                let key = issue_key(args, &reader)?;
                let comment = self
                    .comment_issue(CommentIssue {
                        tracker: reader.tracker,
                        scope: reader.scope.clone(),
                        key: key.key,
                        body: text(args, "body")?.to_owned(),
                    })
                    .await?;
                json!({
                    "id": comment.id,
                    "author": comment.author,
                    "created_at": comment.created_at,
                    "body": self.issue_body_for_tool(&comment.body),
                    "note": ISSUE_NOTE,
                })
            }
            "orbyn_issue_link" => {
                let reader = self.issue_reader(&own).await?;
                let key = issue_key(args, &reader)?;
                self.link_issue(&own.id, &key).await?;
                json!({ "linked": true })
            }
            "orbyn_issue_unlink" => {
                let reader = self.issue_reader(&own).await?;
                let key = issue_key(args, &reader)?;
                self.unlink_issue(&own.id, &key)?;
                json!({ "unlinked": true })
            }
            "orbyn_issue_thread_list" => {
                // Stored links only: a tracker that was disconnected later does not hide what this thread linked.
                let issues = self
                    .store
                    .issue_links()?
                    .get(&own.id)
                    .into_iter()
                    .flatten()
                    .map(issue_ref_value)
                    .collect::<Vec<_>>();
                json!({ "issues": issues })
            }
            "orbyn_delegate_task" => {
                let nested =
                    own.lifecycle.as_ref().and_then(|l| l.relation.as_deref()) == Some("subagent");
                if nested {
                    return Err(CoreError::Invalid(
                        "A delegated thread cannot delegate further.".into(),
                    ));
                }
                let task = DelegateTask {
                    prompt: text(args, "prompt")?.to_owned(),
                    title: args.get("title").and_then(Value::as_str).map(str::to_owned),
                    engine: args
                        .get("engine")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    model: None,
                    effort: None,
                    options: std::collections::BTreeMap::new(),
                };
                let child = self.delegate(&own.id, &task, &actor)?;
                json!({ "thread_id": child.id })
            }
            "orbyn_schedule_task" => {
                let minutes = u32::try_from(number(args, "every_minutes")?)
                    .map_err(|_| CoreError::Invalid("That interval is too long.".into()))?;
                let task = UpsertTask {
                    id: None,
                    project_id: own.project_id.clone(),
                    thread_id: None,
                    name: text(args, "name")?.to_owned(),
                    prompt: text(args, "prompt")?.to_owned(),
                    engine: own.engine.clone(),
                    schedule: TaskSchedule {
                        kind: "interval".into(),
                        every_minutes: Some(minutes),
                        time: None,
                        weekdays: Vec::new(),
                        utc_offset_minutes: 0,
                        after_thread: None,
                        only_on_success: false,
                    },
                    signature: None,
                    secret: None,
                    filters: Vec::new(),
                    dedupe: None,
                    max_age_seconds: None,
                    timestamp_path: None,
                };
                json!(self.save_task(task, &actor)?)
            }
            "orbyn_schedule_delete" => {
                let id = text(args, "task_id")?;
                self.project_task(binding, id)?;
                self.delete_task(id, &actor)?;
                json!({ "deleted": true })
            }
            "orbyn_schedule_run" => {
                let id = text(args, "task_id")?;
                self.project_task(binding, id)?;
                json!({ "thread_id": self.run_task(id).await? })
            }
            other => return Err(CoreError::NotFound(format!("tool {other}"))),
        })
    }

    /// The device tools' own dispatch, kept out of [`Self::run_orbyn_tool`]'s match: the
    /// screenshot's device chain is the deepest future on the tool surface, and a debug build's
    /// frame for the general dispatcher (about 450 KiB here) must not sit under it (#1515).
    async fn run_device_tool(
        &self,
        binding: &McpBinding,
        name: &str,
        args: &Value,
    ) -> Result<Value> {
        Ok(match name {
            "orbyn_device_list" => {
                // Rule 6: the row goes before the effect it describes, so it carries fixed
                // values only; the list's counts are the answer's, not the audit row's.
                self.audit_device_tool(binding, "device.tool.list", "", json!({}))?;
                // Boxed: the device call's future is deep, so it lives on the heap.
                let list = Box::pin(self.devices.list()).await;
                serde_json::to_value(&list)?
            }
            "orbyn_device_screenshot" => {
                let id = text(args, "device_id")?;
                // Rule 6: the row goes before the device call. The target is the caller's
                // text, so the sanitiser keeps a path or control character out of it.
                self.audit_device_tool(
                    binding,
                    "device.tool.screenshot",
                    audit_device_target(id),
                    json!({}),
                )?;
                let output =
                    Box::pin(self.devices.act(id, DeviceActionInput::Screenshot {})).await?;
                screenshot_value(output)?
            }
            "orbyn_device_fold" => {
                self.run_device_action(
                    binding,
                    args,
                    "device.tool.fold",
                    DeviceActionInput::Fold {},
                )
                .await?
            }
            "orbyn_device_unfold" => {
                self.run_device_action(
                    binding,
                    args,
                    "device.tool.unfold",
                    DeviceActionInput::Unfold {},
                )
                .await?
            }
            "orbyn_device_rotate" => {
                let value = device_orientation(text(args, "value")?)?;
                self.run_device_action(
                    binding,
                    args,
                    "device.tool.rotate",
                    DeviceActionInput::Rotate { value },
                )
                .await?
            }
            "orbyn_device_button" => {
                let value = device_button(text(args, "value")?)?;
                self.run_device_action(
                    binding,
                    args,
                    "device.tool.button",
                    DeviceActionInput::Button { value },
                )
                .await?
            }
            "orbyn_device_hinge" => {
                let angle = hinge_angle(args)?;
                self.run_device_action(
                    binding,
                    args,
                    "device.tool.hinge",
                    DeviceActionInput::Hinge { angle },
                )
                .await?
            }
            "orbyn_device_hinge_preset" => {
                let preset = hinge_preset(text(args, "preset")?)?;
                self.run_device_action(
                    binding,
                    args,
                    "device.tool.hinge_preset",
                    DeviceActionInput::HingePreset { preset },
                )
                .await?
            }
            "orbyn_device_power_off" => {
                self.run_device_action(
                    binding,
                    args,
                    "device.tool.power_off",
                    DeviceActionInput::PowerOff {},
                )
                .await?
            }
            other => return Err(CoreError::NotFound(format!("tool {other}"))),
        })
    }

    /// One typed device action a tool asks for: the audit row is written before the call it
    /// describes (rule 6), and the answer is the action route's own output.
    async fn run_device_action(
        &self,
        binding: &McpBinding,
        args: &Value,
        action: &str,
        input: DeviceActionInput,
    ) -> Result<Value> {
        let id = text(args, "device_id")?;
        self.audit_device_tool(binding, action, audit_device_target(id), json!({}))?;
        // Boxed like the list and screenshot calls, so one device action does not hold the
        // whole deep future on a debug thread's stack.
        let output = Box::pin(self.devices.act(id, input)).await?;
        Ok(serde_json::to_value(&output)?)
    }

    /// One audit row for a device tool call, written before the call it describes (rule 6): its
    /// own action, the caller's actor and thread, an already-checked device id, and a fixed
    /// detail. Never the answer, an image or a path.
    fn audit_device_tool(
        &self,
        binding: &McpBinding,
        action: &str,
        target: &str,
        detail: Value,
    ) -> Result<()> {
        self.store.append_audit(&AuditRecord {
            actor: format!("engine:{}", binding.scope.engine),
            action: action.to_owned(),
            target: target.to_owned(),
            decision: None,
            source: "engine".into(),
            thread_id: Some(binding.scope.thread_id.clone()),
            run_id: Some(binding.scope.run_id.clone()),
            detail,
        })?;
        Ok(())
    }

    /// Records a call before it happens, whichever way the gate then goes.
    pub(crate) fn note_tool(&self, binding: &McpBinding, name: &str) -> Result<()> {
        self.store.append_audit(&AuditRecord {
            actor: format!("engine:{}", binding.scope.engine),
            action: "tool.call".into(),
            target: name.to_owned(),
            decision: None,
            source: "engine".into(),
            thread_id: Some(binding.scope.thread_id.clone()),
            run_id: Some(binding.scope.run_id.clone()),
            detail: Value::Null,
        })?;
        Ok(())
    }
}
