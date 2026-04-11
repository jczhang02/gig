# `gig` — 个人接单 workflow 工具设计

**状态:** Draft (brainstorming 产出)
**日期:** 2026-04-11
**背景文档:** `README.md`(个人接单 workflow 描述)

## 0. 背景与目标

作者目前以高频(每月 10+ 单)方式在多个企业微信企业中接单做开发,现有 workflow 覆盖"沟通前 → 沟通 → 开始开发 → 开发过程 → 交付 → 交付后"六个阶段。每个阶段都存在手动、易出错、无记录的痛点,但企业微信侧**没有任何自动化可能性**,所以一切自动化都发生在本机。

`gig` 的目标是:用一个本地 CLI 把整个 workflow 的元数据沉淀下来,在不依赖任何 SaaS 的前提下消除重复劳动,并为以后的桌面端(看板、图表、复盘)预留架构。

**非目标:**
- 不尝试自动化企业微信侧的任何事(抓取、发送、机器人等)
- 不做多人协作、团队版、云同步 —— 这是单人工具
- 不绑定任何一家云盘服务

---

## 1. 工具命名与命令哲学

**名字:** `gig`
- 英语俚语即"零工/接的活",语义零歧义
- 3 字母,双手交替,输入成本最低
- 主流 CLI 生态无名字冲突

**命令风格:** 扁平动词 + 少量 domain 子命名空间(git 风)
- 95% 的操作对象是"订单",所以订单动词全部提到顶层:`gig new`、`gig deliver 42`、`gig paid 42`
- 辅助资源走子命名空间:`gig lead <verb>`、`gig client <verb>`、`gig template <verb>`、`gig config <verb>`
- ID 默认用自增整数(`gig show 42`),也支持 slug(`gig show acme-scraper`)

---

## 2. 项目结构(Rust workspace)

```
part-time/                         # 仓库根(即本文档所在目录)
├── Cargo.toml                    # workspace manifest
├── crates/
│   ├── gig-core/                 # 纯业务逻辑,无 CLI/UI 耦合
│   │   ├── src/
│   │   │   ├── db/               # rusqlite 连接 + refinery 迁移
│   │   │   ├── models/           # Order, Client, Change, Delivery, Tag
│   │   │   ├── services/         # 用例层:下单、改价、交付、归档、统计
│   │   │   ├── templates/        # minijinja 模板渲染
│   │   │   ├── delivery/         # Uploader trait + rclone/s3/http 后端
│   │   │   └── config.rs         # XDG 路径解析 + config.toml 反序列化
│   │   └── Cargo.toml
│   ├── gig-cli/                  # 发布二进制 `gig`
│   │   ├── src/
│   │   │   ├── main.rs           # clap Parser 入口
│   │   │   ├── commands/         # 每个子命令一个文件,薄壳调 core
│   │   │   └── ui.rs             # 表格/颜色输出
│   │   └── Cargo.toml
│   └── gig-desktop/              # 占位:未来 Tauri 壳
├── migrations/                   # refinery SQL 迁移
├── templates/                    # 默认模板(随二进制打包或首次运行时释放)
│   ├── project-readme.md.j2
│   ├── quote-reply.md.j2
│   └── delivery-checklist.md.j2
├── docs/
└── README.md
```

**核心纪律:**`gig-cli` crate 只做 "解析参数 → 调 `gig_core::services::*` → 格式化输出"。所有业务逻辑都在 `gig-core`,这样未来 `gig-desktop` crate `use gig_core::services::*` 就能复用 100% 的核心代码。(workspace 目录名 `part-time/` 保留,因为这是已有仓库路径;crate 的 Cargo 名字和发布名是 `gig-*`。)

**主要依赖:**
- `rusqlite` + `refinery` —— 数据库 + 迁移
- `clap` v4 derive —— CLI 解析
- `serde` + `toml` —— 配置
- `comfy-table` + `owo-colors` —— 表格 / 颜色
- `minijinja` —— 模板
- `time` —— 时间戳
- `thiserror` —— 错误类型
- `directories` —— XDG 路径
- `arboard` —— 剪贴板(交付后复制链接)
- 外部二进制:`rclone`(交付上传)、`tar`/`zstd`(打包)

