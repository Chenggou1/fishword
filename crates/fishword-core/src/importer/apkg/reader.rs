//! 读取 `.apkg` 容器与内嵌的 anki2 SQLite 数据库，产出统一的 notetype / note / deck
//! 视图。本模块只负责「把字节变成结构化记录」，不做字段→角色的语义判断（那是
//! [`crate::importer::apkg::mapping`] 的工作）。
//!
//! 三种 apkg 内部格式的检测见 [`ApkgReader::from_bytes`]；notetype 的两种存储位置
//! （schema 11 的 `col.models` JSON vs schema 15+ 的 `notetypes`/`fields` 表）见
//! [`ApkgReader::read_notetypes`]。

use std::collections::HashMap;
use std::io::Read;

use rusqlite::Connection;

use super::error::{ApkgError, ApkgResult};

/// Anki notetype 的一个字段定义。`ord` 是字段在 note 的 `flds` 中的位置。
#[derive(Debug, Clone)]
pub(crate) struct NotetypeField {
    pub ord: usize,
    pub name: String,
}

/// 一个 Anki notetype：一组有序命名字段。note 按 `mid` 引用它。notetype 自身的 id
/// 由外层 `HashMap<i64, Notetype>` 的 key 承载，这里不重复存。
#[derive(Debug, Clone)]
pub(crate) struct Notetype {
    pub name: String,
    pub fields: Vec<NotetypeField>,
}

/// 一条 Anki note 的原始字段值（按字段 `ord` 排列）+ 标签 + 来源 deck id。
#[derive(Debug, Clone)]
pub(crate) struct AnkiNote {
    pub notetype_id: i64,
    pub fields: Vec<String>,
    pub tags: Vec<String>,
    /// note 首张 card 所属的 Anki deck id；多 deck 合并导入时用于把 deck 名加进 tags。
    pub deck_id: Option<i64>,
}

/// 持有一个临时解压出来的 anki2 数据库连接。`_dir` 必须比 `conn` 活得更久，
/// 因此显式保存在结构体里，随 reader 一起 drop 清理。
pub(crate) struct ApkgReader {
    conn: Connection,
    _dir: tempfile::TempDir,
}

impl ApkgReader {
    /// 从 apkg 字节构造 reader：检测格式、必要时 zstd 解压、写入临时文件、打开连接。
    pub(crate) fn from_bytes(bytes: &[u8]) -> ApkgResult<Self> {
        let db_bytes = extract_collection_db(bytes)?;
        let dir = tempfile::tempdir().map_err(|e| ApkgError::InvalidZip(e.to_string()))?;
        let db_path = dir.path().join("collection.anki2");
        std::fs::write(&db_path, &db_bytes)?;
        let conn = Connection::open(&db_path)?;
        // 防御：确认是 anki2 schema（有 notes 表）。
        if !has_table(&conn, "notes")? {
            return Err(ApkgError::InvalidDatabase(
                "missing 'notes' table — not an Anki collection".to_string(),
            ));
        }
        Ok(Self { conn, _dir: dir })
    }

    /// 读取全部 notetype，按 `mid` 索引。schema 11 与 15+ 走不同路径。
    pub(crate) fn read_notetypes(&self) -> ApkgResult<HashMap<i64, Notetype>> {
        if has_table(&self.conn, "notetypes")? {
            self.read_notetypes_v15()
        } else {
            self.read_notetypes_v11()
        }
    }

    /// schema 11：notetype 存在 `col.models` JSON 里。
    fn read_notetypes_v11(&self) -> ApkgResult<HashMap<i64, Notetype>> {
        let json: String = match self
            .conn
            .query_row("SELECT models FROM col LIMIT 1", [], |row| row.get(0))
        {
            Ok(value) => value,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Err(ApkgError::EmptyCollection),
            Err(e) => return Err(e.into()),
        };
        let models: HashMap<String, ModelDefV11> = serde_json::from_str(&json)?;
        let mut map = HashMap::new();
        for (id_str, model) in models {
            // notetype id 以 JSON 对象 key 为准（始终是数字字符串）。不读 model.id——
            // 有些导出把 model.id 也写成字符串，反序列化成 i64 会失败。
            let id = id_str.parse::<i64>().unwrap_or(0);
            let mut fields: Vec<NotetypeField> = model
                .flds
                .into_iter()
                .map(|f| NotetypeField {
                    ord: f.ord,
                    name: f.name,
                })
                .collect();
            fields.sort_by_key(|f| f.ord);
            map.insert(
                id,
                Notetype {
                    name: model.name,
                    fields,
                },
            );
        }
        Ok(map)
    }

    /// schema 15+：notetype 拆进 `notetypes` + `fields` 两张表。
    fn read_notetypes_v15(&self) -> ApkgResult<HashMap<i64, Notetype>> {
        let mut nt_stmt = self
            .conn
            .prepare("SELECT id, name FROM notetypes ORDER BY id")?;
        let mut nt_rows = nt_stmt.query([])?;
        let mut entries: Vec<(i64, String)> = Vec::new();
        while let Some(row) = nt_rows.next()? {
            entries.push((row.get(0)?, row.get(1)?));
        }
        drop(nt_rows);
        drop(nt_stmt);

        let mut field_stmt = self
            .conn
            .prepare("SELECT ntid, ord, name FROM fields ORDER BY ntid, ord")?;
        let mut field_rows = field_stmt.query([])?;
        let mut by_ntid: HashMap<i64, Vec<NotetypeField>> = HashMap::new();
        while let Some(row) = field_rows.next()? {
            let ntid: i64 = row.get(0)?;
            let ord: i64 = row.get(1)?;
            let name: String = row.get(2)?;
            by_ntid.entry(ntid).or_default().push(NotetypeField {
                ord: ord.max(0) as usize,
                name,
            });
        }

        let mut map = HashMap::new();
        for (id, name) in entries {
            let mut fields = by_ntid.remove(&id).unwrap_or_default();
            fields.sort_by_key(|f| f.ord);
            map.insert(id, Notetype { name, fields });
        }
        Ok(map)
    }

