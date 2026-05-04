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

impl AliasBehavior {
    pub fn as_str(&self) -> &'static str {
        match self {
            AliasBehavior::Default => "DEFAULT",
            AliasBehavior::Asr => "ASR",
        }
    }
}

impl AliasStorage {
    pub fn as_str(&self) -> &'static str {
        match self {
            AliasStorage::Loaded => "LOADED",
            AliasStorage::Streamed => "STREAMED",
            AliasStorage::Primed => "PRIMED",
        }
    }
}

impl AliasBus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AliasBus::BusFx => "BUS_FX",
            AliasBus::BusVoice => "BUS_VOICE",
            AliasBus::BusPfutz => "BUS_PFUTZ",
            AliasBus::BusHdrfx => "BUS_HDRFX",
            AliasBus::BusUi => "BUS_UI",
            AliasBus::BusMusic => "BUS_MUSIC",
            AliasBus::BusMovie => "BUS_MOVIE",
            AliasBus::BusReference => "BUS_REFERENCE",
            AliasBus::BusLicense => "BUS_LICENSE",
            AliasBus::BusNearverbSend => "BUS_NEARVERB_SEND",
            AliasBus::BusNearverbWork => "BUS_NEARVERB_WORK",
            AliasBus::BusNearverbReturn => "BUS_NEARVERB_RETURN",
            AliasBus::BusFarverbSend => "BUS_FARVERB_SEND",
            AliasBus::BusFarverbWork => "BUS_FARVERB_WORK",
            AliasBus::BusFarverbReturn => "BUS_FARVERB_RETURN",
            AliasBus::BusEarlyverbSend => "BUS_EARLYVERB_SEND",
            AliasBus::BusEarlyverbWork => "BUS_EARLYVERB_WORK",
            AliasBus::BusEarlyverbReturn => "BUS_EARLYVERB_RETURN",
        }
    }
}

impl AliasLimitType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AliasLimitType::None => "NONE",
            AliasLimitType::Oldest => "OLDEST",
            AliasLimitType::Reject => "REJECT",
            AliasLimitType::Priority => "PRIORITY",
        }
    }
}

impl AliasLooping {
    pub fn as_str(&self) -> &'static str {
        match self {
            AliasLooping::Looping => "LOOPING",
            AliasLooping::Nonlooping => "NONLOOPING",
        }
    }
}

impl AliasFluxType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AliasFluxType::None => "NONE",
            AliasFluxType::LeftPlayer => "LEFT_PLAYER",
            AliasFluxType::CenterPlayer => "CENTER_PLAYER",
            AliasFluxType::RightPlayer => "RIGHT_PLAYER",
            AliasFluxType::RandomPlayer => "RANDOM_PLAYER",
            AliasFluxType::LeftShot => "LEFT_SHOT",
            AliasFluxType::CenterShot => "CENTER_SHOT",
            AliasFluxType::RightShot => "RIGHT_SHOT",
            AliasFluxType::RandomDirection => "RANDOM_DIRECTION",
        }
    }
}