---

## 3. 目录与路径(严格遵守 XDG Base Directory)

| 用途 | 路径(带默认展开) |
|---|---|
| 数据库 | `${XDG_DATA_HOME:-$HOME/.local/share}/gig/gig.db` |
| 配置 | `${XDG_CONFIG_HOME:-$HOME/.config}/gig/config.toml` |
| 状态 / 日志 / 备份快照 | `${XDG_STATE_HOME:-$HOME/.local/state}/gig/` |
| 用户模板(可编辑) | `${XDG_DATA_HOME:-$HOME/.local/share}/gig/templates/` |

**config.toml 关键字段:**
```toml
[general]
dev_root = "~/dev"                      # gig init 在此下创建项目文件夹
archive_root = "~/Documents/gig-archive" # gig archive 的目的地
default_cut_ratio = 0.60                # 到手比例
default_currency = "CNY"
editor = "$EDITOR"

[delivery]
default_uploader = "rclone:r2"

[delivery.uploader."rclone:r2"]
kind = "rclone"
remote = "r2:gig-delivery"
link_ttl_days = 7

[delivery.uploader."s3:aliyun-hk"]
kind = "s3"
endpoint = "oss-cn-hongkong.aliyuncs.com"
bucket = "gig-delivery"
# credentials from env: AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY
```

---

## 4. 数据模型

### 4.1 实体

**`orders`** — 订单主表
| 列 | 类型 | 说明 |
|---|---|---|
| `id` | INTEGER PK AUTOINCREMENT | 对用户暴露的短 ID |
| `slug` | TEXT UNIQUE NULL | 文件夹名,如 `acme-scraper`;lead 阶段可为 NULL,进入 accepted 或 `gig init` 前必须有值 |
| `external_id` | TEXT NULL | 店主给的订单编号 |
| `title` | TEXT NOT NULL | 一句话标题 |
| `client_id` | INTEGER FK → clients.id | |
| `source_org` | TEXT | 来自哪个企业微信企业/群 |
| `status` | TEXT NOT NULL | 见 §4.2 状态机 |
| `quoted_price` | INTEGER | 以最小货币单位(分)存储,避免浮点 |
| `final_price` | INTEGER | |
| `my_cut_ratio` | REAL NOT NULL | 默认来自 config |
| `my_cut_amount` | INTEGER GENERATED | `final_price * my_cut_ratio` |
| `currency` | TEXT NOT NULL | ISO 4217,默认来自 config |
| `dev_path` | TEXT | `gig init` 后的绝对路径 |
| `archive_path` | TEXT | `gig archive` 后的绝对路径 |
| `notes` | TEXT | 自由备注 |
| `created_at` | INTEGER | unix seconds,以下同 |
| `accepted_at` | INTEGER NULL | 进入 accepted 时填 |
| `delivered_at` | INTEGER NULL | |
| `paid_at` | INTEGER NULL | |
| `archived_at` | INTEGER NULL | |

**`clients`**
`id`、`display_name`、`wechat_contact`、`source_org`、`first_seen_at`、`notes`。支持 `gig client merge` 合并重复录入。

**`requirement_changes`** — 开发中的每一次需求变更
`id`、`order_id`、`description`、`price_delta`(可正可负可零)、`created_at`。

**`price_history`** — 每次改价都留痕
`id`、`order_id`、`old_price`、`new_price`、`reason`、`created_at`。

**`delivery_artifacts`** — 交付物登记
`id`、`order_id`、`local_path`、`uploader_name`、`remote_url`、`expires_at`、`uploaded_at`。

**`tags`** / **`order_tags`** — 标签多对多(python / web / 爬虫 / 数据分析 / ...),用于按品类统计。

### 4.2 订单状态机

