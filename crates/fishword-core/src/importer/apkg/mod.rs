//! `.apkg`（Anki 包）导入解析器。
//!
//! 把 Anki 包解析成 fishword 的 [`ImportCard`] IR，在 IR 层与 `import jsonl` 汇合，
//! 复用同一条持久化管道（见 `crates/fishword-cli/src/cmd/import.rs`）。
//!
//! - 格式与数据库读取：[`reader`]（zip / zstd / sqlite / notetype 双路径）
//! - 字段→角色映射：[`mapping`]（特征匹配分层 + HTML 清理）
//! - 错误：[`error`]（独立 [`ApkgError`]，CLI 映射成 `apkg_*` 错误码）
//!
//! 设计要点（详见 `docs/apkg-import.md`）：
//! 1. 忽略 Anki 的 `tmpls`（正反面模板）与调度字段——fishword 用自己的展示与 FSRS。
//! 2. 字段映射靠**内容特征 + ord 位置**，不靠字段名（字段名不可信：TOEFL 的 `pos`
//!    字段实际存 IPA）。用户可用 `--map` 覆盖。
//! 3. 一个 note 产一张 ImportCard（按 note 去重，与 fishword storage 的单词去重一致）。

mod error;
mod mapping;
mod reader;

#[cfg(test)]
mod tests;

pub use error::{ApkgError, ApkgResult};

use std::collections::HashMap;
use std::path::Path;

use crate::importer::ImportCard;

pub(crate) use mapping::{
    classify_fields, clean_html, map_note_with_bindings, resolve_field_map, NoteMapOutcome,
};
pub(crate) use reader::{AnkiNote, ApkgReader, Notetype};

/// 字段被映射到的语义角色。也是 `--map` 的目标枚举与 inspect 报告里的角色标签。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldRole {
    Term,
    Phonetic,
    Definition,
    Example,
    PartOfSpeech,
    Ignore,
}

impl FieldRole {
    /// CLI / inspect 文本里用的角色名（也是 `--map role=...` 接受的字符串）。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Term => "term",
            Self::Phonetic => "phonetic",
            Self::Definition => "definition",
            Self::Example => "example",
            Self::PartOfSpeech => "pos",
            Self::Ignore => "ignore",
        }
    }

    /// 把 `--map` 的角色字符串解析成 [`FieldRole`]。
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "term" => Self::Term,
            "phonetic" => Self::Phonetic,
            "definition" => Self::Definition,
            "example" => Self::Example,
            "pos" => Self::PartOfSpeech,
            "ignore" => Self::Ignore,
            _ => return None,
        })
    }
}

impl std::fmt::Display for FieldRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `--map role=selector` 里的字段选择器：按索引或按字段名。
#[derive(Debug, Clone)]
pub enum FieldSelector {
    Index(usize),
    Name(String),
}

/// 用户通过 `--map` 提供的字段覆盖。每个角色可绑定多个字段（例如多个 definition
/// 字段 → 多条 meaning）。
#[derive(Debug, Default, Clone)]
pub struct FieldMap {
    pub term: Vec<FieldSelector>,
    pub phonetic: Vec<FieldSelector>,
    pub definition: Vec<FieldSelector>,
    pub example: Vec<FieldSelector>,
    pub pos: Vec<FieldSelector>,
    pub ignore: Vec<FieldSelector>,
}

impl FieldMap {
    /// 解析 `["term=0", "definition=Meaning", "ignore=Audio"]` 形式的 --map 参数。
    pub fn parse(specs: &[String]) -> ApkgResult<Self> {
        let mut map = Self::default();
        for spec in specs {
            let (role_str, selector_str) = spec.split_once('=').ok_or_else(|| {
                ApkgError::InvalidMap(format!(
                    "'{spec}' — expected role=selector (e.g. term=0 or definition=Meaning)"
                ))
            })?;
            let role = FieldRole::parse(role_str.trim()).ok_or_else(|| {
                ApkgError::InvalidMap(format!(
                    "'{spec}' — unknown role '{role_str}' (expected term/phonetic/definition/example/pos/ignore)"
                ))
            })?;
            let selector_str = selector_str.trim();
            let selector = if selector_str.is_empty() {
                return Err(ApkgError::InvalidMap(format!(
                    "'{spec}' — selector is empty"
                )));
            } else if let Ok(index) = selector_str.parse::<usize>() {
                FieldSelector::Index(index)
            } else {
                FieldSelector::Name(selector_str.to_string())
            };
            match role {
                FieldRole::Term => map.term.push(selector),
                FieldRole::Phonetic => map.phonetic.push(selector),
                FieldRole::Definition => map.definition.push(selector),
                FieldRole::Example => map.example.push(selector),
                FieldRole::PartOfSpeech => map.pos.push(selector),
                FieldRole::Ignore => map.ignore.push(selector),
            }
        }
        Ok(map)
    }

