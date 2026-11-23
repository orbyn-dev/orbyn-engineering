//! Projects: the list, clones, trust, icons and images, and the folder browser.

use std::path::{Path as FsPath, PathBuf};

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, patch, post};
use orbyn_contracts::{
    AddProject, CloneProject, FsEntry, FsListing, NewProject, PublishProject, RenameProject,
};
use orbyn_core::CoreError;
use serde::Deserialize;

use crate::http::{ApiJson, ApiResult, AppState, LOCAL_USER};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/projects", get(list_projects))
        .route("/api/projects/clone", post(clone_project))
        .route("/api/projects/create", post(create_project))
        .route("/api/clones", get(list_clones).post(start_clone))
        .route("/api/clones/{id}/cancel", post(cancel_clone))
        .route("/api/clones/{id}/retry", post(retry_clone))
        .route("/api/projects/add", post(add_project))
        .route(
            "/api/projects/{id}",
            patch(rename_project).delete(remove_project),
        )
        .route("/api/projects/{id}/trust", post(trust_project))
        .route("/api/projects/{id}/git/init", post(init_project_git))
        .route("/api/fs/browse", get(browse))
        .route("/api/projects/{id}/publish", post(publish_project))
        .route("/api/projects/{id}/icon", get(project_icon))
        .route(
            "/api/projects/{id}/icon/resource",
            get(project_icon_resource),
        )
        .route(
            "/api/settings/projects/image-search",
            post(search_project_images),
        )
        .route("/api/settings/projects/image", post(change_project_image))
        .route(
            "/api/settings/projects/image/choose",
            post(choose_project_image),
        )
        .route("/api/settings/project-scopes", post(project_scopes))
        .route("/api/settings/projects/icon", post(change_project_icon))
        .route(
            "/api/settings/projects/rename",
            post(rename_selected_projects),
        )
        .route("/api/projects/{id}/pull", post(pull_project))
}

async fn list_projects(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.projects()?))
}

async fn project_icon(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.project_icon(&id).await?))
}

async fn project_icon_resource(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.project_icon_resource(&id).await?))
}

async fn search_project_images(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<orbyn_contracts::SearchProjectImages>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.search_project_images(&body).await?))
}

async fn change_project_image(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<orbyn_contracts::ChangeProjectImage>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.change_project_image(&body).await?))
}

/// Opens the desktop app's image dialog and applies the chosen image. `204` means the
/// person cancelled. The path never comes from the caller, only from the shell.
async fn choose_project_image(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<orbyn_contracts::ChooseProjectImage>,
) -> ApiResult<axum::response::Response> {
    // A dialog opens on the computer the app runs on; a remote viewer can't answer it.
    if state.remote {
        return Err(
            CoreError::Invalid("Choose an image on the computer the app runs on.".into()).into(),
        );
    }
    let Some(path) = state.core.shell.choose_image().await? else {
        return Ok(StatusCode::NO_CONTENT.into_response());
    };
    Ok(Json(state.core.import_project_image(&body, &path).await?).into_response())
}

async fn project_scopes(
    State(state): State<AppState>,
    ApiJson(query): ApiJson<orbyn_contracts::ProjectScopesQuery>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.project_scope_catalog(&query).await?))
}

async fn change_project_icon(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<orbyn_contracts::ChangeProjectIcon>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.change_project_icon(&body).await?))
}

async fn rename_selected_projects(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<orbyn_contracts::RenameSelectedProjects>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.rename_selected_projects(&body).await?))
}

async fn clone_project(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<CloneProject>,
) -> ApiResult<impl IntoResponse> {
    let project = state
        .core
        .clone_project(&body.url, body.name.as_deref(), body.path.as_deref())
        .await?;
    Ok((StatusCode::CREATED, Json(project)))
}

async fn list_clones(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.core.clone_jobs())
}

async fn start_clone(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<CloneProject>,
) -> ApiResult<impl IntoResponse> {
    let job = state
        .core
        .start_clone(&body.url, body.name.as_deref(), body.path.as_deref())?;
    Ok((StatusCode::ACCEPTED, Json(job)))
}

async fn cancel_clone(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.core.cancel_clone(&id)?;
    Ok(StatusCode::ACCEPTED)
}

async fn retry_clone(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok((StatusCode::ACCEPTED, Json(state.core.retry_clone(&id)?)))
}

async fn create_project(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<NewProject>,
) -> ApiResult<impl IntoResponse> {
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .core
                .new_project(&body.name, body.readme, body.path.as_deref())
                .await?,
        ),
    ))
}

async fn add_project(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<AddProject>,
) -> ApiResult<impl IntoResponse> {
    Ok((
        StatusCode::CREATED,
        Json(state.core.add_project(&body.path).await?),
    ))
}