```
        ┌─────────┐
        │  lead   │  潜在单(看到群消息 → 考虑接)
        └────┬────┘
             │ promote
             ▼
      ┌─────────────┐
      │ negotiating │  正在和客户对需求/报价
      └──────┬──────┘
             │ accept
             ▼
      ┌──────────┐
      │ accepted │  确认接单,未开始
      └─────┬────┘
            │ init
            ▼
     ┌─────────────┐      change_requested
     │ in_progress │◄────────┐
     └──────┬──────┘         │
            │                │
            ├────────────────┘  (循环,每次记录到 requirement_changes)
            │
            ▼
      ┌───────────┐
      │ delivered │  已交付,待付款
      └─────┬─────┘
            ▼
         ┌──────┐
         │ paid │  已付款
         └──┬───┘
            ▼
       ┌──────────┐
       │ archived │  代码已移到归档目录
       └──────────┘

任何状态 ──cancel──▶ cancelled(终态)
```

所有状态转移都走 `gig_core::services::orders::transition`,该函数负责:(1) 校验合法性;(2) 回填对应时间戳;(3) 写入审计日志。

---

## 5. 命令行表面

**约定:**
- `<id>` 可以是整数 ID 或 slug。
- 当在某个订单的开发目录(或其子目录)内运行时,`<id>` **可省略**,自动解析为当前目录所属的订单(见 §5.7 上下文感知)。
- 以 ★ 标记的是高频命令。

### 5.1 订单动词(顶层)

```
gig new [--lead]           ★ 新建订单;默认 accepted;--lead 进入 lead 状态
gig ls                     ★ 看板:按状态分组;默认显示"今日应关注"视图
gig show <id>                详情卡片:元数据 + 变更 + 价格历史 + 交付物
gig init <id>              ★ 脚手架开发文件夹(见 §5.4)
gig status <id> <status>     强制改状态(通常不用,交给动词命令推进)
gig price <id> <amt> -m …    改价,要求填原因,写入 price_history
gig cut <id> <ratio>         改到手比例
gig change <id> -m … [--delta AMOUNT]
                             记录一次需求变更,可带 price_delta
gig note <id> "…"            追加备注
gig tag <id> <tag...>        打标签
gig pack <id>                按规则打包,输出到 $TMPDIR
gig deliver <id> [--uploader N] [--resend]
                           ★ 打包 + 上传 + 生成链接 + 写入 delivery_artifacts
                             + 状态 → delivered + 链接复制到剪贴板
gig paid <id> [--on DATE]    标记已付款
gig archive <id>           ★ 把 dev_path 下文件夹 mv 到 archive_root
gig cd <id>                  输出 dev_path,配合 shell 函数做 cd
gig stats [--range R] [--by tag|client]
                             统计:本月进账、未收、订单数、平均客单、漏斗
gig export csv|json [filter] 导出
```

### 5.2 辅助子命名空间

```
gig lead promote <id>        lead → negotiating,触发客户录入向导
gig lead drop <id>           lead → cancelled
gig lead ls                  同 `gig ls --status lead`

gig client ls
gig client show <id>
gig client merge <a> <b>     把 b 合并到 a(处理同一客户重复录入)

gig template ls
gig template edit <name>     用 $EDITOR 打开
gig template show <name>

gig config get <key>
gig config set <key> <value>
gig config edit              用 $EDITOR 打开 config.toml
```

### 5.3 维护命令

```
gig backup                   把 gig.db 快照到 $XDG_STATE/gig/backups/,保留最近 30 份
gig doctor                   健康检查:
                             - dev_path / archive_path 是否还存在
                             - 数据库一致性(迁移版本、外键、孤儿记录)
                             - 未同步状态(如 delivered_at 为 NULL 但 status=delivered)
```

### 5.4 `gig init <id>` 细节

1. 读取订单,确认状态 ∈ {accepted}。若订单没有 slug(来自 `--lead` 创建后 promote 的情况),从标题生成建议 slug(ASCII 化 + 连字符化),交互式确认或 `--slug` 指定;写回数据库。
2. 计算目标路径:`<dev_root>/<slug>/`。若已存在则失败并提示。
3. 创建目录;`git init`;写 `.gitignore`(含禁止 push GitHub 的警示行)。
4. 渲染 `templates/project-readme.md.j2`,注入订单元数据(客户、需求摘要、报价、到手、截止日期、验收清单占位)。
5. 把 `dev_path` 回写到数据库,状态 → `in_progress`。
6. 调用 `$EDITOR` 打开 README。

