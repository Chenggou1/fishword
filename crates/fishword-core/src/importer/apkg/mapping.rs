//! 字段→角色映射：把 Anki note 的原始字段值转成 fishword 的 [`ImportCard`]。
//!
//! 匹配分四层，按优先级从高到低：
//! 1. **用户 `--map` 覆盖**：用户显式指定的字段→角色绑定，永远赢。
//! 2. **位置约定**：`ord == 0` → term（Anki 硬约定第一字段是去重键，几乎 100% 是单词）。
//! 3. **内容特征**：纯媒体引用（`[sound:]`/`<img>`）或清洗后为空 → ignore；
//!    IPA 模式（方括号+非 ASCII，或含 ≥2 个 IPA 专用 Unicode） → phonetic。
//!    这一层**绕开字段名**——正因为有了它，TOEFL 里字段名 `pos` 实际存 IPA
//!    `[ˈesns]` 也能正确判成 phonetic。
//! 4. **字段名 + 内容**：字段名命中某角色关键词且内容非空 → 该角色。多个 example
//!    字段时第一个生效（其余降级为 ignore）。
//!
//! HTML 清理见 [`clean_html`]：剥媒体引用、剥标签、`<br>`/`</div>` → 分隔符、解码实体。

use std::collections::HashSet;

use crate::card::{Meaning, Pronunciation};
use crate::importer::ImportCard;

use super::error::{ApkgError, ApkgResult};
use super::reader::{AnkiNote, Notetype};
use super::{FieldMap, FieldRole, FieldSelector};

// ---- 字段名关键词（子串匹配，ASCII 不区分大小写） ----
// 选词原则：覆盖两份真实样本 + 常见 Anki 模板；宁可漏判（降级 ignore，--map 可拾回）
// 也不要误判（把垃圾塞进 definition/example）。
const TERM_NAME_HINTS: &[&str] = &[
    "word",
    "term",
    "expression",
    "front",
    "单词",
    "词汇",
    "english",
    "英文",
];
const PHONETIC_NAME_HINTS: &[&str] = &["phonetic", "ipa", "phon", "音标", "pinyin"];
const DEFINITION_NAME_HINTS: &[&str] = &[
    "definition",
    "meaning",
    "释义",
    "定义",
    "含义",
    "意思",
    "back",
    "translate",
    "译",
    "chinese",
    "中文",
];
const EXAMPLE_NAME_HINTS: &[&str] = &["example", "例句", "原句", "sentence"];
const POS_NAME_HINTS: &[&str] = &["词性", "pos", "part of speech", "part-of-speech"];

/// IPA 专用 Unicode 字符集合。regular 英文/中文文本几乎不会出现这些。
const IPA_CHARS: &[char] = &[
    'ə', 'ɪ', 'ɑ', 'ʌ', 'ɒ', 'æ', 'ɛ', 'ɔ', 'ʊ', 'ː', 'ˈ', 'ˌ', 'ŋ', 'ð', 'θ', 'ʃ', 'ʒ', 'ʤ', 'ʧ',
    'ɚ', 'ɝ',
];

/// 一条 note 映射后的结果：要么成卡，要么被跳过（带原因，供 collection 层聚合报错）。
pub(crate) enum NoteMapOutcome {
    Card(ImportCard),
    /// 识别不出单词字段（term）——没单词就没法学。
    SkippedNoTerm,
    /// 有单词但没有任何释义/例句内容。
    SkippedNoContent,
}

/// 接受预解析的 user_bindings（索引→角色映射），跳过 [`resolve_field_map`]。
/// 供 [`import_apkg_bytes`] 在循环前预解析所有 notetype 的 field_map，避免每条 note 重算。
pub(crate) fn map_note_with_bindings(
    note: &AnkiNote,
    notetype: &Notetype,
    language: &str,
    user_bindings: &std::collections::HashMap<usize, FieldRole>,
    deck_names: &std::collections::HashMap<i64, String>,
) -> ApkgResult<NoteMapOutcome> {
    // 字段值预算清洗一次：classify 与 build 共用同一份，避免双重 clean_html。
    let cleaned: Vec<String> = note.fields.iter().map(|f| clean_html(f)).collect();
    // 1. 对每个字段判定角色（用户覆盖优先，其次特征匹配）。
    let roles = classify_fields(&cleaned, notetype, user_bindings);
    // 2. 按角色组装 ImportCard。
    build_import_card(note, &cleaned, &roles, language, deck_names)
}

