# 使用说明

本文档面向安装后的 Fishword 用户，记录稳定的 CLI 使用方式和计划中的安装入口。当前项目仍在开发中，正式 npm 包、平台二进制下载和 Pi extension 发布尚未完成。

## 安装方式

### npm 安装（计划）

正式发布后，推荐通过 npm 安装 CLI：

```bash
npm install -g @fishword/cli
```

安装后验证：

```bash
fishword --version
fishword --help
```

### 直接下载 CLI（计划）

后续会提供各平台 CLI 二进制下载。下载后需要确保 `fishword` 在 `PATH` 中，或使用二进制的绝对路径执行。

```bash
fishword --version
```

### Pi extension 安装（计划）

Pi extension 发布后，用户只需要安装扩展包：

```bash
pi install npm:@fishword/pi-extension
```

Pi extension 会依赖 `@fishword/cli`，安装扩展时会自动获得 CLI。当前 Pi extension 仍在开发中。

## 初始化

首次使用先初始化本地数据库：

```bash
fishword init
```

`fishword init` 会自动导入随 npm CLI 包附带的默认词库，目前包括 `cet4`、
`cet6` 和 `toefl`。用户不需要额外下载词库，也不需要手动执行导入命令。

如果只想创建空数据库：

```bash
fishword init --skip-default-decks
```

默认数据库位置由系统决定。macOS 上通常位于：

```text
~/Library/Application Support/fishword/fishword.db
```

## 导入额外词库

默认词库以外，也可以导入其它 Qwerty Learner JSON：

```bash
fishword import qwerty assets/dicts/qwerty-learner/dicts/CET4_T.json --deck cet4 --name "CET-4"
```

导入 CSV：

```bash
fishword import csv ./words.csv --deck custom --name "Custom"
```

导入 JSONL：

```bash
fishword import jsonl ./deck.jsonl --deck custom --name "Custom"
```

导入 Anki TSV：

```bash
fishword import anki-tsv ./anki.txt --deck anki --name "Anki"
```

查看词库和卡片：

```bash
fishword deck list
fishword card list --deck cet4
```

选择当前学习词库：

```bash
fishword deck use cet4
fishword deck current
```

## 学习命令

查看当前卡片：

```bash
fishword current --json
```

选择下一张卡片：

```bash
fishword next --json
```

评分：

```bash
fishword rate again --json
fishword rate hard --json
fishword rate good --json
fishword rate easy --json
```

`current` 和 `next` 不写 review log。只有 `rate again|hard|good|easy` 会更新学习状态。

也可以用 `--deck` 为单次命令指定词库作用域：

```bash
fishword current --deck cet4 --json
fishword next --deck cet4 --json
fishword rate good --deck cet4 --json
```

## 输出格式

前端集成应使用 JSON：

```bash
fishword current --json
```

人工查看可以使用 plain/compact/status：

```bash
fishword current --format plain
fishword current --format compact
fishword current --format status
```

## 当前限制

- 暂不提供词库编辑器。
- Pi extension、Claude Code adapter、Codex adapter 尚未发布。