    /// 读取 deck 名映射（`did` → deck 名），用于给 note 打来源 tag。
    pub(crate) fn read_deck_names(&self) -> ApkgResult<HashMap<i64, String>> {
        if has_table(&self.conn, "decks")? {
            self.read_deck_names_v15()
        } else {
            self.read_deck_names_v11()
        }
    }

    fn read_deck_names_v11(&self) -> ApkgResult<HashMap<i64, String>> {
        let json: String = match self
            .conn
            .query_row("SELECT decks FROM col LIMIT 1", [], |row| row.get(0))
        {
            Ok(value) => value,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(HashMap::new()),
            Err(e) => return Err(e.into()),
        };
        let decks: HashMap<String, DeckDefV11> = serde_json::from_str(&json)?;
        Ok(decks
            .into_iter()
            .filter_map(|(id_str, def)| Some((id_str.parse::<i64>().ok()?, def.name)))
            .collect())
    }

    fn read_deck_names_v15(&self) -> ApkgResult<HashMap<i64, String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name FROM decks ORDER BY id")?;
        let mut rows = stmt.query([])?;
        let mut map = HashMap::new();
        while let Some(row) = rows.next()? {
            map.insert(row.get(0)?, row.get(1)?);
        }
        Ok(map)
    }

    /// 读取全部 note，附带首张 card 的 deck id（用于查 deck 名打 tag）。
    /// 字段值按 `\x1f`（0x1f）拆分；标签按空格拆分。
    pub(crate) fn read_notes(&self) -> ApkgResult<Vec<AnkiNote>> {
        // 子查询取每个 note 第一张 card 的 deck id；没 card 的 note did 为 NULL。
        let mut stmt = self.conn.prepare(
            "SELECT n.mid, n.flds, n.tags,
                    (SELECT c.did FROM cards c WHERE c.nid = n.id ORDER BY c.ord LIMIT 1) AS did
             FROM notes n ORDER BY n.id",
        )?;
        let mut rows = stmt.query([])?;
        let mut notes = Vec::new();
        while let Some(row) = rows.next()? {
            let mid: i64 = row.get(0)?;
            // flds 是 TEXT（字段值以 0x1f 分隔），按字符串读再拆。
            let flds: String = row.get(1)?;
            let tags_raw: String = row.get::<_, Option<String>>(2)?.unwrap_or_default();
            let did: Option<i64> = row.get(3)?;
            let fields = split_fields(&flds);
            let tags = tags_raw
                .split(' ')
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .map(str::to_string)
                .collect();
            notes.push(AnkiNote {
                notetype_id: mid,
                fields,
                tags,
                deck_id: did,
            });
        }
        Ok(notes)
    }
}

/// 从 zip 字节里抽出 collection 数据库的原始字节（anki21b 还会 zstd 解压）。
///
/// 检测看**实际的 collection 文件是否存在**，不靠 `meta` 文件——实测有些包带 `meta`
/// 但用的是 `collection.anki21`（非 zstd），靠 meta 判会误选 anki21b 导致找不到文件。
/// 优先级：anki21b（zstd）> anki21 > anki2。
fn extract_collection_db(bytes: &[u8]) -> ApkgResult<Vec<u8>> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| ApkgError::InvalidZip(e.to_string()))?;

    let (entry_name, zstd_compressed) = if archive.by_name("collection.anki21b").is_ok() {
        ("collection.anki21b", true)
    } else if archive.by_name("collection.anki21").is_ok() {
        ("collection.anki21", false)
    } else if archive.by_name("collection.anki2").is_ok() {
        ("collection.anki2", false)
    } else {
        return Err(ApkgError::MissingCollection);
    };

    let mut entry_bytes = Vec::new();
    let mut file = archive
        .by_name(entry_name)
        .map_err(|e| ApkgError::InvalidZip(format!("cannot read {entry_name}: {e}")))?;
    file.read_to_end(&mut entry_bytes)?;

    if zstd_compressed {
        zstd::decode_all(std::io::Cursor::new(entry_bytes))
            .map_err(|e| ApkgError::ZstdDecode(e.to_string()))
    } else {
        Ok(entry_bytes)
    }
}

/// `notes.flds` 按字段分隔符 `\x1f`（U+001F，单字节）拆成字段值。
fn split_fields(flds: &str) -> Vec<String> {
    flds.split('\u{1f}').map(str::to_string).collect()
}

fn has_table(conn: &Connection, name: &str) -> ApkgResult<bool> {
    let exists: i64 = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name = ?1",
        rusqlite::params![name],
        |row| row.get(0),
    )?;
    Ok(exists > 0)
}

// ---- schema 11 的 col.models / col.decks JSON 反序列化结构 ----

#[derive(serde::Deserialize)]
struct ModelDefV11 {
    #[serde(default)]
    name: String,
    #[serde(default)]
    flds: Vec<FieldDefV11>,
    // 不反序列化 model.id：有些 Anki 导出把它写成字符串，且 id 由外层 JSON key 提供。
}

#[derive(serde::Deserialize)]
struct FieldDefV11 {
    #[serde(default)]
    ord: usize,
    #[serde(default)]
    name: String,
}

#[derive(serde::Deserialize)]
struct DeckDefV11 {
    #[serde(default)]
    name: String,
}