### 5.5 `gig deliver <id>` 细节

1. 读取订单,确认状态 ∈ {in_progress}。
2. `pack` 生成归档(`tar.zst` 默认;可配置 `zip`)。**打包规则 = `.gitignore` + 附加规则:**
   - 基础:以项目根的 `.gitignore` 为准(用 `ignore` crate 解析,和 `git` / `ripgrep` 行为一致),等价于"只打包 git 认识的文件"。
   - 无论 `.gitignore` 怎么写,`.git/` 目录本身永远排除。
   - 附加规则可以来自两处,优先级从低到高:
     - 全局 config.toml 里 `[pack] extra_ignore = [...]`(所有项目生效)
     - 项目根的 `.gigignore` 文件(当前项目独有;语法同 gitignore)
   - 附加规则既能排除(`*.env`)也能用 `!` 反向包含(比如 `!README.md` 强制打进即便被 gitignore 忽略 —— 很少用,但保留能力)。
   - `gig pack <id> --dry-run` 会打印最终要打包的文件列表,交付前可核对。
3. 读取 `config.delivery.default_uploader`(或 `--uploader` 覆盖),调对应 `Uploader::upload`。
4. 获得 `UploadResult { url, expires_at, provider }`,写入 `delivery_artifacts`。
5. 状态 → `delivered`,填 `delivered_at`。
6. 把 `url` 复制到剪贴板(通过 `arboard`),并在终端打印。
7. `--resend` 模式:跳过 pack,重新上传最近一次的本地归档(或重新打包,视参数);生成新链接。

### 5.6 `gig ls` 默认视图("今日应关注")

当不带过滤器时输出一个有主次的看板:

```
今日应关注
──────────────────────────────────────────
  #42  acme-scraper      delivered  ¥1200  已交付 3 天未收款  ← 高亮
  #39  bar-admin-panel   in_progress ¥3000  已 5 天无更新      ← 提醒
  #45  foo-datavis       lead       ¥??    已登记 2 天无进展   ← 提醒

其他进行中:#41, #43, #44
本月进账:¥8,400 / 已成单 7 / 漏斗 12→7
```

规则由 `gig_core::services::dashboard` 计算,阈值可配置。

### 5.7 上下文感知(项目目录内默认当前订单)

**[CRITICAL]** 这是整个 CLI 最不能出错的子系统。错一次就会"操作错单子",后果严重。以下规则是实现纪律,不得违反。

灵感来自 `git`:在项目目录内运行时,`gig` 自动识别当前订单,无需每次输入 `<id>`。

#### 5.7.1 不变式(invariants)

所有路径相关的代码都必须遵守:

1. **所有写入数据库的 `dev_path` / `archive_path` 都必须是 canonical 形式:**
   `std::fs::canonicalize(path)?` 成功后得到的绝对路径、无尾斜杠、解完所有 symlink。
2. **所有用于匹配的 CWD 都必须经过同一个 canonical 化函数。**
   两端对称 —— 这是正确性的核心。
3. **Canonicalize 必须发生在 I/O 可以成功的时刻。**
   若目录不存在(归档后 / 用户 mv 走),`canonicalize` 会失败,此时**不得**用未规范化的 fallback,必须返回 "context unavailable"。
4. **路径比较只使用字节级相等或 "prefix + 分隔符" 测试。**
   不得用 `starts_with` 字符串裸比较(会产生 `/home/jc/dev/acme` 命中 `/home/jc/dev/acme-scraper` 的假阳性)。

统一封装在一个函数里,禁止在其他地方手写路径比较:

