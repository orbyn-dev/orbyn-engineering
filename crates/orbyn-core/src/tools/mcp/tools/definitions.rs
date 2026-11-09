//! The tool table: each Orbyn tool's name, description and input schema, as an agent reads them.

pub(super) struct Tool {
    pub(super) name: &'static str,
    pub(super) description: &'static str,
    /// Has an effect, so it goes through the gate.
    pub(super) effect: bool,
    /// `(name, type)` of each argument.
    pub(super) args: &'static [(&'static str, &'static str)],
    pub(super) required: &'static [&'static str],
}

pub(super) const fn read(
    name: &'static str,
    description: &'static str,
    args: &'static [(&'static str, &'static str)],
    required: &'static [&'static str],
) -> Tool {
    Tool {
        name,
        description,
        effect: false,
        args,
        required,
    }
}

pub(super) const fn act(
    name: &'static str,
    description: &'static str,
    args: &'static [(&'static str, &'static str)],
    required: &'static [&'static str],
) -> Tool {
    Tool {
        name,
        description,
        effect: true,
        args,
        required,
    }
}

pub(super) const TOOLS: &[Tool] = &[
    act(
        "orbyn_project_create",
        "Add an existing absolute folder in place as an untrusted project, through the tool approval policy. Does not create, clone, move or trust a folder.",
        &[("path", "string")],
        &["path"],
    ),
    act(
        "orbyn_project_delete",
        "Remove an approved project's registration and idle thread metadata, keeping every file. Refuses own, library, scratch, busy or cross-project thread families.",
        &[("project_id", "string")],
        &["project_id"],
    ),
    read(
        "orbyn_project_list",
        "Projects visible to this caller: only its own project.",
        &[],
        &[],
    ),
    read(
        "orbyn_environment_read",
        "This service's version, platform and this thread's folder.",
        &[],
        &[],
    ),
    read(
        "orbyn_project_read",
        "This thread's project, with its project file.",
        &[],
        &[],
    ),
    read(
        "orbyn_worktree_list",
        "Worktrees of this project.",
        &[],
        &[],
    ),
    read("orbyn_thread_list", "Threads in this project.", &[], &[]),
    read(
        "orbyn_thread_read",
        "A thread and its recent text.",
        &[("thread_id", "string")],
        &["thread_id"],
    ),
    read(
        "orbyn_thread_search",
        "Threads in this project matching every word.",
        &[("query", "string")],
        &["query"],
    ),
    read(
        "orbyn_thread_wait",
        "Wait for a thread's run to end, up to forty seconds; ended is false at the deadline.",
        &[("thread_id", "string"), ("seconds", "number")],
        &["thread_id"],
    ),
    read(
        "orbyn_queue_list",
        "A thread's queued messages.",
        &[("thread_id", "string")],
        &["thread_id"],
    ),
    read(
        "orbyn_pending_requests",
        "Approvals and questions open in a thread.",
        &[("thread_id", "string")],
        &["thread_id"],
    ),
    read(
        "orbyn_pull_list",
        "Pull requests linked to this thread, and those it watches.",
        &[],
        &[],
    ),
    read(
        "orbyn_pull_guide_read",
        "This project's review guide for a pull request; guide is null when none was written.",
        &[("number", "number")],
        &["number"],
    ),
    read(
        "orbyn_schedule_list",
        "Scheduled tasks in this project.",
        &[],
        &[],
    ),
    read(
        "orbyn_task_status",
        "A delegated thread's state.",
        &[("thread_id", "string")],
        &["thread_id"],
    ),
    read(
        "orbyn_worktree_status",
        "This checkout's branch, whether it is a worktree, and its uncommitted files.",
        &[],
        &[],
    ),
    read(
        "orbyn_orchestrator_capabilities",
        "What this thread can start: engines, and whether it may delegate.",
        &[],
        &[],
    ),
    read(
        "orbyn_preview_list",
        "This thread's preview tabs.",
        &[],
        &[],
    ),
    read(
        "orbyn_preview_status",
        "One preview tab: where it is, its size, view and state.",
        &[("tab_id", "string")],
        &["tab_id"],
    ),
    read(
        "orbyn_preview_snapshot",
        "The visible controls and a sample of the text of a preview tab's page, \
         capped in size.",
        &[("tab_id", "string"), ("max_elements", "number")],
        &["tab_id"],
    ),
    read(
        "orbyn_preview_wait_for",
        "Wait for a selector or text to show up in a preview tab, up to twenty seconds.",
        &[
            ("tab_id", "string"),
            ("selector", "string"),
            ("text", "string"),
            ("timeout_ms", "number"),
        ],
        &["tab_id"],
    ),
    read(
        "orbyn_track_files",
        "Before writing files no tool reports, such as from a shell script, call this with \
         the checkout paths; call it again with done true afterwards.",
        &[("paths", "array"), ("done", "boolean")],
        &["paths"],
    ),
    act(
        "orbyn_thread_launch",
        "Start a new thread in this project with a first message.",
        &[
            ("prompt", "string"),
            ("engine", "string"),
            ("title", "string"),
        ],
        &["prompt"],
    ),
    act(
        "orbyn_threads_create",
        "Start several threads in this project, each with a first message. One failure \
         does not stop the rest.",
        &[("threads", "array")],
        &["threads"],
    ),
    act(
        "orbyn_worktree_handoff",
        "Continue in a new worktree: a new thread there, with a first message.",
        &[
            ("purpose", "string"),
            ("prompt", "string"),
            ("base_branch", "string"),
            ("title", "string"),
        ],
        &["purpose", "prompt"],
    ),
    act(
        "orbyn_environment_preferences_update",
        "Change this project's preferences. null goes back to the default.",
        &[("values", "object")],
        &["values"],
    ),
    act(
        "orbyn_project_update",
        "Rename this project.",
        &[("name", "string")],
        &["name"],
    ),
    act(
        "orbyn_thread_send",
        "Queue a message on a thread, with attachments from orbyn_attachment_add.",
        &[
            ("thread_id", "string"),
            ("prompt", "string"),
            ("attachment_ids", "array"),
        ],
        &["thread_id", "prompt"],
    ),
    act(
        "orbyn_attachment_add",
        "Attach a file from this checkout to a thread, for orbyn_thread_send.",
        &[("thread_id", "string"), ("path", "string")],
        &["thread_id", "path"],
    ),
    act(
        "orbyn_attachment_discard",
        "Remove an attachment that was not sent.",
        &[("thread_id", "string"), ("attachment_id", "string")],
        &["thread_id", "attachment_id"],
    ),
    act(
        "orbyn_thread_organize",
        "Pin, settle, snooze, archive or mark a thread unread.",
        &[
            ("thread_id", "string"),
            ("pinned", "boolean"),
            ("settled", "boolean"),
            ("archived", "boolean"),
            ("unread", "boolean"),
            ("snoozed_until", "string"),
        ],
        &["thread_id"],
    ),
    act(
        "orbyn_thread_configure",
        "Set a thread's model, effort, permission mode or plan mode.",
        &[
            ("thread_id", "string"),
            ("model", "string"),
            ("effort", "string"),
            ("permission_mode", "string"),
            ("plan_mode", "boolean"),
        ],
        &["thread_id"],
    ),
    act(
        "orbyn_preview_open",
        "Open a web page in a preview tab for this thread.",
        &[("url", "string"), ("width", "number"), ("height", "number")],
        &["url"],
    ),
    act(
        "orbyn_preview_navigate",
        "Go to a page, or back or forward, in one of this thread's preview tabs.",
        &[("tab_id", "string"), ("url", "string"), ("back", "boolean")],
        &["tab_id"],
    ),
    act(
        "orbyn_preview_resize",
        "Set a preview tab's size.",
        &[
            ("tab_id", "string"),
            ("width", "number"),
            ("height", "number"),
        ],
        &["tab_id", "width", "height"],
    ),
    act(
        "orbyn_preview_set_appearance",
        "Set a preview tab's color scheme: system, light or dark.",
        &[("tab_id", "string"), ("appearance", "string")],
        &["tab_id", "appearance"],
    ),
    act(
        "orbyn_preview_act",
        "Click, type into or scroll a preview tab's page.",
        &[
            ("tab_id", "string"),
            ("action", "string"),
            ("selector", "string"),
            ("text", "string"),
            ("dy", "number"),
        ],
        &["tab_id", "action"],
    ),
    act(
        "orbyn_preview_evaluate",
        "Run an expression in a preview tab's page and return its value, or its size \
         when that is over 20000 bytes.",
        &[("tab_id", "string"), ("expression", "string")],
        &["tab_id", "expression"],
    ),
    act(
        "orbyn_preview_dialog",
        "Answer the alert, confirm or prompt a page opened. `handled` is false if none was.",
        &[
            ("tab_id", "string"),
            ("accept", "boolean"),
            ("text", "string"),
        ],
        &["tab_id", "accept"],
    ),
    act(
        "orbyn_preview_close",
        "Close one of this thread's preview tabs.",
        &[("tab_id", "string")],
        &["tab_id"],
    ),
    act(
        "orbyn_preview_recording_start",
        "Start recording a preview tab's frames.",
        &[("tab_id", "string")],
        &["tab_id"],
    ),
    act(
        "orbyn_preview_recording_stop",
        "Stop recording, and return where the frames are.",
        &[("tab_id", "string")],
        &["tab_id"],
    ),
    act(
        "orbyn_html_render",
        "Publish an HTML page in this thread's timeline, as a card with a frame. The page \
         runs sandboxed: no network, no API and no storage of its own. `html` is the whole \
         page. `height` is the frame's height in CSS pixels; nothing measures the page yet, \
         so a page that needs more room needs a taller height.",
        &[
            ("html", "string"),
            ("title", "string"),
            ("height", "number"),
        ],
        &["html"],
    ),
    act(
        "orbyn_thread_update",
        "Rename, pin or settle a thread.",
        &[
            ("thread_id", "string"),
            ("title", "string"),
            ("pinned", "boolean"),
            ("settled", "boolean"),
        ],
        &["thread_id"],
    ),
    act(
        "orbyn_thread_interrupt",
        "Stop a thread's running turn.",
        &[("thread_id", "string")],
        &["thread_id"],
    ),
    act(
        "orbyn_pull_link",
        "Link a pull request to this thread.",
        &[("number", "number")],
        &["number"],
    ),
    act(
        "orbyn_pull_unlink",
        "Unlink a pull request from this thread.",
        &[("number", "number")],
        &["number"],
    ),
    act(
        "orbyn_pull_watch",
        "Wake this thread when a pull request changes.",
        &[("number", "number")],
        &["number"],
    ),
    act(
        "orbyn_pull_unwatch",
        "Stop watching a pull request.",
        &[("number", "number")],
        &["number"],
    ),
    act(
        "orbyn_pull_guide_write",
        "Write this project's review guide for a pull request, replacing any earlier one: \
         up to 30 stops in reading order, each {title, summary, files: [{path, note}], \
         questions: [text]}. head_sha names the commit it describes. Fails while review \
         guides are off for this project.",
        &[
            ("number", "number"),
            ("head_sha", "string"),
            ("stops", "array"),
        ],
        &["number", "stops"],
    ),
    act(
        "orbyn_delegate_task",
        "Hand work to a child thread, which reports back when it ends.",
        &[
            ("prompt", "string"),
            ("title", "string"),
            ("engine", "string"),
        ],
        &["prompt"],
    ),
    act(
        "orbyn_task_cancel",
        "Stop a delegated thread's run.",
        &[("thread_id", "string")],
        &["thread_id"],
    ),
    act(
        "orbyn_workflow_stop",
        "Request a stop for this caller's delegated thread and its running delegated children. An idle thread is unchanged.",
        &[("thread_id", "string")],
        &["thread_id"],
    ),
    act(
        "orbyn_schedule_task",
        "Run a prompt in this project every so many minutes.",
        &[
            ("name", "string"),
            ("prompt", "string"),
            ("every_minutes", "number"),
        ],
        &["name", "prompt", "every_minutes"],
    ),
    act(
        "orbyn_schedule_delete",
        "Delete a scheduled task in this project.",
        &[("task_id", "string")],
        &["task_id"],
    ),
    act(
        "orbyn_schedule_run",
        "Run a scheduled task in this project now.",
        &[("task_id", "string")],
        &["task_id"],
    ),
];