    /// 按角色遍历所有选择器（供 [`mapping`] 的 resolve 用）。
    pub(crate) fn iter(&self) -> impl Iterator<Item = (FieldRole, &Vec<FieldSelector>)> {
        [
            (FieldRole::Term, &self.term),
            (FieldRole::Phonetic, &self.phonetic),
            (FieldRole::Definition, &self.definition),
            (FieldRole::Example, &self.example),
            (FieldRole::PartOfSpeech, &self.pos),
            (FieldRole::Ignore, &self.ignore),
        ]
        .into_iter()
    }
}

/// apkg 导入选项。
#[derive(Debug, Clone)]
pub struct ApkgImportOptions {
    /// 导入卡片的语言（写入 ImportCard.language），默认 "en"。
    pub language: String,
    /// 用户 `--map` 覆盖。
    pub field_map: FieldMap,
}

impl Default for ApkgImportOptions {
    fn default() -> Self {
        Self {
            language: "en".to_string(),
            field_map: FieldMap::default(),
        }
    }
}

/// `--inspect` 的诊断报告：每个 notetype 的字段映射结果 + 置信度。
#[derive(Debug, Clone)]
pub struct ApkgInspectReport {
    pub note_count: usize,
    pub notetypes: Vec<NotetypeInspection>,
}

/// 单个 notetype 的字段映射检查结果。
#[derive(Debug, Clone)]
pub struct NotetypeInspection {
    pub id: i64,
    pub name: String,
    pub note_count: usize,
    pub fields: Vec<FieldInspection>,
}

/// 单个字段的检查结果：检测到的角色 + 置信度（采样中该角色的占比）+ 多条样本值
/// （让用户一眼看出字段内容形态，判断自动识别对不对）。
#[derive(Debug, Clone)]
pub struct FieldInspection {
    pub ord: usize,
    pub name: String,
    pub role: FieldRole,
    pub confidence: f64,
    pub samples: Vec<String>,
}

/// inspect 采样上限：前 N 条 note 用于统计字段角色分布。
const INSPECT_SAMPLE: usize = 50;
/// 每个字段最多展示几条样本值（去重后）。
const INSPECT_SAMPLES_PER_FIELD: usize = 3;

/// 解析 apkg 文件成 ImportCard 列表（IR 层，不触碰 storage）。
pub fn import_apkg_file(path: &Path, options: &ApkgImportOptions) -> ApkgResult<Vec<ImportCard>> {
    let bytes = std::fs::read(path).map_err(|e| ApkgError::InvalidZip(e.to_string()))?;
    import_apkg_bytes(&bytes, options)
}