```rust
// gig_core::context
pub fn canonical(p: impl AsRef<Path>) -> Result<PathBuf> {
    std::fs::canonicalize(p).map_err(Error::from)
}

/// True iff `haystack` is `needle` or a descendant directory of `needle`.
/// Both inputs MUST already be canonical.
pub fn is_within(haystack: &Path, needle: &Path) -> bool {
    if haystack == needle { return true; }
    // Walk component-by-component instead of string prefix — immune to
    // "foo" vs "foo-bar" false positives.
    let mut h = haystack.components();
    let mut n = needle.components();
    loop {
        match (h.next(), n.next()) {
            (Some(a), Some(b)) if a == b => continue,
            (_, Some(_)) => return false,   // needle still has components → not a prefix
            (Some(_), None) => return true, // needle exhausted, haystack still has more → descendant
            (None, None) => return true,    // exact equality (already handled above, but safe)
        }
    }
}
```

#### 5.7.2 解析算法

```rust
pub fn resolve_context(conn: &Connection) -> Result<Option<i64>> {
    let cwd = canonical(std::env::current_dir()?)?;

    // Pull all candidate rows (N is small — hundreds at most).
    // Do the component-wise prefix test in Rust rather than SQL LIKE,
    // because LIKE cannot express component-boundary matching correctly.
    let mut stmt = conn.prepare(
        "SELECT id, dev_path, archive_path FROM orders
         WHERE dev_path IS NOT NULL OR archive_path IS NOT NULL",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
        ))
    })?;

    let mut best: Option<(i64, usize)> = None; // (id, matched path length)
    for row in rows {
        let (id, dev, arch) = row?;
        for path_str in [dev, arch].into_iter().flatten() {
            let p = PathBuf::from(&path_str);
            if is_within(&cwd, &p) {
                let len = p.as_os_str().len();
                if best.map_or(true, |(_, best_len)| len > best_len) {
                    best = Some((id, len));
                }
            }
        }
    }
    Ok(best.map(|(id, _)| id))
}
```

**关键点:**
- **不用 SQL `LIKE`** 做前缀匹配 —— `LIKE` 无法表达"分隔符边界",会把 `/home/jc/dev/acme` 误匹配到 `/home/jc/dev/acme-scraper`。
- 用 **component-wise** 比较(`Path::components()`),跨平台正确。
- 候选集在 Rust 侧做 O(N) 线性选最长,N 是订单总数,几百级别,<1ms。
- 只有 `dev_path` / `archive_path` **当前仍是合法 canonical 路径** 时(能被 `canonicalize` 成功),才参与匹配。数据库里存的字符串不经过二次 canonicalize;但解析 CWD 时 `canonicalize` 任何一端失败直接返回 None。

#### 5.7.3 命令集成

`<id>` 参数在 CLI 层是 `Option<String>`;对每个启用上下文的命令:

```rust
fn resolve_id(cli_id: Option<String>, conn: &Connection) -> Result<i64> {
    if let Some(s) = cli_id {
        return resolve_id_or_slug(&s, conn);  // 显式优先
    }
    match resolve_context(conn)? {
        Some(id) => Ok(id),
        None => bail!(
            "not inside a gig project directory; pass <id> or cd into one.\n\
             hint: `gig cd <id>` prints the dev_path."
        ),
    }
}
```

**命令分类:**

| 命令 | 上下文行为 |
|---|---|
| `gig show` / `price` / `change` / `note` / `tag` / `status` / `cut` / `pack` / `deliver` / `paid` / `archive` / `cd` | `<id>` 省略时用上下文;无上下文则报错退出 |
| `gig init` | **要求显式 `<id>`**。init 是"第一次进入项目",此时还没有 dev_path,不可能有上下文 |
| `gig new` / `new --lead` | 永远创建新订单,**不**读上下文 |
| `gig ls` / `stats` / `export` / `backup` / `doctor` / `client *` / `template *` / `config *` | 与上下文无关,忽略 |
| `gig lead promote` / `drop` | 要求显式 `<id>`(安全考虑:避免误转状态) |

**强制覆盖:** 任何启用上下文的命令都接受 `--id <id>` 或位置参数来覆盖自动解析。

#### 5.7.4 显示与确认

对任何即将**写入数据库或改变文件系统**的上下文命令(`price` / `change` / `price` 之外的所有动词),CLI **必须先打印一行 banner** 明确告诉用户操作的是哪个订单:

```
$ gig deliver
→ order #42  acme-scraper  (in_progress, ¥1200)
packing... ✓
uploading to rclone:r2... ✓
link: https://r2.example.com/...  (copied to clipboard)
status: in_progress → delivered
```

对高风险动词(`archive` / `cancel` / `status` 强制改状态)默认加一步确认:
```
$ gig archive
→ order #42  acme-scraper  (delivered, ¥1200)
archive to /home/jc/Documents/gig-archive/acme-scraper? [y/N]
```
可用 `--yes` 跳过。

#### 5.7.5 为什么不用 marker 文件

- SQLite 是单一真源(§2 纪律);marker 会让"真源"分裂成两处
- marker 容易被误打包进交付(需要额外 `.gitignore`,多一个心智负担)
- 候选集查询 + 线性筛选在订单几百条时 <1ms,无感知
- 如果用户手动 `mv` 了文件夹,前缀匹配会自然失效 —— `gig doctor` 会发现数据库和实际不一致并提示修复,这是正确行为

#### 5.7.6 `gig doctor` 必做的一致性检查

- 遍历所有 `dev_path` / `archive_path`,`canonicalize` 一次;失败或和存储值不一致的标记为"drift"
- 对每条 drift 提示:路径当前是什么、建议操作(修正、清空、重新 init)
- 检测两条订单的 `dev_path` / `archive_path` 互为祖先的情况(嵌套),提示用户确认

#### 5.7.7 必须写的测试(在 v0.1 里就有,不允许省)

`gig_core::context` 的单元测试和集成测试,至少覆盖:

1. **精确等值**:CWD == dev_path,命中
2. **真子目录**:CWD 在 dev_path 下几层深,命中
3. **相似前缀名**:dev_path = `/tmp/acme`,CWD = `/tmp/acme-scraper`,**不**命中
4. **嵌套项目**:两条 dev_path,一条是另一条的祖先,CWD 在内层,命中**更深**那条
5. **项目目录外**:CWD 和任何 dev_path 都无关,返回 None
6. **symlink 一致性**:用 `tempfile` 创建 `/tmp/realdir` 和 `/tmp/link → realdir`,`gig init` 时用 link 路径,`cd` 到 link 路径下,`resolve_context` 应命中
7. **目录已被删除**:dev_path 在 DB 里但磁盘上已不存在,`canonicalize(cwd)` 成功但该条不参与匹配,不应 panic
8. **尾斜杠与否无影响**:存储路径 `/tmp/foo` 或 `/tmp/foo/` 应产出相同结果(通过 `PathBuf` 规范化即可)
9. **空候选集**:数据库无任何订单,返回 None

这些测试是 context 模块的正确性契约。**任何修改必须保持它们全部通过。**

#### 5.7.8 体验示例

```bash
$ cd ~/dev/acme-scraper
$ gig show            # 等价于 gig show 42
$ gig change -m "客户要求支持 SOCKS5 代理" --delta 200
$ gig deliver         # 直接交付当前项目
$ gig paid            # 标记当前项目已收款
$ gig archive         # 一键归档(会二次确认)
```

**和 `gig cd <id>` 的配合:**
`gig cd` 输出 dev_path,配合 shell 函数 `gcd() { cd "$(gig cd "$1")"; }`,实现 `gcd 42` 跳转到项目;进入后所有后续 `gig` 命令自动作用于该订单。

---

## 6. 交付云盘策略

**核心思路:** 不绑定任何一家云,在 `gig_core::delivery` 定义 `Uploader` trait,后端可插拔。README 里提到的"中国大陆 + 香港都快"的空洞,用"可替换后端"而不是"选定某家"来解决。

```rust
pub trait Uploader: Send + Sync {
    fn name(&self) -> &str;
    fn upload(&self, local: &Path, opts: &UploadOpts) -> Result<UploadResult>;
}

pub struct UploadResult {
    pub url: String,
    pub expires_at: Option<OffsetDateTime>,
    pub provider: String,
}
```

**首发内置实现(按优先级):**

