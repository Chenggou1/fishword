# apkg 导入

`fishword import apkg <path>` 把 Anki `.apkg` 包导入成 fishword 卡片。字段→角色靠
内容特征自动判定，`--map` 可覆盖。实现：`crates/fishword-core/src/importer/apkg/`，
CLI：`cmd/import.rs::cmd_import_apkg`。

## 格式与检测

| 格式 | 文件名 | Anki 版本 | schema | 压缩 |
|------|--------|-----------|--------|------|
| anki2 | `collection.anki2` | 2.0 | 11 | 无 |
| anki21 | `collection.anki21` | 2.1 ≤2.1.54 | 11 | 无 |
| anki21b | `collection.anki21b` + `meta` | 2.1.55+ | 15+ | zstd |

检测：按 `collection.anki21b` → `.anki21` → `.anki2` 顺序找**实际存在的文件**
（不靠 `meta`——实测有包带 `meta` 却用 `.anki21` 非 zstd）；找到 `.anki21b` 才 zstd 解压。

## Anki 数据模型

anki2 数据库主要表（schema 11）：

- **`col`**（全局配置，仅 1 行）：
  - `models` 列 = 所有 notetype 定义的 JSON：每个 notetype 含 `flds[]`（字段名**图例**——
    「第 0 槽叫 word、第 1 槽叫 pos…」，相当于 Excel 表头）+ `tmpls[]`（正反面渲染模板）。
  - `decks` 列 = 所有 deck 名的 JSON（`did → 名称`）。
  - fishword：读 `models` 拿字段名图例（用来知道 `notes.flds` 每段是什么）、读 `decks` 拿 deck 名；
    忽略 `tmpls`（不渲染 Anki 卡片）。
- **`notes`**（一行 = 一份**内容**，主数据源）：
  - `mid` = notetype id，决定这份内容有哪些字段槽位（配 `col.models` 的图例）。
  - `flds` = 各字段**值**按 `\x1f`（U+001F）拼成一串，顺序对齐 notetype 的 `flds[].ord`。
  - `tags` = 空格分隔的标签，可含 `::` 层级（如 `单词::2005`）。
- **`cards`**（一行 = 一张**可复习的牌**，note→card 是 1:N）：
  - `nid` = 这张牌展示哪个 note 的内容（指向 `notes.id`）；`ord` = 用第几个正反面模板；
    `did` = 属于哪个 deck。
  - 其余 `queue`/`due`/`ivl`/`reps`/`ease`/`lapses` 是 SM-2 调度状态（下次到期、间隔、复习次数…）。
  - fishword：**只取 `did`**（把 Anki deck 名加进 tag 保留出处），丢弃全部调度字段；
    且一个 note 只产一张 ImportCard（不做 Anki 的正反双面区分）。
- **`revlog`**（一行 = 一次评分的复习日志）/ **`graves`**（已删除 note/card 的墓碑，Anki 同步用）：
  fishword 都不读。

以 TOEFL__.apkg 的单词 `essence` 为例，三张表里实际是这样的（已截短）：

```
col（1 行，schema 11）
  models = {"1556532380136":{
    "name":"TOEFL 绿宝书",
    "flds":[{"ord":0,"name":"word"},{"ord":1,"name":"pos"},{"ord":2,"name":"audio"},
            {"ord":3,"name":"definition"},{"ord":4,"name":"example_en"},
            {"ord":5,"name":"example_zh"}, ...]}}
  decks  = {"1":{"name":"托福绿宝书"}}

notes（一行一个 note）
  id=1  mid=1556532380136
  flds = essence \x1f [ˈesns] \x1f [sound:essence_1556532380136.mp3] \x1f n. 本质，精髓（＊basic nature） \x1f This constant reshaping... \x1f 这种不断的重塑...
        ↑ord0    ↑ord1     ↑ord2(音频)                       ↑ord3(释义)                    ↑ord4(英文例句)             ↑ord5(中文译文)
  tags = ""

cards（一行一张牌，note→card 是 1:N）
  id=1  nid=1  did=1  ord=0        # nid→notes.id=1, did→decks.id=1="托福绿宝书", ord=第几个 template
```

`flds` 的 `\x1f`（U+001F）把字段值按 notetype 字段顺序拼成一串；读取器按这个分隔符拆开，
再靠 notetype 的 `flds[].name` 知道每段叫什么。**注意 `pos`（ord 1）这一段存的是音标，不是词性**——
字段名和实际内容对不上，这就是下文字段名不可信、要靠内容特征判定的原因。

**正反面不存在数据库里**。`notes.flds` 只存字段值；正反面是 notetype 的 `tmpls`
（`qfmt`/`afmt`）在展示时把 `{{字段名}}` 替换成字段值渲染出的 HTML。fishword 忽略
`tmpls`，一个 note 产一张 ImportCard。