/// 从字节解析 apkg（供测试直接传入构造好的包字节）。
pub fn import_apkg_bytes(bytes: &[u8], options: &ApkgImportOptions) -> ApkgResult<Vec<ImportCard>> {
    let ApkgContents {
        notetypes,
        deck_names,
        notes,
    } = load_apkg(bytes)?;

    // 按 notetype 预解析 field_map（避免每条 note 重算）；解析失败的 notetype 标记为跳过。
    // 多 notetype 时，单个 notetype 解析失败（如 --map 引用其不存在的字段）不应中断整次导入。
    let mut resolved_maps: HashMap<i64, ApkgResult<HashMap<usize, FieldRole>>> = HashMap::new();
    for (&ntid, notetype) in &notetypes {
        resolved_maps.insert(ntid, resolve_field_map(&options.field_map, notetype));
    }

    let mut cards = Vec::with_capacity(notes.len());
    let mut skipped_no_term = 0usize;
    for note in &notes {
        let Some(notetype) = notetypes.get(&note.notetype_id) else {
            continue; // note 引用了未知 notetype，跳过。
        };
        // 该 notetype 的 field_map 解析失败 → 跳过其所有 note（计入 skipped，行为同 SkippedNoTerm）。
        let Some(Ok(user_bindings)) = resolved_maps.get(&note.notetype_id) else {
            skipped_no_term += 1;
            continue;
        };
        match map_note_with_bindings(
            note,
            notetype,
            &options.language,
            user_bindings,
            &deck_names,
        )? {
            NoteMapOutcome::Card(card) => cards.push(card),
            NoteMapOutcome::SkippedNoTerm => skipped_no_term += 1,
            NoteMapOutcome::SkippedNoContent => {}
        }
    }
    if cards.is_empty() {
        // 全空时区分原因：
        // 1. 若只有一个 notetype 且它解析失败 → 返回该 FieldNotFound（保留单 notetype 字段拼错时的精确报错）
        // 2. 若主要因为识别不出单词字段 → NoTerm（引导用户 --inspect/--map）
        // 3. 否则 → NoCards
        if notetypes.len() == 1 {
            // 单 notetype：检查它是否解析失败
            for (_, result) in resolved_maps {
                result?;
            }
        }
        return Err(if skipped_no_term > 0 {
            ApkgError::NoTerm
        } else {
            ApkgError::NoCards
        });
    }
    Ok(cards)
}

/// apkg 解析出的原始数据（读完后 sqlite 连接与临时文件已释放）。
struct ApkgContents {
    notetypes: HashMap<i64, Notetype>,
    deck_names: HashMap<i64, String>,
    notes: Vec<AnkiNote>,
}

/// 打开 apkg 字节流，读取 notetypes / deck_names / notes 并做空校验。读完后 drop reader
/// （关闭 sqlite 连接 + 清理临时文件）。[`import_apkg_bytes`] 与 [`inspect_apkg_bytes`] 共用，
/// 避免两入口重复这段读取前导。
fn load_apkg(bytes: &[u8]) -> ApkgResult<ApkgContents> {
    let reader = ApkgReader::from_bytes(bytes)?;
    let notetypes = reader.read_notetypes()?;
    if notetypes.is_empty() {
        return Err(ApkgError::EmptyCollection);
    }
    let deck_names = reader.read_deck_names()?;
    let notes = reader.read_notes()?;
    if notes.is_empty() {
        return Err(ApkgError::EmptyCollection);
    }
    drop(reader); // 关闭 sqlite 连接 + 清理临时文件。
    Ok(ApkgContents {
        notetypes,
        deck_names,
        notes,
    })
}

/// 按 notetype 分组 note（保持首次出现顺序），跳过引用未知 notetype 的 note。
fn group_notes_by_notetype(
    notes: Vec<AnkiNote>,
    notetypes: &HashMap<i64, Notetype>,
) -> (Vec<i64>, HashMap<i64, Vec<AnkiNote>>) {
    let mut order: Vec<i64> = Vec::new();
    let mut groups: HashMap<i64, Vec<AnkiNote>> = HashMap::new();
    for note in notes {
        if !notetypes.contains_key(&note.notetype_id) {
            continue;
        }
        if !groups.contains_key(&note.notetype_id) {
            order.push(note.notetype_id);
        }
        groups.entry(note.notetype_id).or_default().push(note);
    }
    (order, groups)
}

/// 解析 apkg 并产出字段映射检查报告（不导入）。
pub fn inspect_apkg_file(
    path: &Path,
    options: &ApkgImportOptions,
) -> ApkgResult<ApkgInspectReport> {
    let bytes = std::fs::read(path).map_err(|e| ApkgError::InvalidZip(e.to_string()))?;
    inspect_apkg_bytes(&bytes, options)
}

