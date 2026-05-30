# gig — 个人接单管理工具

一个面向自由职业者的订单全生命周期管理 CLI，覆盖从线索跟踪到归档的完整工作流。

## 安装

```bash
cargo install --path crates/gig-cli
```

### Zsh completion

```bash
mkdir -p ~/.local/share/zsh/site-functions
gig completion zsh > ~/.local/share/zsh/site-functions/_gig
```

然后把 `~/.local/share/zsh/site-functions` 加入 `fpath`（如果你还没加），并执行：

```bash
autoload -Uz compinit && compinit
```

## 初始配置

```bash
gig config --edit
```

```toml
[general]
dev_root = "~/dev/partjobs"          # 项目开发目录
archive_root = "~/Documents/archive" # 归档目录
default_cut_ratio = 0.6              # 默认到手比例（60%）
default_currency = "CNY"

[pack]
default_format = "zip"               # 打包格式：zip 或 tar.zst
extra_ignore = []                    # 额外忽略规则（gitignore 语法）

[delivery]
default_uploader = "s3:aliyun-hk"    # 默认上传器

[delivery.short_link]
enabled = true
endpoint = "https://go.jczhang.cc/api/links"
token = "..."                        # Cloudflare Worker 的 SHORT_LINK_TOKEN

[delivery.s3.aliyun-hk]
bucket = "my-bucket"
region = "cn-hongkong"
endpoint = "https://s3.oss-cn-hongkong.aliyuncs.com"
# Optional: use a faster presigned-download endpoint while uploads keep using endpoint.
# Enable the target bucket's transfer acceleration/CDN support before setting this.
# download_endpoint = "https://oss-accelerate.aliyuncs.com"
access_key = "..."
secret_key = "..."
link_ttl_seconds = 604800            # 分享链接有效期：7 天
path_style = false
```

添加渠道（用于自动计算到手金额）：

```bash
gig source add --name "企业微信A群" --cut-ratio 0.6
gig source add --name "直接客户" --cut-ratio 1.0
```

## 完整工作流

```
Lead → Negotiating → Accepted → PlanReady → PlanApproved → InProgress
                                                               ↓
ReadyToDeliver → Delivered ⇄ Revision → Paid → Archived
        ↘ Cancelled (任何非终态均可取消)
```

`gig` 现在使用 workflow-only 交付路径：

- `gig quote` / `gig plan` / `gig acceptance` / `gig package` 负责状态推进、文件校验、交付上传与留痕。
- 外部 workflow / agents 负责生成项目文件、计划文件、验收材料和客户交付包内容；`gig` 不生成这些文件。

### XDG 报价草稿 vs 项目内 `.gig/`

报价阶段还没有正式项目目录，因此 `gig quote new` 只在 XDG 数据目录中记录报价草稿路径，例如 `$XDG_DATA_HOME/gig/quotes/<slug>`，不会创建项目目录或工作流文件。

客户接受报价后，外部工作流应已经准备好正式项目目录及项目内 `.gig/` 文件。此时用 `gig quote accept` 登记这些路径：

```bash
gig quote new --slug crawler-a --title "数据爬虫" \
  --project-type crawler \
  --summary "采集公开列表并导出 CSV"

gig quote price crawler-a --min 3000 --recommended 5000 --max 8000
gig quote mark-sent crawler-a

gig quote accept crawler-a --project-dir ~/dev/partjobs/crawler-a
```

`gig quote accept` 只记录 `<project>/.gig/JOB.md`、`QUOTE.md`、`INDEX.html` 等路径并创建订单，不会生成这些文件。缺失文件会阻塞后续 `plan` / `acceptance` / `package` 命令。

### 计划、验收和安全交付包

```bash
gig plan ready crawler-a       # 校验 .gig/plan/PLAN.md 和 PLAN.html
gig plan approve crawler-a     # plan_ready → plan_approved
gig work start crawler-a       # plan_approved → in_progress
gig acceptance complete crawler-a

# 外部 workflow 先准备 .gig/delivery/2026-05-27/ 下的交付文档和客户包
gig package check crawler-a \
  --delivery-date 2026-05-27 \
  --delivery-dir ~/dev/partjobs/crawler-a/.gig/delivery/2026-05-27

gig package send crawler-a \
  --delivery-date 2026-05-27 \
  --delivery-dir ~/dev/partjobs/crawler-a/.gig/delivery/2026-05-27
```