**schema 11 vs 15+**：11 的 notetype 在 `col.models` JSON；15+ 拆成 `notetypes`/
`fields`/`templates` 表。读取器按 `notetypes` 表是否存在判版本，统一成
`HashMap<i64, Notetype{ name, fields[] }>`。

**字段名不可信**：实测 TOEFL 词库的 `pos` 字段存的是 IPA `[ˈesns]`，不是词性。
所以映射靠内容特征 + ord 位置，不靠字段名。

## 读取流程

```
import_apkg_file(path) → 读字节
  ApkgReader::from_bytes:  zip 打开 → 检测格式 → (anki21b 走 zstd) → 写 tempfile → Connection::open
  read_notetypes()   schema 11 走 col.models JSON；15+ 走 notetypes/fields 表
  read_deck_names()  col.decks JSON 或 decks 表
  read_notes()       SELECT mid, flds, tags, <首张 card 的 did>；flds 按 \x1f 拆
  map_note(note) → ImportCard  (见下)
```

随后 CLI 复用 `persist_import`（与 `import jsonl` 同管道）写入 storage。

## 忽略项

- `tmpls`（模板）：不解析。
- 媒体：`[sound:]`/`<img>` 引用从文本剔除，不提取媒体文件。
- 调度：`cards` 的 queue/ivl/reps/ease 全弃。SM-2 → FSRS 无无损映射，按 ADR-0003 当
  新卡（storage 的 `INSERT OR IGNORE INTO card_state` 自动初始化 New）。
- `graves`/`revlog`：不读。

**多 deck**：合并进单个本地 deck，Anki deck 名作 tag 保留出处。note 自带 tags 原样进 tags。

## 字段映射（`mapping.rs`）

四层，按优先级从高到低，首个命中的层定角色。**关键：内容特征（Layer 2）先于字段名
（Layer 3）判定**——所以一个叫 `pos` 却存 IPA 的字段，Layer 2 就判成 phonetic，不会被
字段名误导。

1. **用户 `--map`**（最高）：显式绑定永远赢；其唯一型角色预先标占用，避免重复分配。
2. **位置**：`ord == 0` → term（Anki 第一字段通常是去重键）。**守卫**：字段名像排序键
   （含 `sort`/`order`/`index`/`序号`/`编号`）时不判 term——实测有牌组把 `sort_field`
   （值 `07-02-01`）放 ord 0，真单词在后面的 `english` 字段。
3. **内容特征**（看清洗后的值，不看字段名）：

   | 内容形态 | → 角色 | 示例 |
   |---|---|---|
   | 空 / 纯空白 | ignore | `` |
   | 清洗后为空（原本只有媒体引用） | ignore | `[sound:essence.mp3]`、`<img src="x.jpg">` |
   | IPA：`[...]` 且内含非 ASCII | phonetic | `[ˈesns]`、`[pə'si:v]` |
   | IPA：`/.../` 且内含非 ASCII | phonetic | `/taɪ/`、`/ˈæfrɪkə/` |
   | IPA：无定界符但含 ≥2 个 IPA 专用字符 | phonetic | `pəˈsiːv` |
   | 词性前缀 + 正文（`n.`/`vt.`/`adj.`… 后跟非空正文） | definition | `n. 本质`、`vt.察觉`、`adj. 有能力的` |
   | 普通文本（上述都不匹配） | 交给 Layer 4 | `取消`、`This constant...` |

   IPA 专用字符集：`ə ɪ ɑ ʌ ɒ æ ɛ ɔ ʊ ː ˈ ˌ ŋ ð θ ʃ ʒ ʤ ʧ ɚ ɝ`
   词性前缀：`n. v. vt. vi. adj. adv. art. prep. conj. num. pron. aux. abbr. det.`（小写；
   点号后必须有正文——光秃秃 `n.` 是词性字段，不算释义）

4. **字段名 + 内容非空**：Layer 3 没命中（普通文本）的字段，看字段名是否子串命中关键词
   （ASCII 不区分大小写，**下划线归一化成空格**——`part_of_speech`→`part of speech`）。
   example 先于 definition 检查（名字同时含两者信号时，如「中文例句」，判 example）。
   唯一型角色（term/example/pos）已被占用则降级 ignore。
   > 为什么普通文本还要看名字：`取消`（释义）、`这种不断的重塑...`（例句译文）、
   > `词根记忆：...`（助记）三者都是纯文本，内容上无法区分，只能靠字段名消歧。

关键词清单（`mapping.rs` 常量）：

| 角色 | 关键词 |
|------|--------|
| term | `word` `term` `expression` `front` `单词` `词汇` `english` `英文` |
| phonetic | `phonetic` `ipa` `phon` `音标` `pinyin` |
| definition | `definition` `meaning` `释义` `定义` `含义` `意思` `back` `translate` `译` `chinese` `中文` |
| example | `example` `例句` `原句` `sentence` |
| pos | `词性` `pos` `part of speech` `part-of-speech` |

