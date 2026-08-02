# @fishword/pi-extension

Fishword 的 Pi 编程助手扩展，在编程时内嵌间隔重复词汇学习。

[GitHub 仓库](https://github.com/Chenggou1/fishword) · [产品展示页](https://chenggou1.github.io/fishword/)

## 安装

```
pi install npm:@fishword/pi-extension
```

重启 Pi 后会通过 Fishword catalog 自动下载 CET-4 / CET-6 / TOEFL 三个默认词库，无需手动导入词表。

## 快捷键

先按 `Ctrl+Q`，松开后再按操作键：

| 连续快捷键 | 功能 |
|------------|------|
| `Ctrl+Q F` | 隐藏或唤起 Fishword UI |
| `Ctrl+Q I` | 打开详情面板（音标、词性、释义、例句） |
| `Ctrl+Q G` | 评分：good（记住了） |
| `Ctrl+Q H` | 评分：hard（有点难） |
| `Ctrl+Q A` | 评分：again（没记住） |
| `Ctrl+Q E` | 评分：easy（轻松） |

按下 `Ctrl+Q` 后，状态栏会显示可用操作键；1.5 秒内没有继续输入会自动取消。评分快捷键在词卡视图和详情面板内均有效。详情面板内还额外支持 `G` / `H` / `A` / `E` 单键评分。

## Slash 命令

| 命令 | 功能 |
|------|------|
| `/fw` | 隐藏或唤起 Fishword UI |
| `/fw-detail` | 打开当前单词的详情面板 |
| `/fw-stats` | 查看今日进度和 7 日学习趋势 |
| `/fw-manage` | 词库管理：下载远程词库、切换或删除本地词库 |
| `/fw-good` | 评分：good（记住了） |
| `/fw-hard` | 评分：hard（有点难） |
| `/fw-again` | 评分：again（没记住） |
| `/fw-easy` | 评分：easy（轻松） |

## 许可证

GPL-3.0-only
