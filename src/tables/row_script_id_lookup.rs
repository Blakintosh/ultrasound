use serde::Deserialize;

use crate::tables::{Row, empty_as_none};

/// One row of a `<sound>/scriptid/<source>.csv` lookup file. Maps a
/// script-id string to the alias the engine should play when scripts
/// reference that id. The CSVs are paired by filename with alias source
/// files (so `aliases/foo.csv` ↔ `scriptid/foo.csv`); when a sibling
/// exists, every row is appended to the zone's accumulated lookup
/// table and emitted into `<zone>.<lang>.scriptid.sz`.
#[derive(Debug, Deserialize)]
pub struct RowScriptIdLookup {
    #[serde(rename = "ScriptId", default)]
    pub script_id: String,

    #[serde(rename = "AliasName", default)]
    pub alias_name: String,

    #[serde(rename = "RowSourceFileName", default)]
    pub row_source_file_name: String,

    #[serde(rename = "RowSourceShortName", default)]
    pub row_source_short_name: String,

    #[serde(
        rename = "RowSourceLineNumber",
        default,
        deserialize_with = "empty_as_none"
    )]
    pub row_source_line_number: Option<i32>,
}

impl Row for RowScriptIdLookup {
    fn get_row_name(&self) -> &str {
        &self.script_id
    }
}
