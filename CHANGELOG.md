# Changelog

## [0.1.4](https://github.com/Chenggou1/fishword/compare/v0.1.3...v0.1.4) (2026-06-16)


### Features

* add Fishword boss key ([2f52703](https://github.com/Chenggou1/fishword/commit/2f52703607603d5b286e358bff4c1bcf7e8469da)), closes [#9](https://github.com/Chenggou1/fishword/issues/9) [#10](https://github.com/Chenggou1/fishword/issues/10)
* show shortcuts in slash commands ([c854c58](https://github.com/Chenggou1/fishword/commit/c854c58f6e91a172369154b7369b2103bcd18888))


### Bug Fixes

* launch pi from dev script ([628d5d6](https://github.com/Chenggou1/fishword/commit/628d5d6c7ec36ddeab359234e420e6d3761db99f))
* publish releases from release-please ([adac375](https://github.com/Chenggou1/fishword/commit/adac3757b34f6e2785190206ac4e02a89ee9fe6a))
* update Cargo.lock in release-please config ([ef1b121](https://github.com/Chenggou1/fishword/commit/ef1b1212b9d5307040b613f71676628e8ba05b17))

## [0.1.3](https://github.com/Chenggou1/fishword/compare/v0.1.2...v0.1.3) (2026-06-15)


### Features

* add named imports and clean empty tags ([54fb67a](https://github.com/Chenggou1/fishword/commit/54fb67afecfcfc5fe37869b0afbb02382474e3a6))


### Bug Fixes

* align release-please tag lookup ([9260816](https://github.com/Chenggou1/fishword/commit/92608163c49734ce0ff15355f6c04de0e0b8efe6))
* make imports atomic ([486a488](https://github.com/Chenggou1/fishword/commit/486a488b02ebfba9d1d4f312dfc08d588acc3cbb))
* move tag-name to root level so release-please generates v${version} tags ([c379384](https://github.com/Chenggou1/fishword/commit/c37938437ddd89aa4aa6784426b44c35592a9239))

## [0.1.2](https://github.com/Chenggou1/fishword/compare/v0.1.1...v0.1.2) (2026-06-11)


### Features

* **pi-extension:** add detail overlay with phonetics, examples, and in-panel rating ([1ed3efc](https://github.com/Chenggou1/fishword/commit/1ed3efc190a8d22f7e47fe9342df1aa3c28c94af))


### Bug Fixes

* set release-please tag-name to v${version} format ([fb146bf](https://github.com/Chenggou1/fishword/commit/fb146bf9cf129dc932596b6019b45842ad04cab7))

## [0.1.1](https://github.com/Chenggou1/fishword/compare/v0.1.0...v0.1.1) (2026-06-11)


### Features

* **m0:** add pi extension probe for feasibility validation ([8e23003](https://github.com/Chenggou1/fishword/commit/8e2300313a290b441d268ebe6c26d2c22d233230))
* **m1:** implement core data models, SQLite storage, and CLI ([9fceb95](https://github.com/Chenggou1/fishword/commit/9fceb9543c3d250a898e04175ef340cb053ae37d))


### Bug Fixes

* add postinstall chmod for Unix platform binary packages ([2c3e6cd](https://github.com/Chenggou1/fishword/commit/2c3e6cd1256ab3ddd45fac8ae62eef4c517a2323))
* remove redundant .into_iter() call flagged by clippy ([5f6a45e](https://github.com/Chenggou1/fishword/commit/5f6a45e96cf291843a012439a2bf6eae9eee1139))
* run prepare-assets before building pi-extension in CI ([d36d43d](https://github.com/Chenggou1/fishword/commit/d36d43d11c85c01775d8a3f9268ea6377962638b))
* use pnpm publish to correctly rewrite workspace:* on release ([4c1d06f](https://github.com/Chenggou1/fishword/commit/4c1d06f93c96adc861caf485bb5a8653b43e8b47))

## [0.1.0](https://github.com/Chenggou1/fishword/releases/tag/v0.1.0) (2026-06-11)

首个公开发布版本。

### 核心功能

- **FSRS 调度引擎**：基于 FSRS v5 算法，根据每次评分（again / hard / good / easy）动态调整复习间隔，每个词库的算法状态独立维护
- **本地 SQLite 存储**：全部数据存储在本机，无需联网，无账号

### 词库管理

- `deck create / delete / rename`：完整的词库 CRUD，以数字 ID 为主键
- `deck use`：激活词库，后续复习命令默认作用于当前激活词库
- `deck list`：列出所有词库及今日进度

### 词汇导入

支持四种格式：
- **JSONL**（含例句，推荐）
- **CSV**
- **Anki TSV**
- **Qwerty Learner JSON**

内置三个考纲词库（首次启动 Pi extension 时自动导入）：

| 词库 | 词条数 |
|------|--------|
| CET-4 | 4,544 |
| CET-6 | 3,992 |
| TOEFL | 10,377 |

### 复习流程

- `current`：获取当前待复习词卡（含例句、释义）
- `rate <again|hard|good|easy>`：评分并推进到下一张
- `status`：当前词库的今日进度（新词 / 复习 / 剩余）
- `stats`：7 日学习趋势

### Pi 编程助手集成（`@fishword/pi-extension`）

- 首次启动自动初始化三个内置词库，无需任何配置
- 词卡 overlay：`Ctrl+Shift+V` 显示当前词卡，快捷键一键评分
- 统计 overlay：`/fw-stats` 查看今日进度和 7 日趋势
- 词库选择器：`/fw-deck` 交互式切换激活词库
- 完成今日任务后展示鼓励信息

### 发布与分发

- 多平台预编译二进制（macOS arm64/x64、Linux arm64/x64、Windows x64）
- npm 包：`@fishword/cli`、`@fishword/pi-extension` 及五个平台包
- GitHub Actions release CI：tag 触发，自动编译并发布到 npm