`gig package check` 是 `ReadyToDeliver` 后的交付门禁。外部 workflow 必须先准备 `.gig/delivery/<YYYY-MM-DD>/DELIVERY.md`、`internal/DELIVERY_INTERNAL.html`、`client/DELIVERY_CLIENT.html`、`client/DELIVERY_CLIENT.pdf`，以及 `export/client-package.zip`。`gig package check` 会执行严格安全校验并记录 validated package；只有存在该订单的 validated package 后，`gig ls` / `gig show` 的下一步才会变成 `send_package`。允许交付的文件必须写在 `manifest.toml` 的 `client_files` 中；`.gig/`、`internal/`、`prompts/`、`ACCEPTANCE.md`、`DELIVERY_INTERNAL.html` 等路径会被拒绝，避免把内部材料泄露给客户。

`gig package send` 会再次校验并用配置好的 uploader 上传 zip，记录 delivery artifact 与 sent 状态。

旧订单可以没有 `order_workflow` 行和 `project_type`。它们仍可 `list`、`show`、`export`、`archive`；机器可读输出会把需要工作流元数据的下一步标为 `legacy_workflow_metadata_missing`。后续如果要补齐旧订单类型，核心库提供了 `orders::update_project_type`，可以由迁移脚本或未来 CLI 助手调用；本阶段不强制回填。

---

### 1. 发现线索

在群里看到接单信息，先记录为线索：

```bash
gig new --lead --title "某公司官网重做" --quoted-price 5000
# created order #1 (lead)
```

查看当前所有线索：

```bash
gig lead ls
```

### 2. 沟通谈判

开始和客户沟通后，推进到谈判状态：

```bash
gig lead promote 1
# lead → negotiating
```

沟通过程中随时记录备注：

```bash
gig note 1 "客户要求响应式设计，需兼容移动端"
gig note 1 "预算有弹性，看最终方案"
```

### 3. 确认接单

谈判完成，确认价格和需求：

```bash
gig price --amount 5000 --reason "最终报价" 1
gig status 1 accepted
```

或者跳过线索/谈判，直接创建已确认订单：

```bash
gig new --title "数据爬虫" --slug data-scraper \
  --quoted-price 3000 --final-price 3000 \
  --source "企业微信A群"
# created order #2 (accepted)
```

### 4. 项目接入与上下文

项目目录和 `.gig/` 结构由外部 workflow 维护。`gig` 只登记并使用这些路径。

快速跳转到已登记项目目录：

```bash
cd $(gig cd 1)
```

### 5. 开发过程

随时记录需求变更和价格调整：

```bash
# 客户新增需求，加价 500 元
gig change --message "新增后台管理页面" --delta 500 1

# 客户缩减需求，减价 200 元
gig change --message "去掉支付模块" --delta -200 1

# 添加标签方便筛选
gig tag 1 web vue

# 修改到手比例（如更换渠道）
gig cut 1 0.7
```

查看订单全貌：

```bash
gig show 1
gig show 1 --json
```

输出包含：价格历史、需求变更记录、备注、标签；支持层订单还会显示工作流路径和下一步决策。

### 6. 交付

支持层交付先校验客户包，再发送：

```bash
gig package check 1 \
  --delivery-date 2026-05-27 \
  --delivery-dir ~/dev/partjobs/crawler-a/.gig/delivery/2026-05-27

gig package send 1 \
  --delivery-date 2026-05-27 \
  --delivery-dir ~/dev/partjobs/crawler-a/.gig/delivery/2026-05-27
```

`package check` 会校验 `manifest.toml` 与 `client-package.zip`（严格 allowlist），并记录 validated package。`package send` 要求已存在匹配的 validated package，随后会：

- 再次校验交付目录和客户包；
- 通过配置的 uploader 上传 zip；
- 写入 `delivery_artifacts`；
- 把 package 置为 `sent`，订单置为 `delivered`。

如果只是给某个订单上传临时附件或单独文件，不推进客户交付状态，用 `artifact send`：

```bash
gig artifact send 1 report.pdf screenshot.png
```

