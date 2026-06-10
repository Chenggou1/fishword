# Fishword

Fishword 是一个本地词汇学习项目。当前核心是 Rust CLI，负责词库导入、SQLite 存储、FSRS 复习调度和稳定 JSON 协议。

## 文档

- 当前开发流程：[docs/development.md](./docs/development.md)
- 用户使用说明草案：[docs/usage.md](./docs/usage.md)


## 开发环境

本项目使用 pnpm 管理 JS/Pi workspace

安装 workspace：

```bash
pnpm install
```

## CLI 开发流程

日常开发 Rust CLI 时，先构建 debug 版本：

```bash
pnpm dev:cli
```

这会生成：

```text
target/debug/fishword
```

然后跑 CLI 冒烟测试：

```bash
pnpm smoke:cli
```

`smoke:cli` 会使用临时 `HOME`，不会污染你的真实数据库。当前覆盖：

```text
fishword init
fishword import qwerty
fishword deck current
fishword deck use <deck>
fishword current --json
fishword next --json
fishword rate good --json
```

也可以直接用 Cargo 手动调试：

```bash
cargo run -p fishword-cli -- init
cargo run -p fishword-cli -- current --json
cargo run -p fishword-cli -- next --json
cargo run -p fishword-cli -- rate good --json
```

手动测试时建议使用隔离 `HOME`：

```bash
HOME=/private/tmp/fishword-dev cargo run -p fishword-cli -- init
HOME=/private/tmp/fishword-dev cargo run -p fishword-cli -- current --json
```

## 测试与检查

Rust 测试：

```bash
pnpm test:rust
```

完整本地检查：

```bash
pnpm check
```

`pnpm check` 当前会执行：

```text
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/smoke-cli.mjs
```

如果只想单独跑某一项：

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/smoke-cli.mjs
```

## 本机部署测试

如果想像普通用户一样在任意目录执行 `fishword`，可以把本地 `@fishword/cli` wrapper link 到全局。

先构建 CLI：

```bash
pnpm dev:cli
```

再全局 link：

```bash
cd packages/cli
pnpm link --global
cd ../..
```

验证：

```bash
fishword --version
fishword --help
```

用隔离数据库做完整手动测试：

```bash
HOME=/private/tmp/fishword-dev fishword init
HOME=/private/tmp/fishword-dev fishword import qwerty assets/dicts/qwerty-learner/dicts/CET4_T.json --deck cet4 --name "CET-4"
HOME=/private/tmp/fishword-dev fishword deck use cet4
HOME=/private/tmp/fishword-dev fishword current --json
HOME=/private/tmp/fishword-dev fishword next --json
HOME=/private/tmp/fishword-dev fishword rate good --json
```

这个全局 `fishword` 实际是 JS wrapper。开发模式下它会解析到当前仓库的：

```text
target/debug/fishword
```

并把仓库内的默认词库目录传给 Rust CLI。发布到 npm 时，`@fishword/cli`
包会内置同一份词库资产。用户安装 CLI 后执行 `fishword init` 就会创建本地
数据库并自动导入默认词库，不需要再单独下载或手动 import。

所以每次修改 Rust 后，重新执行：

```bash
pnpm dev:cli
```

全局命令就会使用最新的 debug binary。

## 默认词库

本仓库内置了来自 [Qwerty Learner](https://github.com/RealKai42/qwerty-learner) 的默认词库。

- 本地路径：`assets/dicts/qwerty-learner/dicts/`
- 上游仓库：`https://github.com/RealKai42/qwerty-learner`
- 上游目录：`public/dicts/`
- 导入的上游 commit：`2498f753aaf955645f466664d3972c2c7d29dd55`
- 词库数量：380 个 JSON 文件
- 上游许可证：GPL-3.0
- 内置许可证副本：`assets/dicts/qwerty-learner/upstream/LICENSE`

这些词库按上游 GPL-3.0 许可证再分发。如果分发包含这些词库的 Fishword，请遵守 GPL-3.0，并保留 attribution 和 license notice。

`fishword init` 默认导入以下内置词库：

- `cet4`：CET-4
- `cet6`：CET-6
- `toefl`：TOEFL

如果只想创建空数据库，可以执行：

```bash
fishword init --skip-default-decks
```

也可以继续手动导入其它词库：

```bash
fishword import qwerty assets/dicts/qwerty-learner/dicts/CET4_T.json --deck cet4 --name "CET-4"
fishword import qwerty assets/dicts/qwerty-learner/dicts/CET6_T.json --deck cet6 --name "CET-6"
fishword import qwerty assets/dicts/qwerty-learner/dicts/TOEFL_3_T.json --deck toefl --name "TOEFL"
```

查看导入结果：

```bash
fishword deck list
fishword card list --deck cet4
```

选择当前学习词库：

```bash
fishword deck use cet4
fishword deck current
```
