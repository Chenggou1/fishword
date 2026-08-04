# Anki 词库导入指南

把 Anki 社区流传的 `.apkg` 词库包直接导入 fishword。fishword 会自动识别每个字段是
单词、音标、释义还是例句——**大多数词库不用任何手动配置就能正确导入**。遇到识别
不准的字段，用 `--map` 手动指定即可。

> 这是面向使用者的指南。字段映射的判定机制与设计细节见 [apkg-import.md](apkg-import.md)。

## 快速开始

```bash
fishword import apkg 我的词库.apkg --create-deck 托福      # 建新词库并导入
fishword import apkg 我的词库.apkg --deck-id 3             # 导入到已有词库
```

导入后用 `fishword current` 就能开始复习。

## 第一步：先用 --inspect 看自动识别结果（强烈推荐）

导入陌生词库前，先跑 `--inspect` 看看 fishword 把每个字段认成了什么——不导入，只看：

```bash
fishword import apkg 我的词库.apkg --inspect
```

输出长这样：

```
Notetype "TOEFL 绿宝书" (4127 notes):
   ord  field            role           conf  samples
     0  word             term           100%  essence · pastel · cultivate
     1  pos              phonetic        96%  [ˈesns] · [pæˈstel] · [ˈkʌltɪveɪt]
     2  audio            ignore         100%  (empty)
     3  definition       definition     100%  n. 本质，精髓 · a. 彩色蜡笔的 · vt. 耕种；培养
     4  example_en       example        100%  This constant reshaping... · Pastel colors are...
```

- **ord** = 字段位置（第几个），后面 `--map` 用数字指定时就填这个。
- **field** = 字段名，后面 `--map` 用名字指定时就填这个。
- **role** = fishword 认定它是什么（term/phonetic/definition/example/pos/ignore）。
- **conf** = 置信度（采样中这个判定的占比），接近 100% 说明很稳。
- **samples** = 该字段的几条真实内容样本（多条方便你判断 role 判得对不对——比如
  `pos` 字段三个样本都是 `[xxx]` 形式的音标，就能确认 phonetic 判对了）。

如果 role 列看着都对，直接去掉 `--inspect` 导入即可。**如果某字段判错了，看下文的 `--map`。**

## --map：手动覆盖字段映射

`--map` 让你告诉 fishword「这个字段是什么」，覆盖自动判定。

### 语法

```
--map 角色=字段
```

两部分，用 `=` 连接：

**① 角色**（这个字段「是什么」）——六选一：

| 角色 | 含义 | 导入到哪里 |
|------|------|-----------|
| `term` | 单词 | 卡片标题（去重键，每个词一张卡） |
| `phonetic` | 音标 | 详情面板的音标 |
| `definition` | 释义 | 卡片释义（可有多个，每个产一条释义） |
| `example` | 例句 | 卡片例句 |
| `pos` | 词性 | 释义的词性标注 |
| `ignore` | 忽略 | 这个字段不要（丢弃） |

**② 字段**（指定「哪个」字段）——两种写法任选：

| 写法 | 含义 | 哪来的 |
|------|------|--------|
| 数字，如 `0` `1` `2` | 字段位置（ord） | `--inspect` 输出的 **ord** 列 |
| 文本，如 `中文释义` `Audio` | 字段名 | `--inspect` 输出的 **field** 列 |

### 那串例子逐条拆开

```bash
fishword import apkg 我的词库.apkg --create-deck X \
  --map term=0 --map definition=中文释义 --map ignore=Audio
```

| 这条 | 角色 | 字段 | 意思 |
|------|------|------|------|
| `--map term=0` | term（单词） | `0`（第 0 个字段） | 「第 0 个字段是单词」 |
| `--map definition=中文释义` | definition（释义） | `中文释义`（字段名） | 「名叫『中文释义』的字段是释义」 |
| `--map ignore=Audio` | ignore（忽略） | `Audio`（字段名） | 「名叫『Audio』的字段丢弃掉」 |

三条加在一起：单词取第 0 字段、释义取名为「中文释义」的字段、丢弃 Audio 字段，其余字段
继续自动判定。`--map` 可以写任意多条，**只覆盖你指定的，没指定的字段照常自动识别**。

### 常见用法

| 想做的事 | 怎么写 |
|---------|--------|
| 单词不在第 0 个字段 | `--map term=<单词所在字段的 ord 或名>` |
| 某字段被误判成音标，其实是释义 | `--map definition=<那个字段>` |
| 丢弃某个无用字段（如纯图片、序号） | `--map ignore=<那个字段>` |
| 把第 2 个字段当释义 | `--map definition=2` |
| 指定词性字段 | `--map pos=<词性字段>` |

### 报错怎么办

- `apkg_field_not_found`：`--map` 里写的字段名/序号在词库里不存在。跑 `--inspect` 核对
  字段名和 ord。
- `apkg_invalid_map`：`--map` 写法不对（漏了 `=`、角色名拼错等）。角色只能是
  term/phonetic/definition/example/pos/ignore。
- `apkg_no_term`：fishword 没认出哪个字段是单词。跑 `--inspect` 看字段，然后
  `--map term=<字段>` 指定。

## 支持的 Anki 格式

| 格式 | Anki 版本 | 说明 |
|------|-----------|------|
| anki2 | 2.0 | 旧格式 |
| anki21 | 2.1（≤2.1.54） | schema 11，未压缩 |
| anki21b | 2.1.55+ | zstd 压缩，新格式 |

三种全自动识别，无需指定。

## 已知限制

- **不迁移 Anki 的复习进度**：Anki 用 SM-2、fishword 用 FSRS，两者没有无损换算方式，
  所以导入的词都按**新卡**处理（复习从零开始）。
- **不提取音频 / 图片**：`[sound:...]` 和 `<img>` 引用会从文本中剔除，但不下载媒体文件。
- **仅 CLI**：Pi 扩展端暂未提供字段选择界面。