/// 对每个字段判定角色（`fields` 需为已 `clean_html` 清洗的值）。`user_bindings` 是已解析的
/// --map（索引→角色）。供 [`map_note_with_bindings`] 与 inspect 报告共用：inspect 需要逐字段
/// 角色统计置信度。
pub(crate) fn classify_fields(
    fields: &[String],
    notetype: &Notetype,
    user_bindings: &std::collections::HashMap<usize, FieldRole>,
) -> Vec<FieldRole> {
    let field_count = fields.len();
    // 预先把用户绑定的「唯一型」角色标为已占用，避免 auto-match 给 ord 0 等位置
    // 重复分配（例如用户 --map term=1 时，ord 0 不应再被自动判成 term）。
    let mut taken: HashSet<FieldRole> = user_bindings
        .values()
        .copied()
        .filter(is_unique_role)
        .collect();
    let mut roles: Vec<FieldRole> = Vec::with_capacity(field_count);
    for ord in 0..field_count {
        let assigned = if let Some(role) = user_bindings.get(&ord).copied() {
            role
        } else {
            auto_match_field(ord, notetype, fields, &taken)
        };
        if is_unique_role(&assigned) {
            taken.insert(assigned);
        }
        roles.push(assigned);
    }
    roles
}

/// 唯一型角色：一个 note 里至多出现一次（term/example/pos）。phonetic 与 definition
/// 可重复（多条音标 / 多条释义）。
fn is_unique_role(role: &FieldRole) -> bool {
    matches!(
        role,
        FieldRole::Term | FieldRole::Example | FieldRole::PartOfSpeech
    )
}

/// 特征匹配自动判定单个字段角色（Layer 1–3）。`fields` 为已 `clean_html` 清洗的字段值
/// （由 [`classify_fields`] 预算）。`taken` 是本 note 已被占用的「唯一型」角色
/// （Term/Example/PartOfSpeech），用于让第二个 example 字段降级、以及让用户
/// `--map term=N` 后 ord 0 不再被自动判成 term。
fn auto_match_field(
    ord: usize,
    notetype: &Notetype,
    fields: &[String],
    taken: &HashSet<FieldRole>,
) -> FieldRole {
    let name = notetype
        .fields
        .get(ord)
        .map(|f| f.name.as_str())
        .unwrap_or("");
    // 下划线归一化成空格：Anki 字段名常用 snake_case（part_of_speech、example_translation），
    // 归一化后才能命中带空格的关键词（"part of speech"、"example"）。
    let name_lower = name.to_lowercase().replace('_', " ");

    // Layer 1：ord == 0 → term（若 term 尚未被占用，且字段名不像排序键）。
    // 守卫：实测有牌组把 sort_field（值如 "07-02-01" 年级-学期-单元）放在 ord 0，
    // 此时 ord 0 不是单词，应让 Layer 3 按字段名（如 english）找真正的 term。
    if ord == 0 && !taken.contains(&FieldRole::Term) && !looks_like_sort_field(&name_lower) {
        return FieldRole::Term;
    }

    // `fields` 已在外层预算清洗过，这里直接取值。
    let cleaned = fields.get(ord).map(String::as_str).unwrap_or("");

    // Layer 2：清洗后为空（含纯媒体引用） → ignore。
    if cleaned.trim().is_empty() {
        return FieldRole::Ignore;
    }
    // Layer 2：IPA → phonetic（绕开字段名，解 TOEFL pos 存音标的问题）。
    if is_ipa(cleaned) {
        return FieldRole::Phonetic;
    }
    // Layer 2：词性前缀（n./vt./adj. + 正文） → definition。带词性前缀的内容几乎必是
    // 释义，不依赖字段名（如 Anki.apkg 的 `中文释义` 存 `vt.察觉...`）。
    if looks_like_pos_prefixed_definition(cleaned) {
        return FieldRole::Definition;
    }

    // Layer 3：字段名 + 内容非空。example 先于 definition：名字同时含两者信号时
    // （如「中文例句」），example/例句 更具体，应判 example。
    if name_hints_contain(EXAMPLE_NAME_HINTS, &name_lower) && !taken.contains(&FieldRole::Example) {
        return FieldRole::Example;
    }
    if name_hints_contain(DEFINITION_NAME_HINTS, &name_lower) {
        return FieldRole::Definition;
    }
    if name_hints_contain(PHONETIC_NAME_HINTS, &name_lower) && !taken.contains(&FieldRole::Phonetic)
    {
        return FieldRole::Phonetic;
    }
    if name_hints_contain(POS_NAME_HINTS, &name_lower) && !taken.contains(&FieldRole::PartOfSpeech)
    {
        return FieldRole::PartOfSpeech;
    }
    if name_hints_contain(TERM_NAME_HINTS, &name_lower) && !taken.contains(&FieldRole::Term) {
        return FieldRole::Term;
    }

    FieldRole::Ignore
}

