# gig — 个人接单管理工具

一个面向自由职业者的订单全生命周期管理 CLI，覆盖从线索跟踪到归档的完整工作流。

## 安装

```bash
cargo install --path crates/gig-cli
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

[delivery.s3.aliyun-hk]
bucket = "my-bucket"
region = "cn-hongkong"
endpoint = "https://s3.oss-cn-hongkong.aliyuncs.com"
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
Lead → Negotiating → Accepted → InProgress → Delivered ⇄ Revision → Paid → Archived
                                                  ↘ Cancelled (任何阶段均可取消)
```

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

### 4. 初始化项目

**方式 A**：从已有订单创建项目目录

```bash
gig init 1
# 在 dev_root/某公司官网重做/ 下创建目录、git init、.gitignore、README
# accepted → in_progress
```

**方式 B**：在当前目录直接开始（自动创建订单 + 初始化）

```bash
cd ~/dev/partjobs/my-project
gig init
# 以当前目录名作为 slug，创建订单并注册 dev_path
```

快速跳转到项目目录：

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
```

输出包含：价格历史、需求变更记录、备注、标签。

### 6. 交付

开发完成，一键打包上传：

```bash
gig deliver 1
# packing... 42 file(s)
# uploading to s3:aliyun-hk... done
# link: https://...presigned-url...
# size: 12.3 MB
# status: in_progress → delivered
# (URL copied to clipboard)
```

把链接发给客户即可。打包规则：

- 自动排除 `.git/`、`.gitignore` 中的文件
- 项目内可添加 `.gigignore` 定义额外排除
- 配置 `[pack] extra_ignore` 定义全局排除

先预览再交付：

```bash
gig deliver 1 --dry-run       # 查看会打包哪些文件
gig pack 1                     # 只打包不上传
```

### 7. 客户要求修改

客户收到后提出修改意见：

```bash
gig status 1 revision
# delivered → revision
```

修改完成后重新交付：

```bash
gig deliver 1
# revision → delivered（delivered_at 更新为新的交付时间）
```

或使用上次的打包文件重新上传（链接过期时有用）：

```bash
gig deliver 1 --resend
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

### 查看当前状态

```bash
gig ls              # 今日焦点面板：活跃订单 + 本月收入
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
gig backup          # 备份数据库
```

## 上传任意文件

不绑定订单，直接上传文件获取分享链接：

```bash
gig upload report.pdf slides.pptx
# uploading report.pdf... done (2.3 MB)
# → https://...presigned-url...
```

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
          ↓  init
      InProgress ──→ Delivered ⇄ Revision
          │              │
          ↓              ↓
      Cancelled        Paid
                         │
                         ↓
                      Archived
```

- **Lead**：线索，尚未接触
- **Negotiating**：沟通中
- **Accepted**：已确认接单
- **InProgress**：开发中
- **Delivered**：已交付
- **Revision**：交付后返工
- **Paid**：已收款
- **Archived**：已归档
- **Cancelled**：已取消（任何非终态均可取消）