`artifact send` 会上传每个文件并写入 `delivery_artifacts`，但不会把订单改成 `delivered`。

### 7. 客户要求修改

客户收到后提出修改意见：

```bash
gig status 1 revision
# delivered → revision
```

修改完成后外部 workflow 重新生成 `.gig/delivery/<date>/` 内容，先校验客户包，再发送：

```bash
gig package check 1 \
  --delivery-date 2026-05-28 \
  --delivery-dir ~/dev/partjobs/crawler-a/.gig/delivery/2026-05-28

gig package send 1 \
  --delivery-date 2026-05-28 \
  --delivery-dir ~/dev/partjobs/crawler-a/.gig/delivery/2026-05-28
```

### 8. 收款

客户确认，款项到账：

```bash
gig paid 1
# delivered → paid
```

如果已经归档但客户延迟付款，仍可标记收款：

```bash
gig paid 1
# 状态保持 archived，paid_at 时间戳被记录
```

### 9. 归档

项目完结，归档到 `archive_root`：

```bash
gig archive 1
# 项目目录从 dev_root 移动到 archive_root
# paid → archived
```

如果不需要保留本地文件：

```bash
gig archive 1 --purge
# 删除 dev_path 和 archive_path 下的所有文件
```

归档前会自动检查 git 未提交变更并警告。

---

## 日常管理

### 本地 GUI（只读）

```bash
gig gui              # 打开 127.0.0.1 上的本地只读看板
gig gui --no-open   # 只打印带 token 的本地 URL
```

`gig gui` 只通过 `gig-core` 读取本地数据库、配置和 action metadata，不调用现有 CLI 命令；当前版本提供 dashboard、orders、actions 和脱敏 config 视图。

### 查看当前状态

```bash
gig ls              # 今日焦点面板：活跃订单 + 本月收入
gig ls --json       # 机器可读决策面板
gig ls --all        # 全部订单表格
gig ls --status in_progress   # 按状态筛选
```

### 收入统计

```bash
gig stats
# total income: CNY 12000.00
# orders: 8
# avg value: CNY 1500.00
```

### 导出

```bash
gig export csv                 # 输出到 stdout
gig export json --output orders.json
```

### 批量导入已有项目

```bash
gig import ~/dev/partjobs/old-project-*  # 批量注册已有目录
gig import --interactive                  # 交互式逐项确认元数据
gig import --relocate                     # 导入并移动到标准目录结构
```

### 维护

```bash
gig doctor          # 检查数据和路径一致性
                    # 也检查工作流文件和客户交付包是否缺失/不安全
gig backup          # 备份数据库
```

## 命令迁移说明

旧的公开交付入口已从命令面移除。客户交付统一使用 `gig package check` 后接 `gig package send`；单独文件上传使用 `gig artifact send`，不改变订单状态。

## OSS 费用参考（香港区）

| 项目 | 免费额度 | 超出价格 |
|------|----------|----------|
| 存储 | 5 GB/月 | $0.017/GB/月 |
| 出站流量 | 100 GB/月 | $0.118/GB |
| API 请求 | 读 5 亿次 / 写 1 亿次 | 几乎可忽略 |

典型自由职业者月费：**$0 ~ $1**（多数在免费额度内）。

## 状态机

```
           promote
  Lead ──────────→ Negotiating
    │                   │
    └→ Accepted ←───────┘
          │
          ↓
      PlanReady ──→ PlanApproved ──→ InProgress ──→ ReadyToDeliver
          │                │              │               │
          ↓                ↓              ↓               ↓
      Cancelled        Cancelled      Cancelled       Delivered ⇄ Revision
                                                             │
                                                             ↓
                                                            Paid
                                                             │
                                                             ↓
                                                          Archived
```

- **Lead**：线索，尚未接触
- **Negotiating**：沟通中
- **Accepted**：已确认接单
- **PlanReady**：计划文件已准备，等待批准
- **PlanApproved**：计划已批准，等待开始执行
- **InProgress**：开发中
- **ReadyToDeliver**：验收完成，等待校验客户交付包
- **Delivered**：已交付
- **Revision**：交付后返工
- **Paid**：已收款
- **Archived**：已归档
- **Cancelled**：已取消（任何非终态均可取消）