/// 字段名是否像排序/索引键（sort/order/index/序号/编号/排序）。这类字段即使排在
/// ord 0 也不是单词。
fn looks_like_sort_field(name_lower: &str) -> bool {
    ["sort", "order", "index", "序号", "编号", "排序"]
        .iter()
        .any(|hint| name_lower.contains(hint))
}

/// 从已判定的角色 + 字段值构建 ImportCard。返回 [`NoteMapOutcome`]：
/// 无单词 → `SkippedNoTerm`；有单词但无释义/例句 → `SkippedNoContent`；否则成卡。
fn build_import_card(
    note: &AnkiNote,
    cleaned_fields: &[String],
    roles: &[FieldRole],
    language: &str,
    deck_names: &std::collections::HashMap<i64, String>,
) -> ApkgResult<NoteMapOutcome> {
    let mut word = String::new();
    let mut pronunciations = Vec::new();
    let mut definitions: Vec<String> = Vec::new();
    let mut pos_value: Option<String> = None;
    let mut example_value: Option<String> = None;

    for (ord, role) in roles.iter().enumerate() {
        // cleaned_fields 已预算清洗；空值直接跳过，各 arm 无需重复 clean_html + is_empty。
        let trimmed = cleaned_fields
            .get(ord)
            .map(String::as_str)
            .unwrap_or("")
            .trim();
        if trimmed.is_empty() {
            continue;
        }
        match role {
            FieldRole::Term => word = trimmed.to_string(),
            FieldRole::Phonetic => pronunciations.push(Pronunciation {
                notation: trimmed.to_string(),
                audio_url: None,
            }),
            FieldRole::Definition => definitions.push(trimmed.to_string()),
            FieldRole::Example if example_value.is_none() => {
                example_value = Some(trimmed.to_string());
            }
            FieldRole::PartOfSpeech if pos_value.is_none() => {
                pos_value = Some(trimmed.to_string());
            }
            _ => {}
        }
    }

    if word.is_empty() {
        return Ok(NoteMapOutcome::SkippedNoTerm);
    }
    if definitions.is_empty() {
        // 没有 definition：若也没有 pos/example 的任何内容，则该 note 无可学信息。
        if pos_value.is_none() && example_value.is_none() {
            return Ok(NoteMapOutcome::SkippedNoContent);
        }
        definitions.push(String::new());
    }

    let mut meanings = Vec::with_capacity(definitions.len());
    for (index, definition) in definitions.into_iter().enumerate() {
        meanings.push(Meaning {
            part_of_speech: if index == 0 {
                pos_value.clone().unwrap_or_default()
            } else {
                String::new()
            },
            definition,
            example: if index == 0 {
                example_value.clone()
            } else {
                None
            },
        });
    }

    // tags：note 自带标签 + 来源 Anki deck 名（保留出处）。
    let mut tags = note.tags.clone();
    if let Some(deck_id) = note.deck_id {
        if let Some(deck_name) = deck_names.get(&deck_id) {
            if !tags.iter().any(|tag| tag == deck_name) {
                tags.push(deck_name.clone());
            }
        }
    }

    Ok(NoteMapOutcome::Card(ImportCard {
        word,
        language: language.to_string(),
        meanings,
        pronunciations,
        tags,
        source: None,
    }))
}

/// 解析用户 `--map`：把每个选择器解析成字段索引。索引越界或字段名不存在 → 报错。
pub(crate) fn resolve_field_map(
    field_map: &FieldMap,
    notetype: &Notetype,
) -> ApkgResult<std::collections::HashMap<usize, FieldRole>> {
    let mut bindings = std::collections::HashMap::new();
    for (role, selectors) in field_map.iter() {
        for selector in selectors {
            let ord = match selector {
                FieldSelector::Index(index) => {
                    if notetype.fields.get(*index).is_some() {
                        *index
                    } else {
                        return Err(ApkgError::FieldNotFound(format!(
                            "field index {index} (notetype '{}' has {} fields)",
                            notetype.name,
                            notetype.fields.len()
                        )));
                    }
                }
                FieldSelector::Name(name) => notetype
                    .fields
                    .iter()
                    .find(|f| f.name.eq_ignore_ascii_case(name))
                    .map(|f| f.ord)
                    .ok_or_else(|| {
                        ApkgError::FieldNotFound(format!(
                            "field named '{name}' (notetype '{}')",
                            notetype.name
                        ))
                    })?,
            };
            bindings.insert(ord, role);
        }
    }
    Ok(bindings)
}