1. **`rclone` 后端** (MVP 必做) —— subprocess 调 `rclone copyto` + `rclone link`。用户 `rclone config` 配好一个远端即可,rclone 原生支持阿里云 OSS、腾讯云 COS、Cloudflare R2、S3、WebDAV、SFTP 等几十种后端。**这一个实现覆盖 80% 的需求**,并把"选云"的责任推到 rclone 生态。
2. **原生 S3 / R2** (v0.2) —— 用 `aws-sdk-s3`,对想直接走 Cloudflare R2 的特别友好(免出口费、香港节点可用)。
3. **本地 HTTP 分享** (v0.3) —— `axum` 起一个带 token 的临时 server,生成 `http://<LAN_IP>:port/<token>` 短期链接,用于内网或同城场景的退路。

**多后端同时交付:** `gig deliver --uploader r2,aliyun-hk` 允许一次上传到多个后端,产出多个链接打到剪贴板,客户挑能用的。这个能力排在 v0.2。

---

## 7. workflow 各阶段 → 命令映射

| Workflow 阶段 | 现状痛点 | 对应命令 |
|---|---|---|
| 沟通前 | 多群盯着,错过或反应慢 | `gig new --lead` 一行录入;`gig ls` 看手上潜在单 |
| 沟通 / 报价 | 重复话术 | `gig template show quote` 拉模板;`gig lead promote` 走客户录入向导 |
| 开始开发 | 手动建文件夹、写 README | `gig init <id>` 一条命令完成 |
| 开发过程 | 改了几次记不清 | `gig change add` / `gig price` 强制留痕 |
| 交付 | 手动打包上传,云盘慢 | `gig deliver <id>` 一条命令完成 |
| 交付后 | 忘了是否到账、归档散乱 | `gig paid` → `gig archive`;`gig ls` 自动提醒"delivered 未收款" |
| 复盘 | 原本没有 | `gig stats`、`gig stats --by tag` |

---

## 8. 桌面端过渡路径

- `crates/gig-desktop/` 用 `cargo tauri init` 生成
- 前端选 SvelteKit 或 React + Vite(任选,目前倾向 Svelte)
- Tauri commands 直接 `use gig_core::services::*`,0 行业务逻辑重复
- 首页 = `gig ls` 的图形化版本:看板、拖拽改状态、图表
- 桌面端补强**视觉向的场景**:图表、时间线、附件预览、模板所见即所得编辑
- CLI 和桌面长期共存:日常敲命令,复盘/看报表用桌面

---

## 9. 里程碑

**v0.1 — MVP(可以用起来):**
- core:Order + Client + RequirementChange + PriceHistory + DeliveryArtifact 模型 & 服务
- CLI:`new`、`ls`(含今日应关注)、`show`、`init`、`change`、`price`、`paid`、`archive`、`stats`(本月基础看板)、`cd`
- lead 子空间:`promote`、`drop`
- `config` 子空间 + XDG 路径
- `rclone` Uploader,`pack` + `deliver`
- 基础模板:README 骨架、报价回复、交付 checklist
- `backup` + `doctor`

**v0.2 — 打磨:**
- `gig stats --by tag | --by client`,`gig export`
- S3/R2 原生 Uploader,多后端并发交付
- `client merge`
- `template` 子空间完整
- 本月 / 任意区间报表

**v0.3 — 桌面端起步:**
- `crates/desktop` 初始化,Tauri + Svelte
- 只做只读看板 + 统计图

**v0.4+:**
- 本地 HTTP Uploader、桌面端可写入、模板 WYSIWYG、提醒集成

---

## 10. 开放问题 / 非目标确认

**已确认非目标:**
- 不做企业微信侧的抓取/发送/机器人
- 不做多用户、云同步、团队版
- 不自研云存储

**仍待决策(可在实施阶段再定):**
- 打包格式:`tar.zst` vs `zip`(默认哪个?客户 Windows 能解 zst 吗?可能需要 `zip` 默认)
- 标签体系初始种子:是否预置 `python / web / 爬虫 / 数据分析 / 自动化 / 其他`?
- ID 是否需要月份前缀(如 `2604-042`)以便口头报给客户?
- 备份是否要加密?(涉及客户信息)
- 是否要在 `gig init` 时自动起一个 `direnv` `.envrc`?
