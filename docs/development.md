# 开发指南

## 环境要求

| 工具 | 最低版本 | 说明 |
|------|----------|------|
| Rust | 1.80+ | `rustup` 安装 |
| Node.js | 18+ | pnpm 和 Pi extension 依赖 |
| pnpm | 9.x | JS 包管理 |

### 安装 Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### 安装 pnpm

```bash
corepack enable
corepack prepare pnpm@9.15.4 --activate
```

---

## 首次克隆后的初始化

```bash
git clone https://github.com/Chenggou1/fishword.git
cd fishword

# 安装 JS 依赖（Pi extension 等）
pnpm install

# 编译 Rust CLI（debug 模式）
pnpm dev:cli
```

---

## 安装到本机测试

最简单的方式是用 `cargo install`，直接把 CLI 装进 `~/.cargo/bin/`：

```bash
cargo install --path crates/fishword-cli
```

装完后就能直接用 `fishword` 命令：

```bash
fishword init
fishword import qwerty crates/fishword-core/fixtures/qwerty_cet4_sample.json --deck cet4 --name CET-4
fishword current
fishword next
fishword rate good
```

卸载：

```bash
cargo uninstall fishword-cli
```

### 数据库位置

`fishword init` 会在系统默认数据目录创建数据库：

- macOS：`~/Library/Application Support/fishword/fishword.db`
- Linux：`~/.local/share/fishword/fishword.db`

默认情况下，`fishword init` 还会从内置 Qwerty Learner 资产导入 `cet4`、
`cet6`、`toefl` 三个默认词库。本地开发如果尚未拉取 Git LFS 内容，可以先执行
`git lfs pull`，或者用 `fishword init --skip-default-decks` 创建空数据库。

---

## 开发调试循环

**不想装到全局**，用 `cargo run` 直接跑：

```bash
cargo run -p fishword-cli -- init
cargo run -p fishword-cli -- current --json
cargo run -p fishword-cli -- next --json
cargo run -p fishword-cli -- rate good --json
```

**用隔离的临时 HOME**，避免污染本机数据：

```bash
export FW_HOME=/tmp/fishword-dev

HOME=$FW_HOME cargo run -p fishword-cli -- init
HOME=$FW_HOME cargo run -p fishword-cli -- import qwerty \
  crates/fishword-core/fixtures/qwerty_cet4_sample.json --deck cet4 --name CET-4
HOME=$FW_HOME cargo run -p fishword-cli -- current --json
HOME=$FW_HOME cargo run -p fishword-cli -- rate good --json
```

---

## 测试

```bash
# 运行所有 Rust 单元测试
pnpm test:rust

# 端到端冒烟测试（先编译再跑）
pnpm dev:cli && pnpm smoke:cli

# 全量检查（格式 + lint + 测试 + 冒烟）
pnpm check
```

冒烟测试覆盖完整链路：`init → import → current → next → rate`，使用独立临时 HOME，不影响本机数据。

---

## Pi Extension 本地开发

```bash
# 1. 编译最新 CLI
pnpm dev:cli

# 2. 用 -e 临时加载 extension（不需要安装）
pi -e ./packages/pi-extension/src/index.ts
```

extension 通过 `@fishword/cli` 找到 CLI 二进制，开发时会优先使用 `target/debug/fishword`，无需配置路径。

---

## 常用命令速查

```bash
pnpm dev:cli        # 编译 debug CLI
pnpm test:rust      # Rust 单元测试
pnpm smoke:cli      # 端到端冒烟测试
pnpm check          # 全量检查
cargo test          # 同 test:rust（直接用 cargo 也行）
```