#[derive(Deserialize)]
struct RemoveQuery {
    confirm: Option<bool>,
    delete_files: Option<bool>,
}

async fn remove_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<RemoveQuery>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state
            .core
            .remove_project(
                &id,
                query.confirm.unwrap_or(false),
                query.delete_files.unwrap_or(false),
            )
            .await?,
    ))
}

async fn trust_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.store.trust_project(&id, LOCAL_USER)?))
}

/// Starts Git in a registered project folder that was added without it. The handler
/// returns the project row, as trust does; the core refuses the cases it cannot do
/// safely.
async fn init_project_git(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.init_project_git(&id).await?))
}

#[derive(Deserialize)]
struct BrowseQuery {
    path: Option<String>,
}

/// Windows attributes that keep a folder out of the picker: `FILE_ATTRIBUTE_HIDDEN` and
/// `FILE_ATTRIBUTE_SYSTEM` are the ones Windows puts on its own folders, such as `AppData`
/// in a profile and `$Recycle.Bin` on a drive, and `FILE_ATTRIBUTE_REPARSE_POINT` is a
/// junction or a link, which can point at a folder above it and loop when opened.
#[cfg(any(windows, test))]
const HIDING_ATTRIBUTES: u32 = 0x2 | 0x4 | 0x400;

/// Whether the attributes Windows reports take a folder out of the picker. They are handed
/// in rather than read, so the rule can be tested where no hidden, system or junction folder
/// can be made.
#[cfg(any(windows, test))]
pub(super) fn attributes_hide_folder(attributes: u32) -> bool {
    attributes & HIDING_ATTRIBUTES != 0
}

/// Whether the picker lists one folder: a directory, and on Windows not one that Windows
/// marks hidden or system, and not a junction or a link.
#[cfg(windows)]
fn is_listed_folder(entry: &std::fs::DirEntry) -> bool {
    use std::os::windows::fs::MetadataExt;

    // This is the entry's own metadata, not the target of a reparse point.
    let Ok(metadata) = entry.metadata() else {
        return false;
    };
    metadata.is_dir() && !attributes_hide_folder(metadata.file_attributes())
}

#[cfg(not(windows))]
fn is_listed_folder(entry: &std::fs::DirEntry) -> bool {
    entry.path().is_dir()
}

/// The folder picker under Add a folder: a parent link, and the folders a person may choose
/// from. A `.`-named folder is left out on every platform; on Windows so is a folder Windows
/// hides or marks system, and a junction or a link.
async fn browse(Query(query): Query<BrowseQuery>) -> ApiResult<Json<FsListing>> {
    let requested = query
        .path
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(orbyn_core::util::home_dir)
        .ok_or_else(|| CoreError::Invalid("Choose a folder.".into()))?;
    if !requested.is_absolute() {
        return Err(CoreError::Invalid("Choose an absolute folder path.".into()).into());
    }
    let path = orbyn_core::util::canonicalize(&requested)
        .map_err(|_| CoreError::Invalid(format!("{} cannot be opened.", requested.display())))?;
    let reader = std::fs::read_dir(&path)
        .map_err(|_| CoreError::Invalid(format!("{} cannot be read.", path.display())))?;
    // A folder row is where the picker goes next. A drive root has no file name,
    // so it keeps its whole path as the name.
    let row = |folder: &FsPath| FsEntry {
        name: folder
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| folder.to_string_lossy().into_owned()),
        is_git: orbyn_core::source_control::git::is_git_repo(folder),
        path: folder.to_string_lossy().into_owned(),
    };
    let mut entries: Vec<FsEntry> = reader
        .filter_map(Result::ok)
        .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
        .filter(is_listed_folder)
        .map(|entry| row(&entry.path()))
        .collect();
    entries.sort_by_key(|entry| entry.name.to_lowercase());
    if path.parent().is_none() {
        // Nothing is above a root, so a drive root also offers the mounted
        // drives: that is how a person leaves the drive they started on. A
        // drive that cannot be read, an empty card reader, is no way in.
        let drives = orbyn_core::util::drive_roots()
            .into_iter()
            .filter(|drive| drive != &path && std::fs::read_dir(drive).is_ok())
            .map(|drive| row(&drive));
        entries.splice(0..0, drives);
    }
    Ok(Json(FsListing {
        parent: path
            .parent()
            .map(FsPath::to_string_lossy)
            .map(std::borrow::Cow::into_owned),
        path: path.to_string_lossy().into_owned(),
        entries,
    }))
}

async fn publish_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<PublishProject>,
) -> ApiResult<StatusCode> {
    state.core.publish_project(&id, &body, LOCAL_USER).await?;
    Ok(StatusCode::ACCEPTED)
}

async fn pull_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.core.pull_project(&id, LOCAL_USER).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn rename_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<RenameProject>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.rename_project(&id, &body.name)?))
}