### term（单词）与 definition（释义）怎么确认

这两个最关键的字段，确认方式不一样——**term 有位置信号，definition 只能靠名字**：

**term**（优先级从高到低，首个命中即定）：
1. **`--map term=N`**：用户显式指定。
2. **位置（Layer 1）**：`ord == 0` 默认就是 term——Anki 约定第一字段是去重键（单词本身），
   几乎 100% 命中。**除非**字段名像排序键（`sort`/`order`/`index`/`序号`）才跳过
   （见 basic_word.apkg：ord 0 是 `sort_field`，真单词在 ord 1 的 `english`）。
3. **名字兜底（Layer 3）**：ord 0 被跳过或被占用时，靠字段名命中 term 关键词
   （`word`/`english`/`单词`/…）找真正的单词字段。

> 所以 term 主要靠「排第一」这个位置确认，不是靠字段名叫 `word`——这也是为什么
> 即使字段名是中文 `英语单词`、`Front` 甚至没明显名字，ord 0 照样能认成单词。

**definition**（没有位置信号，但有两个内容/名字信号）：
1. **词性前缀内容**（Layer 2）：内容以 `n.`/`vt.`/`adj.`… 开头且后跟正文 → definition。
   这是不依赖字段名的强信号（如 Anki.apkg 的 `中文释义` 存 `vt.察觉...`）。
2. **字段名**（Layer 3）：字段名命中 definition 关键词（`definition`/`释义`/`chinese`/`meaning`/
   `译`/…）且内容非空 → definition。
3. 多个 definition 字段 → 每个产一条 meaning（definition 角色可重复）。
4. `--map definition=N` 覆盖。

> 为什么 definition 不能像 term 靠位置：Anki 对「第几字段是释义」没有任何约定——可能
> ord 1 也可能 ord 5，所以只能靠字段名 + 内容非空来认。若某牌组的释义字段名不命中任何
> 关键词（罕见），用 `--map definition=字段名` 显式指定。

## `--map`

```
fishword import apkg X.apkg --create-deck 名 --map 'role=selector' [--map ...]
```

- role：`term`/`phonetic`/`definition`/`example`/`pos`/`ignore`
- selector：字段索引（`term=0`）或字段名（`definition=中文释义`）
- 可重复、部分覆盖。索引越界/字段名不存在 → `apkg_field_not_found`

## `--inspect`

不导入，打印每个 notetype 的字段映射（采样前 50 条，取占比最高角色 + 置信度 + 样本）。
文本诊断，全程忽略 `--json`（报错路径与成功路径一致，均输出纯文本）。

```
Notetype "TOEFL 绿宝书" (4127 notes):
   ord  field            role           conf  samples
     0  word             term           100%  essence · pastel · cultivate
     1  pos              phonetic        96%  [ˈesns] · [pæˈstel] · [ˈkʌltɪveɪt]
     2  audio            ignore         100%  (empty)
     3  definition       definition     100%  n. 本质，精髓 · a. 彩色蜡笔的 · vt. 耕种
     4  example_en       example        100%  This constant reshaping... · Pastel colors are...
```

## HTML 清洗（`clean_html`，顺序重要）

1. 剔除 `[sound:...]`、`<img ...>`
2. `<br>`/`<br/>`/`</div>`/`</p>`/`</li>` → `; `
3. 剥其余 `<...>` 标签
4. 解码实体：`&nbsp;`→空格 `&lt;`→`<` `&gt;`→`>` `&quot;`→`"` `&#39;`/`&#x27;`/`&apos;`→`'`
   （`&amp;`→`&` 最后做，避免二次解码）
5. 折叠空白；去分隔符前空格（` ;`→`;`）；trim

## 错误码

| 错误码 | 触发条件 |
|--------|----------|
| `apkg_invalid_zip` | ZIP 读不出 / IO 错误 |
| `apkg_missing_collection` | 无 `collection.anki*` |
| `apkg_zstd_decode` | anki21b 解压失败 |
| `apkg_invalid_database` | SQLite 打开/查询失败，或非 anki2 schema |
| `apkg_empty_collection` | 无 notetype 或无 note |
| `apkg_no_cards` | note 存在但映射后无可用卡片 |
| `apkg_no_term` | 所有 note 都识别不出单词字段（term）——消息会引导 `--inspect` + `--map term=` |
| `apkg_invalid_map` | `--map` 语法错 |
| `apkg_field_not_found` | `--map` 字段索引/名不存在 |

## v1 不做

调度迁移（SM-2→FSRS）、媒体提取、Pi 扩展端字段选择 GUI、`tmpls` 解析。
