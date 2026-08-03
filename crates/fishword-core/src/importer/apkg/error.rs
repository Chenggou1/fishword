//! apkg 解析专属错误类型。独立于 [`crate::error::Error`]，避免给核心错误枚举
//! 塞入只服务于单一子系统的变体；CLI 层把每个变体映射成一个稳定的协议错误码。

use std::fmt;

/// 解析 `.apkg` 时可能出现的错误。每个变体对应 CLI 的一个 `apkg_*` 错误码。
#[derive(Debug)]
pub enum ApkgError {
    /// zip 容器本身读不出来或损坏。
    InvalidZip(String),
    /// zip 里找不到 `collection.anki2` / `.anki21` / `.anki21b`。
    MissingCollection,
    /// `collection.anki21b` 的 zstd 解压失败。
    ZstdDecode(String),
    /// SQLite 数据库打开或查询失败（文件不是合法 anki2 schema）。
    InvalidDatabase(String),
    /// 数据库里没有 notetype / note，是空集合。
    EmptyCollection,
    /// note 存在但映射后没有任何可导入的卡片（比如全部字段为空）。
    NoCards,
    /// 所有 note 都识别不出单词字段（term）——这是最常见也最需要用户介入的失败：
    /// 没有单词就没法学。错误消息会引导用户用 `--inspect` + `--map term=`。
    NoTerm,
    /// `--map` 语法不合法。
    InvalidMap(String),
    /// `--map` 引用的字段索引或字段名在 notetype 里不存在。
    FieldNotFound(String),
}

impl fmt::Display for ApkgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidZip(msg) => write!(f, "invalid apkg zip: {msg}"),
            Self::MissingCollection => write!(
                f,
                "apkg missing collection.anki2/.anki21/.anki21b — not a valid Anki package"
            ),
            Self::ZstdDecode(msg) => write!(f, "zstd decode of collection.anki21b failed: {msg}"),
            Self::InvalidDatabase(msg) => write!(f, "invalid Anki database: {msg}"),
            Self::EmptyCollection => write!(f, "Anki collection has no notes/notetypes"),
            Self::NoCards => write!(f, "no importable cards after mapping Anki notes"),
            Self::NoTerm => write!(
                f,
                "could not identify the word/term field in any note. \
                 Run `fishword import apkg <file> --inspect` to see the detected field mapping, \
                 then pass `--map term=<field-index-or-name>` to specify the word field"
            ),
            Self::InvalidMap(msg) => write!(f, "invalid --map: {msg}"),
            Self::FieldNotFound(msg) => write!(f, "--map references unknown field: {msg}"),
        }
    }
}

impl std::error::Error for ApkgError {}

impl From<rusqlite::Error> for ApkgError {
    fn from(error: rusqlite::Error) -> Self {
        Self::InvalidDatabase(error.to_string())
    }
}

impl From<serde_json::Error> for ApkgError {
    fn from(error: serde_json::Error) -> Self {
        Self::InvalidDatabase(format!("malformed JSON in col table: {error}"))
    }
}

impl From<std::io::Error> for ApkgError {
    fn from(error: std::io::Error) -> Self {
        Self::InvalidZip(error.to_string())
    }
}

pub type ApkgResult<T> = std::result::Result<T, ApkgError>;
