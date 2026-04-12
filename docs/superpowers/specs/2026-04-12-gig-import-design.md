# `gig import` — 导入已有项目到 gig 数据库

**状态:** Approved
**日期:** 2026-04-12
**依赖:** gig v0.1 MVP (已实现)

## 0. 背景

用户在使用 `gig` 之前已经积累了大量接单项目,分散在 `~/dev/`(活跃)和 `~/Documents/archive/work/`(归档)中。需要一个 `gig import` 子命令把这些已有项目录入数据库,让它们进入 gig 的管理体系。

**非目标:**
- 不自动扫描目录树发现项目 —— 用户显式指定路径
- 不创建新的数据库表 —— 复用现有 `orders`、`clients` 表
- 不修改已有命令的行为

---

## 1. 命令签名

```
gig import [<path>...] [--interactive] [--status <s>] [--relocate] [--dry-run]
```

| 参数/flag | 类型 | 说明 |
|---|---|---|
| `<path>...` | 位置参数,可多个 | 要导入的目录路径。省略时 = 当前目录(`.`) |
| `--interactive` / `-i` | bool | 逐项交互式填写/确认元数据 |
| `--status <s>` | 可选 string | 强制覆盖所有导入项的状态(如 `archived`、`in_progress`) |
| `--relocate` | bool | 导入后 mv 目录到 `dev_root` 或 `archive_root`(按状态),按 slug 重命名 |
| `--dry-run` | bool | 只打印"会做什么",不写数据库也不动文件 |

---

## 2. 导入流程(每个路径)

### 2.1 验证

1. **路径存在且为目录** —— 否则报错跳过
2. **是否是 git 仓库** —— 检查 `<path>/.git` 是否存在。不是 git 仓库时**警告**但不阻塞(有些旧项目可能没 git init)
3. **防重复** —— 查询 `SELECT id FROM orders WHERE dev_path = ?1 OR archive_path = ?1`(用 canonicalize 后的路径)。已存在则打印 `skipped: already imported as #42` 并跳过
4. **当前目录模式额外检查**（不传路径时 = `gig import .`）:
   - 当前目录不能是 `dev_root` 或 `archive_root` 本身（防止把整个 dev 目录当一个项目导入）
   - 当前目录如果不是 git 仓库根（无 `.git/`），显示 `not a git root, are you sure? [y/N]` 确认

### 2.2 元数据推断（静默模式）

| 字段 | 推断逻辑 |
|---|---|
| `slug` | 目录名 strip 掉日期前缀后,lowercase,非 `[a-z0-9-]` 的字符替换为 `-` |
| `title` | slug 里的 `-` 替换为空格,每个词首字母大写 |
| `created_at` | 优先级:目录名日期前缀 > git 第一次 commit 时间 > 目录 ctime。转为 unix seconds |
| `status` | 见 §2.3 |
| `dev_path` | 当 status 不是 `archived` 时,`canonicalize(path)` |
| `archive_path` | 当 status 是 `archived` 时,`canonicalize(path)` |
| `my_cut_ratio` | 来自 config 默认值 |
| `currency` | 来自 config 默认值 |
| 其余 | `NULL`(price, client, notes, external_id, tags 等) |

### 2.3 状态推断规则

优先级从高到低:

| 条件 | 推断状态 |
|---|---|
| 用户传了 `--status <s>` | 使用用户指定的值 |
| 路径在 `archive_root` 下(canonical 前缀匹配) | `archived` |
| 路径在 `dev_root` 下,且有 git commits | `in_progress` |
| 路径在 `dev_root` 下,无 git history | `accepted` |
| 其他路径 | `in_progress`(保守默认) |

状态推断使用 `gig_core::context::is_within` 做路径前缀判断,复用已有代码。

### 2.4 日期前缀解析

从目录名开头提取日期,剩余部分作为 slug 原材料:

```
"2026-02-06_steam-game-review-scraper"  → date=2026-02-06, rest="steam-game-review-scraper"
"04-08-human-action-live-recognization" → date=<当年>-04-08, rest="human-action-live-recognization"
"GingiCasc"                             → date=None,        rest="gingicasc"
```

正则模式(按顺序尝试):
1. `^(\d{4}-\d{2}-\d{2})[_-](.+)$` — 完整日期 `YYYY-MM-DD`
2. `^(\d{2}-\d{2})[_-](.+)$` — 短日期 `MM-DD`,补当前年份
3. 均不匹配 → `date = None`,`rest = 目录名全部`

