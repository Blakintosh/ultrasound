use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AliasBehavior {
    Default,
    Asr,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AliasStorage {
    Loaded,
    Streamed,
    Primed,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AliasBus {
    BusFx,
    BusVoice,
    BusPfutz,
    BusHdrfx,
    BusUi,
    BusMusic,
    BusMovie,
    BusReference,
    BusLicense,
    BusNearverbSend,
    BusNearverbWork,
    BusNearverbReturn,
    BusFarverbSend,
    BusFarverbWork,
    BusFarverbReturn,
    BusEarlyverbSend,
    BusEarlyverbWork,
    BusEarlyverbReturn,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AliasLimitType {
    None,
    Oldest,
    Reject,
    Priority,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AliasLooping {
    Looping,
    Nonlooping,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AliasFluxType {
    None,
    LeftPlayer,
    CenterPlayer,
    RightPlayer,
    RandomPlayer,
    LeftShot,
    CenterShot,
    RightShot,
    RandomDirection,
}