/// 字段名是否命中任意关键词（子串匹配，关键词本身已小写）。
fn name_hints_contain(hints: &[&str], name_lower: &str) -> bool {
    // ASCII 关键词用小写名直接比；CJK 关键词不区分大小写（本就无大小写），原样比。
    hints.iter().any(|hint| name_lower.contains(hint))
}

// ---- 内容特征检测 ----

/// 判定清洗后的值是否是 IPA 音标。三种形态都认：
/// - 方括号包裹且内含非 ASCII **且** 至少 1 个 IPA 专用字符（`[ˈesns]`、`[pə'si:v]`）
/// - 斜杠包裹且内含非 ASCII **且** 至少 1 个 IPA 专用字符（`/taɪ/`、`/ˈæfrɪkə/`）——短音标可能只有 1 个 IPA 字符，
///   靠 ≥2 阈值会漏，故斜杠形式单独判
/// - 无定界符但含 ≥2 个 IPA 专用字符（`pəˈsiːv`）
///
/// 非纯括号 CJK（如 `[注意]`/`[备注]`）不含 IPA 专用字符，不会误判。
pub(crate) fn is_ipa(value: &str) -> bool {
    let trimmed = value.trim();
    for (open, close) in [('[', ']'), ('/', '/')] {
        if trimmed.starts_with(open) && trimmed.ends_with(close) && trimmed.len() >= 3 {
            let inner = &trimmed[1..trimmed.len() - 1];
            // 收紧：非 ASCII **且** 内含至少 1 个 IPA 专用字符。
            // 避免 `[注意]`（CJK 非 ASCII）被误判为 phonetic。
            if !inner.is_ascii() && inner.chars().any(|c| IPA_CHARS.contains(&c)) {
                return true;
            }
        }
    }
    let ipa_count = value.chars().filter(|c| IPA_CHARS.contains(c)).count();
    ipa_count >= 2
}

/// 词性前缀 + 正文 → 像释义。匹配 `n.`/`vt.`/`adj.` 等词典词性缩写开头，且点号后**有正文**
/// （`n. 本质`、`vt.察觉`）；光秃秃的 `n.`（词性字段）不匹配（rest 为空）。
/// 只认小写——句子开头的 `No.`/`A.`（大写）不会误判。
fn looks_like_pos_prefixed_definition(value: &str) -> bool {
    // 长前缀先判，避免 `v.` 抢先匹配 `vt.`（结果一样，但语义清晰）。
    const MARKERS: &[&str] = &[
        "vt.", "vi.", "adj.", "adv.", "art.", "prep.", "conj.", "num.", "pron.", "aux.", "abbr.",
        "det.", "n.", "v.",
    ];
    let trimmed = value.trim();
    for marker in MARKERS {
        if let Some(rest) = trimmed.strip_prefix(*marker) {
            return !rest.trim().is_empty();
        }
    }
    false
}

// ---- HTML / 媒体清洗 ----

/// 清洗 Anki 字段值：去媒体引用、剥 HTML 标签、`<br>`/`</div>` → `; `、解码实体、
/// 折叠空白。term 字段也走同一清洗（单词通常无标签，结果不变）。
pub(crate) fn clean_html(value: &str) -> String {
    let without_audio = strip_delimited(value, "[sound:", "]");
    let without_images = strip_delimited(&without_audio, "<img", ">");
    // 块级闭合标签 / <br> → 分隔符，保留多行释义的边界。
    let mut s = without_images
        .replace("<br />", "; ")
        .replace("<br/>", "; ")
        .replace("<br>", "; ")
        .replace("</div>", "; ")
        .replace("</p>", "; ")
        .replace("</li>", "; ");
    s = strip_tags(&s);
    s = decode_entities(&s);
    s = collapse_whitespace(&s);
    // 分隔符前的空格（来自 `<br>` 前的尾随空白）去掉，`v.吸收 ; n.吸收` → `v.吸收; n.吸收`。
    s = s.replace(" ;", ";");
    // 去除前导 `;` 及其相邻空格（值以 <br>/</div> 开头时会产生），但保留内部 `;`（如 `a; b; c`）。
    s = s.trim_start_matches(';').trim_start().to_string();
    s.trim().trim_end_matches(';').trim().to_string()
}