/// 从字节产出 inspect 报告（供测试直接传入构造好的包字节）。
pub fn inspect_apkg_bytes(
    bytes: &[u8],
    options: &ApkgImportOptions,
) -> ApkgResult<ApkgInspectReport> {
    let ApkgContents {
        notetypes, notes, ..
    } = load_apkg(bytes)?;

    // 按 notetype 分组（保持出现顺序）。
    let (order, mut groups) = group_notes_by_notetype(notes, &notetypes);

    let mut notetype_reports = Vec::with_capacity(order.len());
    let mut total_notes = 0usize;
    let mut first_error: Option<ApkgError> = None;

    for mid in order {
        let notetype = &notetypes[&mid];
        let group_notes = groups.remove(&mid).unwrap_or_default();
        total_notes += group_notes.len();
        // 解析失败的 notetype 跳过（不纳入报告），多 notetype 时不应中断整次 inspect。
        match inspect_notetype(notetype, &group_notes, options) {
            Ok(fields) => {
                notetype_reports.push(NotetypeInspection {
                    id: mid,
                    name: notetype.name.clone(),
                    note_count: group_notes.len(),
                    fields,
                });
            }
            Err(e) => {
                if first_error.is_none() {
                    first_error = Some(e);
                }
            }
        }
    }

    // 若所有 notetype 都解析失败 → 返回第一个错误（保留单 notetype 字段拼错时的精确报错）。
    if notetype_reports.is_empty() {
        if let Some(e) = first_error {
            return Err(e);
        }
    }

    Ok(ApkgInspectReport {
        note_count: total_notes,
        notetypes: notetype_reports,
    })
}

/// 对单个 notetype 采样前 [`INSPECT_SAMPLE`] 条 note，统计每个字段的角色分布，
/// 取占比最高的角色作为该字段的检测结果。
///
/// 解析失败（如 `--map` 引用不存在的字段）时返回 `Err`；调用方应跳过该 notetype
/// 而非中断整次 inspect（多 notetype 时只跳过失败的）。
fn inspect_notetype(
    notetype: &Notetype,
    notes: &[AnkiNote],
    options: &ApkgImportOptions,
) -> ApkgResult<Vec<FieldInspection>> {
    let user_bindings = resolve_field_map(&options.field_map, notetype)?;

    let sample: Vec<&AnkiNote> = notes.iter().take(INSPECT_SAMPLE).collect();
    let sampled = sample.len().max(1);

    // role_counts[ord][role] = 命中次数。
    let mut role_counts: HashMap<usize, HashMap<FieldRole, usize>> = HashMap::new();
    // samples[ord] = 去重后的样本值（最多 INSPECT_SAMPLES_PER_FIELD 条），供用户人工核对。
    let mut samples: HashMap<usize, Vec<String>> = HashMap::new();
    for note in &sample {
        // 字段值预算清洗一次：classify 与样本展示共用，避免重复 clean_html。
        let cleaned: Vec<String> = note.fields.iter().map(|f| clean_html(f)).collect();
        let roles = classify_fields(&cleaned, notetype, &user_bindings);
        for (ord, role) in roles.iter().enumerate() {
            *role_counts
                .entry(ord)
                .or_default()
                .entry(*role)
                .or_default() += 1;
            let bucket = samples.entry(ord).or_default();
            if bucket.len() >= INSPECT_SAMPLES_PER_FIELD {
                continue;
            }
            let trimmed = cleaned.get(ord).map(String::as_str).unwrap_or("").trim();
            if trimmed.is_empty() {
                continue;
            }
            let candidate = truncate_sample(trimmed);
            if !bucket.iter().any(|existing| existing == &candidate) {
                bucket.push(candidate);
            }
        }
    }

    let mut fields = Vec::with_capacity(notetype.fields.len());
    for field in &notetype.fields {
        let counts = role_counts.remove(&field.ord).unwrap_or_default();
        let (role, confidence) = counts
            .into_iter()
            .max_by_key(|(_, c)| *c)
            .map(|(role, c)| (role, c as f64 / sampled as f64))
            .unwrap_or((FieldRole::Ignore, 0.0));
        fields.push(FieldInspection {
            ord: field.ord,
            name: field.name.clone(),
            role,
            confidence,
            samples: samples.remove(&field.ord).unwrap_or_default(),
        });
    }
    Ok(fields)
}

fn truncate_sample(value: &str) -> String {
    const MAX: usize = 60;
    if value.chars().count() <= MAX {
        return value.to_string();
    }
    let truncated: String = value.chars().take(MAX).collect();
    format!("{truncated}…")
}
