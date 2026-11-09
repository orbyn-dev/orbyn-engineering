//! Derive wrappers every wire type goes through.

macro_rules! wire {
    ($item:item) => {
        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
        #[ts(export, export_to = "../../../web/src/generated/")]
        $item
    };
}

macro_rules! wire_enum {
    ($item:item) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
        #[serde(rename_all = "snake_case")]
        #[ts(export, export_to = "../../../web/src/generated/")]
        $item
    };
}