/// 移除所有 `<...>` 标签。`<` `>` 是 ASCII，不会切断多字节字符。
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        match after.find('>') {
            Some(end) => rest = &after[end + 1..],
            None => {
                out.push_str(after);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// 移除 `[sound:` ... `]`、`<img` ... `>` 等成对界定包裹的片段。`start`/`end` 必须是 ASCII。
fn strip_delimited(s: &str, start: &str, end: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(begin) = rest.find(start) {
        out.push_str(&rest[..begin]);
        let after = &rest[begin..];
        match after.find(end) {
            Some(close) => rest = &after[close + end.len()..],
            None => {
                out.push_str(after);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// 解码常见 HTML 实体。`&amp;` 必须最后处理，避免 `&amp;lt;` 被二次解码成 `<`。
fn decode_entities(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// 把连续空白（含换行、制表符）折叠成单个空格。
fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipa_bracketed_ascii_inside_detected() {
        assert!(is_ipa("[ˈesns]"));
        assert!(is_ipa("[pə'si:v]"));
    }

    #[test]
    fn ipa_bracketless_multi_char_detected() {
        assert!(is_ipa("pəˈsiːv"));
    }

    #[test]
    fn ipa_slash_delimited_single_char_detected() {
        // /taɪ/ 只有 1 个 IPA 字符，靠斜杠包裹+非ASCII 判定（否则 ≥2 阈值会漏）。
        assert!(is_ipa("/taɪ/"));
        assert!(is_ipa("/ˈæfrɪkə/"));
    }

    #[test]
    fn plain_english_not_ipa() {
        assert!(!is_ipa("cancel"));
        assert!(!is_ipa("n. 本质，精髓"));
        assert!(!is_ipa("[V n prep]"));
    }

    #[test]
    fn clean_strips_audio_and_html() {
        let raw = "[sound:essence.mp3]";
        assert_eq!(clean_html(raw), "");
        let raw = "vt.察觉 <br />v.感知,感到,认识到";
        assert_eq!(clean_html(raw), "vt.察觉; v.感知,感到,认识到");
    }

    #[test]
    fn clean_decodes_entities_and_collapses() {
        let raw = "<div>This means that&nbsp;our noses are <b>limited</b></div>";
        assert_eq!(clean_html(raw), "This means that our noses are limited");
    }

    #[test]
    fn clean_strips_collins_style_nested_html() {
        let raw = "<span class=\"text_blue\">注意到</span> If you <b>perceive</b> something";
        assert_eq!(clean_html(raw), "注意到 If you perceive something");
    }

    #[test]
    fn name_hints_match_chinese_substring() {
        assert!(name_hints_contain(DEFINITION_NAME_HINTS, "中文释义"));
        assert!(name_hints_contain(EXAMPLE_NAME_HINTS, "真题原句"));
        assert!(name_hints_contain(PHONETIC_NAME_HINTS, "英美音标"));
    }

    #[test]
    fn name_hints_match_english_substring() {
        assert!(name_hints_contain(EXAMPLE_NAME_HINTS, "example_en"));
        assert!(!name_hints_contain(DEFINITION_NAME_HINTS, "vocab简明"));
    }

    #[test]
    fn plain_bracketed_cjk_not_ipa() {
        // 纯括号 CJK（如 [注意]、[备注]）不应被误判为 phonetic。
        assert!(!is_ipa("[注意]"));
        assert!(!is_ipa("[备注]"));
        assert!(!is_ipa("[说明]"));
    }

    #[test]
    fn clean_leading_semicolon_after_br() {
        // 值以 <br> 或 </div> 开头时，去除残留的前导分号。
        assert_eq!(clean_html("<br>vt.察觉"), "vt.察觉");
        assert_eq!(clean_html("</div>foo"), "foo");
        assert_eq!(clean_html("</div> vt.察觉"), "vt.察觉");
    }

    #[test]
    fn clean_preserves_internal_semicolons() {
        // 内部分隔符 `;` 应保留（如 `a; b; c`）。
        assert_eq!(clean_html("a; b; c"), "a; b; c");
    }
}
