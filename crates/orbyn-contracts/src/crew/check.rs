//! One verifiable check. It sits inside an answer key, a contract fact or a claim and
//! has no id of its own. Only the core runs it.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::common::{CrewError, Verdict, text, within};
use super::text::{CrewId, CrewLoopbackUrl, CrewPath, CrewPathPattern, CrewShortText, CrewToken};

wire_enum! {
pub enum CheckKind {
    Command,
    Test,
    Lint,
    Typecheck,
    CustomScript,
    FileScope,
    DiffSize,
    Schema,
    HttpProbe,
    Preview,
    VisualCompare,
    RepoRequired,
    Manual,
    ScreenshotOnly,
    AccessibilityAudit,
    PerformanceBudget,
    SecretScan,
    LicenceScan,
}
}

wire! {
#[serde(deny_unknown_fields)]
pub struct CrewCheck {
    pub id: CrewId,
    pub kind: CheckKind,
    /// The shape depends on `kind`; [`CrewCheck::typed_params`] reads it.
    pub params: Value,
    pub proves: CrewShortText,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub does_not_prove: Option<CrewShortText>,
}
}

/// The exact command. `expect_exit` defaults to 0.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunParams {
    pub run: String,
    #[serde(default)]
    pub expect_exit: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileScopeParams {
    pub only: Vec<CrewPathPattern>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiffSizeParams {
    #[serde(default)]
    pub max_files: Option<u64>,
    #[serde(default)]
    pub max_lines: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaParams {
    pub schema: CrewPath,
    pub instance: CrewPath,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpProbeParams {
    pub url: CrewLoopbackUrl,
    pub expect_status: u16,
}

/// A page that must load; a preview also requires no console errors.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageParams {
    pub url: CrewLoopbackUrl,
    #[serde(default)]
    pub screenshot: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualParams {
    pub frame: CrewShortText,
    pub tolerance_permille: u32,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetParams {
    pub metric: CrewToken,
    pub max: u64,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedParams {
    #[serde(default)]
    pub name: Option<CrewShortText>,
    #[serde(default)]
    pub note: Option<CrewShortText>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmptyParams {}

/// A check's `params`, read by its `kind`.
#[derive(Clone, Debug, PartialEq)]
pub enum CheckParams {
    Run(RunParams),
    FileScope(FileScopeParams),
    DiffSize(DiffSizeParams),
    Schema(SchemaParams),
    HttpProbe(HttpProbeParams),
    Page(PageParams),
    Visual(VisualParams),
    Budget(BudgetParams),
    Named(NamedParams),
    Empty(EmptyParams),
}

fn read<T: serde::de::DeserializeOwned>(kind: CheckKind, params: &Value) -> Result<T, CrewError> {
    if !params.is_object() {
        return Err(CrewError::invalid(format!(
            "params of a {kind:?} check must be an object"
        )));
    }
    serde_json::from_value(params.clone())
        .map_err(|e| CrewError::invalid(format!("params of a {kind:?} check: {e}")))
}

impl CrewCheck {
    /// Reads `params` as the shape its `kind` requires.
    pub fn typed_params(&self) -> Result<CheckParams, CrewError> {
        use CheckKind::*;
        let (kind, params) = (self.kind, &self.params);
        Ok(match kind {
            Command | Test | Lint | Typecheck | CustomScript => {
                CheckParams::Run(read(kind, params)?)
            }
            FileScope => CheckParams::FileScope(read(kind, params)?),
            DiffSize => CheckParams::DiffSize(read(kind, params)?),
            Schema => CheckParams::Schema(read(kind, params)?),
            HttpProbe => CheckParams::HttpProbe(read(kind, params)?),
            Preview | ScreenshotOnly | AccessibilityAudit => CheckParams::Page(read(kind, params)?),
            VisualCompare => CheckParams::Visual(read(kind, params)?),
            PerformanceBudget => CheckParams::Budget(read(kind, params)?),
            RepoRequired | Manual => CheckParams::Named(read(kind, params)?),
            SecretScan | LicenceScan => CheckParams::Empty(read(kind, params)?),
        })
    }

    /// The command a run-kind check executes, which is part of the answer key.
    pub fn command(&self) -> Option<String> {
        match self.typed_params() {
            Ok(CheckParams::Run(run)) => Some(run.run),
            _ => None,
        }
    }

    pub fn validate(&self) -> Verdict {
        match self.typed_params()? {
            CheckParams::Run(p) => text("run", &p.run, 1000)?,
            CheckParams::FileScope(p) => super::common::count("only", p.only.len(), 1, 256)?,
            CheckParams::DiffSize(p) if p.max_files.is_none() && p.max_lines.is_none() => {
                return Err(CrewError::invalid(
                    "a diff_size check sets max_files or max_lines",
                ));
            }
            CheckParams::HttpProbe(p) => within("expect_status", p.expect_status.into(), 100, 599)?,
            CheckParams::Visual(p) => {
                within("tolerance_permille", p.tolerance_permille.into(), 0, 1000)?
            }
            _ => {}
        }
        Ok(())
    }
}
