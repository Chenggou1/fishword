# Fishword

> 代码在跑，单词在长。  
> 摸鱼和 Vibe Coding 间隙，顺手背几个单词。

[![npm](https://img.shields.io/npm/v/@fishword/pi-extension)](https://www.npmjs.com/package/@fishword/pi-extension)
[![license](https://img.shields.io/badge/license-GPL--3.0--only-blue)](#license)

Fishword 是一个藏在开发环境里的背单词工具。当你在等待 AI 回复、命令执行或测试完成时，它会用一张轻量词卡接住这些碎片时间。

它使用 [FSRS](https://github.com/open-spaced-repetition/fsrs4anki/wiki/The-Algorithm) 算法调度复习，学习数据保存在本地 SQLite 中。你可以通过 CLI 直接使用，也可以通过 Pi.dev 扩展把它嵌进日常开发流程。

[查看产品展示页](https://chenggou1.github.io/fishword/)：核心演示、功能 GIF、安装入口和 CLI 说明。

---

## Why Fishword?

- **为摸鱼和 Vibe Coding 间隙设计**：AI 回复、命令执行、测试运行的空档，都可以顺手复习几个单词。
- **学习数据在本地**：词库、复习进度和学习记录保存在本地 SQLite 中，不依赖账号和云端服务。
- **间隔重复复习**：使用 FSRS 根据你的评分动态调整复习时间，让每次评分都影响下一次出现时间。

## Quick Start

### Pi.dev Extension

```
pi install npm:@fishword/pi-extension
```

重启 Pi 后，扩展自动完成初始化，通过 Fishword catalog 下载三个默认词库：

| 词库 | 词条数 |
|------|--------|
| CET-4 | 4,544 |
| CET-6 | 3,992 |
| TOEFL | 10,377 |

默认激活 CET-4，无需任何配置，打开 Pi 即可开始背单词。

## Features

- Pi.dev 内轻量词卡，不离开开发环境也能复习。
- 老板键式隐藏 / 唤起，摸鱼时更从容。
- 快捷键评分，适合键盘优先的复习流程。
- 详情面板展示音标、词性、释义和例句。
- 学习统计展示今日完成量、新词数和 7 日趋势。
- 词库管理器支持自建词库导入、本地词库切换、删除和远程词库下载。
- 每个词库保留独立 FSRS 复习进度，CET-4、TOEFL 等目标互不影响。

## Usage

### 隐藏 / 唤起

先按 **`Ctrl+Q`**，松开后再按 **`F`**，即可隐藏或唤起 Fishword。还可以输入 `/fw` 执行相同操作。

### 评分

词卡显示后，用快捷键评分：

| 连续快捷键 | 评分 | 含义 |
|------------|------|------|
| `Ctrl+Q G` | good | 记住了 |
| `Ctrl+Q H` | hard | 有点难 |
| `Ctrl+Q A` | again | 没记住，下次再来 |
| `Ctrl+Q E` | easy | 轻松，拉长复习间隔 |

评分后自动显示下一张，FSRS 算法根据你的表现动态调整复习时间。

### 详情面板

按 **`Ctrl+Q I`** 打开详情面板。面板展示完整音标（US / UK）、词性、释义和例句，并支持 `A` / `H` / `G` / `E` 单键评分。按 `Esc` 关闭面板并返回词卡视图。

### 学习统计

输入 `/fw-stats` 查看今日完成量、学习新词数和 7 日学习趋势。

### 词库管理

输入 `/fw-manage` 打开词库管理器，内含两个页面（左右箭头切换）：

- **我的词库**：列出所有本地词库，`Enter` 切换激活词库，`i` 导入自建词库，`d` 删除词库（需确认）。
- **词库目录**：浏览远程词库目录，选中后按 `Enter` 下载并导入。

导入自建词库时，先选择本地 `.jsonl` 文件，再确认词库名称。导入成功后会自动切换到新词库。首期支持 `fishword.deck.v1` JSONL 格式，可以复制并修改[示例词库](examples/custom-deck.jsonl)。

## Dictionaries

每个词库都有**独立的 FSRS 调度状态**——CET-4 的复习进度不会影响 CET-6，反之亦然。你可以同时维护多个词库，在不同学习目标之间自由切换，算法会分别记住你对每个词的掌握程度。

例如：备考四级时激活 CET-4，考完后切换到 TOEFL 备考托福，两套进度互不干扰。

目前提供 CET-4、CET-6、TOEFL、IELTS、GRE、SAT、Oxford 3000、Oxford 5000 等词库。

---

## Development

如需二次开发、构建或接入新的集成，请参阅 [docs/development.md](docs/development.md)。

---

## Acknowledgements

- 感谢 [LINUX DO](https://linux.do/) 社区。
- 感谢阮一峰老师在[《科技爱好者周刊（第 401 期）：如何赚到10亿美元》](https://www.ruanyifeng.com/blog/2026/06/weekly-issue-401.html)中推荐 Fishword。

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=Chenggou1/fishword&type=Date)](https://star-history.com/#Chenggou1/fishword&Date)

---

## License

[GPL-3.0-only](LICENSE)
