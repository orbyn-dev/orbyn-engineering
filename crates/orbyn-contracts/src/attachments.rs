//! Photo conversion (docs/attachments-transcripts.md, "Photo conversion"): what
//! `POST /api/threads/{id}/attachments/convert` answers. The stored photo is the one the engine
//! reads; the photo the person picked is only a label and a size in the answer.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire_enum! {
/// What the service did to the picked photo.
pub enum PhotoAction {
    /// HEIC or HEIF, decoded and written as JPEG.
    Converted,
    /// PNG, JPEG or WebP, made smaller until it fit the image limit.
    Shrunk,
    /// Already within the limit; written again so its metadata does not travel.
    None,
}
}

wire! {
/// The answer. `id` goes in the run's `attachment_ids`; `name` is the stored name with the new
/// ending; `original_*` describe the file the person picked.
pub struct ConvertedPhoto {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub original_name: String,
    pub original_size: u64,
    pub action: PhotoAction,
}
}