### 2.5 交互式覆盖（仅 `--interactive`）

显示推断结果,逐项确认。用户按回车保留推断值,输入新值覆盖:

```
importing: ~/dev/04-08-human-action-live-recognization
  slug         [human-action-live-recognization]: 
  title        [Human Action Live Recognization]: Human Action Live Recognition
  status       [in_progress]: 
  quoted_price [—]: 50000
  final_price  [—]: 50000
  client       [—]: Acme Corp
  source_org   [—]: 
  notes        [—]: opencv 实时动作识别
  tags         [—]: python,opencv
```

可填字段:slug, title, status, quoted_price, final_price, client (display_name → find_or_create), source_org, notes, tags (逗号分隔)。

### 2.6 写入

1. 如果 `--interactive` 时填了 `client` → `repo::clients::find_or_create(conn, display_name)` 获取 `client_id`。需要新增这个 repo 函数(目前只有 `insert`)。
2. `repo::orders::insert(conn, &new_order)` 写入订单
3. 如果 `--interactive` 时填了 tags → `services::lifecycle::add_tags(conn, order_id, tags)`
4. 如果 `--relocate`:
   - 计算目标路径:`<dev_root>/<slug>` 或 `<archive_root>/<slug>`(按 status）
   - 目标已存在则报错跳过
   - `std::fs::rename(old, new)`
   - `canonicalize` 新路径,更新 `dev_path` / `archive_path`
5. 填入对应时间戳:
   - `accepted_at` = `created_at`(对 `accepted` / `in_progress` / `delivered` / `paid` / `archived`）
   - `archived_at` = `created_at`（仅对 `archived`）

### 2.7 输出

每个成功导入打印一行:
```
imported #42  human-action-live-recognization  (in_progress)  ~/dev/04-08-human-action-live-recognization
```

`--dry-run` 模式打印:
```
[dry-run] would import  human-action-live-recognization  (in_progress)  ~/dev/04-08-human-action-live-recognization
```

最后汇总:
```
imported 5 orders (skipped 2 duplicates)
```

---

## 3. 文件结构

| 新增/修改 | 位置 | 说明 |
|---|---|---|
| 新增 | `crates/gig-core/src/services/import.rs` | 导入逻辑:验证、推断、写入 |
| 修改 | `crates/gig-core/src/services/mod.rs` | `pub mod import;` |
| 修改 | `crates/gig-core/src/repo/clients.rs` | 新增 `find_by_name` 和 `find_or_create` |
| 新增 | `crates/gig-cli/src/commands/import.rs` | CLI 参数解析、交互向导、输出 |
| 修改 | `crates/gig-cli/src/commands/mod.rs` | `pub mod import;` |
| 修改 | `crates/gig-cli/src/cli.rs` | 新增 `Import(ImportArgs)` |
| 修改 | `crates/gig-cli/src/dispatch.rs` | 路由 `Import` |

**不需要**新的数据库迁移或表。

---

## 4. 边界情况

| 情况 | 处理 |
|---|---|
| slug 冲突（DB 中已有相同 slug 的订单） | slug 后追加 `-2`、`-3`... 直到唯一 |
| 目录名全是数字或特殊字符 | slugify 后如果为空,用 `import-<id>` 作为 fallback |
| `--relocate` 目标已存在 | 报错 "target path already exists: ...",跳过该项 |
| 非 UTF-8 目录名 | 用 `to_string_lossy`,slug 中的非 ASCII 替换为 `-` |
| 权限不足无法读取目录 | 报错跳过,继续下一个 |
| 空目录（无文件） | 正常导入,无特殊处理 |
| 符号链接目录 | `canonicalize` 后按真实路径处理 |

---

## 5. 使用示例

```bash
# 批量导入归档项目
gig import ~/Documents/archive/work/* --status archived

# 交互式导入两个活跃项目
gig import ~/dev/04-08-human-action-live-recognization ~/dev/04-11-excel-equation -i

# 在当前项目目录内导入
cd ~/dev/04-08-human-action-live-recognization
gig import

# 预览
gig import ~/dev/* --dry-run

# 导入并搬到标准位置
gig import ~/Documents/archive/work/2026-02-06_steam-game-review-scraper --status archived --relocate
```
