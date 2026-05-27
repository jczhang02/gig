# `gig` — 报价到交付的接单流程重构设计

**状态:** Approved design, implementation not started  
**日期:** 2026-05-26  
**背景:** 基于用户当前接单流程、现有 `gig` v0.1 代码与历史 SQLite 数据库

## 0. 背景

用户的真实工作流不是从“订单”开始的，而是从一次客户沟通后的报价判断开始：需求已经大致明确，但客户可能嫌贵，用户也可能决定不接。现有 `gig` 更像订单管理工具，比较适合已经确定要做的项目；它没有很好覆盖报价草稿、计划审批、验收证据和丢单复盘。

这次重构的目标是让 `gig` 成为一个本地、可控、以人为中心的接单流程控制台。它不直接调用 AI 干活，也不替用户做最终判断，而是负责状态、文件、提示词、证据和历史价格沉淀。

## 1. 目标

1. 在客户沟通后，根据需求摘要和项目类型生成推荐报价。
2. 在未成交阶段只创建轻量 quote draft，不污染 `/home/jc/dev/partjobs`。
3. 客户接受报价后，再创建正式项目目录并生成完整计划文件。
4. 计划必须由用户显式批准后才能进入执行。
5. 每条验收标准必须有验证方法、证据和结论，不能只靠 agent 自述完成。
6. `gig ls` 默认应该像决策看板，优先显示需要用户处理的事项。
7. 价格推荐采用“默认规则 + 历史记录校准”，并复用现有数据库里的旧订单记录。
8. 交付阶段要生成完整交付文档集，采用 `DELIVERY.md` 一源双版：用户看内部版，客户收 HTML/PDF 版。

## 2. 非目标

- 不自动读取企业微信聊天，也不自动发送报价。
- 不把 `gig` 变成 AI agent 编排器。
- 不在报价阶段生成完整实施计划。
- 不在客户接受报价前创建 `/home/jc/dev/partjobs/<slug>`。
- 不把丢单机会混进正式订单表里当成订单管理。

## 3. 核心设计判断

推荐方案是“quote-to-delivery state machine”。也就是 `gig` 管状态和文件，agent 只消费它生成的 prompt 和 md/html 规范。

不采用两个极端方案：

- 只做文件生成器：太轻，无法沉淀价格历史和决策状态。
- 直接做 agent 编排器：太重，用户会失去控制感，也不符合“人可用工具”的方向。

## 4. 项目类型

内置 7 类项目类型，另外保留 `custom`：

| 类型 | 说明 |
|---|---|
| `automation_script` | 自动化脚本、批处理、小工具 |
| `data_processing` | 数据清洗、表格处理、格式转换、统计分析 |
| `crawler` | 爬虫、网页抓取、批量下载、反爬处理 |
| `cv_ml` | 计算机视觉、机器学习、深度学习实验或工程 |
| `document_pdf` | PDF、Word、文档抽取、内容替换、排版处理 |
| `frontend_web` | 前端页面、可视化、Web UI |
| `research_writing` | 论文、报告、科研材料、文字与图表工作 |
| `custom` | 不适合以上分类的项目 |

项目类型必须出现在 quote draft 和正式 order 上。旧订单需要补类型，否则历史价格无法真正校准新报价。

## 5. 状态机

### 5.1 报价阶段

报价阶段是轻量流程：

```text
quote_draft -> quoted -> accepted
            -> dropped
            -> needs_clarification
```

状态含义：

| 状态 | 含义 | 典型下一步 |
|---|---|---|
| `quote_draft` | 已录入需求摘要和项目类型，正在生成或调整报价 | 发送报价、补充需求、丢单 |
| `quoted` | 已向客户报价，等待客户回应 | 接受、丢单、改价 |
| `needs_clarification` | 信息不足，需要向客户追问 | 回到 quote draft 或 quoted |
| `accepted` | 客户接受报价，准备创建正式订单 | 生成项目目录和计划 |
| `dropped` | 未成交或用户决定不接 | 记录原因，进入历史复盘 |

报价阶段只存轻量文件，不生成 `PLAN.md` 或 `PLAN.html`。

### 5.2 成交后阶段

客户接受报价后，才进入正式订单流程：

```text
accepted -> plan_ready -> plan_approved -> in_progress
         -> ready_to_deliver -> delivered -> revision
         -> paid -> archived
```

状态含义：

| 状态 | 含义 | 阻塞条件 |
|---|---|---|
| `accepted` | 已成交，但还没有完整计划 | 尚未生成 `PLAN.md` / `PLAN.html` |
| `plan_ready` | 计划已生成，等待用户审阅 | 用户未批准 |
| `plan_approved` | 用户已明确批准计划 | 可以开始执行 |
| `in_progress` | 正在实现 | 验收证据未齐 |
| `ready_to_deliver` | 所有验收项通过，交付物已整理，内部交付文档和客户版 HTML/PDF 都已生成 | 等待用户确认发送给客户 |
| `delivered` | 已交付客户 | 等客户反馈或付款 |
| `revision` | 客户要求修改 | 重新记录变更和证据 |
| `paid` | 已收款 | 可归档 |
| `archived` | 已结束 | 无 |

## 6. 文件布局

### 6.1 报价阶段文件

报价草稿存放在 XDG 数据目录，不进入 `partjobs`：

```text
$XDG_DATA_HOME/gig/quotes/<quote-id>/
├── JOB.md
├── QUOTE.md
└── prompts/
    └── quote.md
```

默认展开路径通常是：

```text
/home/jc/.local/share/gig/quotes/<quote-id>/
```

报价阶段的 `JOB.md` 是轻量版，只包含需求摘要、项目类型、报价依据和待澄清问题。它不是执行文档。

### 6.2 成交后文件

客户接受报价后才创建正式项目目录：

```text
/home/jc/dev/partjobs/<slug>/
├── JOB.md
├── PLAN.md
├── PLAN.html
├── ACCEPTANCE.md
├── DELIVERY.md
├── DELIVERY_INTERNAL.html
├── DELIVERY_CLIENT.html
├── DELIVERY_CLIENT.pdf
├── delivery/
│   └── <YYYY-MM-DD>/
│       ├── manifest.md
│       ├── client/
│       ├── internal/
│       └── artifacts/
└── prompts/
    ├── plan.md
    ├── execute.md
    └── verify.md
```

`JOB.md` 从 quote draft 迁移而来，并逐步补全交付物、输入、验收标准、执行日志和交付记录。

`PLAN.md` 和 `PLAN.html` 只在成交后生成。`PLAN.html` 主要给用户自己审计划，不是默认发给客户。

交付阶段不是简单丢一个压缩包。`DELIVERY.md` 是事实源，记录这次交付到底包含什么、怎么用、哪些验收项已经通过、哪些文件应该发给客户、哪些细节只给用户自己看。`DELIVERY_INTERNAL.html` 从同一份事实源生成，保留完整证据链和操作判断；`DELIVERY_CLIENT.html` 和 `DELIVERY_CLIENT.pdf` 也是从同一份事实源生成，但只写客户需要知道的结论。

每次正式交付还要落到一个日期目录里：

```text
delivery/<YYYY-MM-DD>/
├── manifest.md
├── client/
├── internal/
└── artifacts/
```

`manifest.md` 说明本次交付对应的版本、客户可见文件、内部留存文件和关键产物路径。`client/` 放准备发给客户的文件，`internal/` 放用户自己看的复盘、证据和草稿，`artifacts/` 放原始输出、截图、日志、压缩包或其他交付产物。这样以后查账、改版或追问问题时，不需要靠记忆还原当时发了什么。

## 7. `PLAN.html` 固定内容

`PLAN.html` 固定六块：

1. 需求理解
2. 交付物
3. 实施阶段
4. 验收标准
5. 风险与问题
6. 报价依据

如果 agent 在生成计划时发现信息不足，不能硬编计划。它应该把项目打回 `needs_clarification`，并生成一个短问题列表，每个问题附带一个默认假设，方便用户继续问客户。

## 7.5 交付文档：一源双版

交付文档采用一源双版：`DELIVERY.md` 是唯一事实源，内部版和客户版都从它生成，不允许两边各写一份然后靠人工同步。事实源里可以有内部字段和客户可见字段，但同一个交付物、同一条验收结论、同一个版本号只能有一个事实来源。

内部版 `DELIVERY_INTERNAL.html` 给用户自己看，要保留完整证据链和操作细节，包括：

- 验收项、验证方法、证据链接和 pass/fail 结论。
- 执行过程中的关键记录、临时假设和已知限制。
- 风险判断、是否需要返工、是否建议接受客户修改。
- 交付前检查清单，包括文件是否齐、是否能打开、是否已脱敏。
- 给客户的消息草稿，方便用户复制前再改语气。

客户版 `DELIVERY_CLIENT.html` 和 `DELIVERY_CLIENT.pdf` 是正式交付说明，规则是只写结论，不写详细验证日志，也不暴露内部判断。它应该包含：

- 本次交付物清单和每个文件的用途。
- 使用说明、运行步骤或查看方式。
- 完成结论和验收结论，用客户能理解的话说明已经完成什么。
- 重要注意事项，比如输入格式、环境要求、已确认的边界。
- 反馈、修改和付款的下一步。

客户版不能出现“agent 认为”“内部判断”“日志显示”这类过程话术。它要像一份交付文件，不像施工记录。内部版可以详细，客户版要干净。

## 8. 报价规则

报价采用“默认规则 + 历史记录校准”。

默认规则按项目类型维护，每类至少包含：

- 基础价格区间
- 推荐价格
- 难度触发项
- 加价触发项
- 降价或不接触发项
- 常见风险

输出时不要只给一个数字，而是给：

```text
推荐报价: 600-800 CNY
建议报价: 700 CNY
加价触发:
- 客户要求 GUI
- 输入数据格式不稳定
- 需要交互式反复调试
历史参考:
- #19 pdf-content-replace final_price=350 CNY
- #13 website-scraper-keywords final_price=600 CNY
```

## 9. 历史校准

当前 live 数据库位于：

```text
/home/jc/.local/share/gig/gig.db
```

已确认当前库里有：

| 表 | 数量 |
|---|---:|
| `orders` | 10 |
| `price_history` | 5 |
| `delivery_artifacts` | 7 |
| `clients` | 0 |

现有 `orders` 价格字段是 `quoted_price` 和 `final_price`，`price_history` 字段是 `old_price` 和 `new_price`。部分旧订单只有 `final_price`，没有 `quoted_price`。这说明历史数据能用，但需要清洗和补类型。

重构后应该提供一个 backfill 流程：

```text
gig price backfill-types
gig price history
gig price similar --type document_pdf --summary <file>
```

backfill 时不要自动瞎猜并写死。可以先根据 slug/title 给建议类型，再由用户确认。

## 10. 验收证据

每条验收标准必须有三列：

| 验收项 | 验证方法 | 证据与结论 |
|---|---|---|
| PDF 内容替换正确 | 对比替换前后页面文本和截图 | `evidence/pdf-check.png`, pass |

`ready_to_deliver` 的前置条件：所有验收项都有验证方法、证据路径或说明、结论，并且结论为 pass；交付物已经按 `delivery/<YYYY-MM-DD>` 整理；`DELIVERY.md` 已补全；`DELIVERY_INTERNAL.html` 已生成；`DELIVERY_CLIENT.html` 和 `DELIVERY_CLIENT.pdf` 已生成；用户确认这些文件可以发送给客户。

如果验收项没有证据，或者客户版交付文档还没有生成，`gig` 应该阻止进入 ready-to-deliver，而不是只警告。

## 11. 决策看板

`gig ls` 的默认输出不应只是订单列表，而应该优先回答“我现在要处理什么”。

看板分组示例：

```text
Needs quote
- Q12 PDF table extraction, missing price recommendation

Waiting client
- Q10 crawler keywords, quoted 600 CNY, 2 days ago

Needs plan approval
- #21 uav-localization, PLAN.html ready

Needs evidence
- #19 pdf-content-replace, 2/5 acceptance items verified

Payment follow-up
- #15 araucariaceae-equations, delivered 8 days ago, unpaid
```

这会把用户的注意力从“查项目”转成“处理决策”。

## 12. 命令草案

报价相关：

```text
gig quote new --type <type> --title <title> --summary <file> [--client <name>] [--source <org>]
gig quote price <quote-id>
gig quote send <quote-id>
gig quote revise <quote-id> --price <amount> --reason <text>
gig quote clarify <quote-id>
gig quote accept <quote-id> --slug <slug>
gig quote drop <quote-id> --reason <reason>
```

计划相关：

```text
gig plan generate <order-id>
gig plan approve <order-id>
gig plan reject <order-id> --reason <text>
```

验收相关：

```text
gig acceptance add <order-id> --item <text> --method <text>
gig acceptance evidence <order-id> <item-id> --path <path> --result pass
gig ready <order-id>
```

日常入口：

```text
gig ls
gig show <id-or-slug>
gig dashboard
```

`gig dashboard` 可以先作为 `gig ls` 的别名或详细版，不必一开始做 TUI。

## 13. 数据模型方向

不要把 quote draft 强行塞进 `orders`。建议新增 `quote_drafts`：

```text
quote_drafts
- id
- title
- slug_candidate
- client_name
- source_org
- project_type
- requirement_summary
- quoted_price
- recommended_price
- quote_range_min
- quote_range_max
- pricing_rationale
- status
- drop_reason
- created_at
- quoted_at
- accepted_at
- dropped_at
```

`orders` 新增或补齐：

```text
- project_type
- quote_draft_id
- plan_status
- acceptance_status
```

也可以增加专门表：

```text
acceptance_items
- id
- order_id
- description
- verification_method
- evidence
- result
- updated_at

prompt_artifacts
- id
- owner_type
- owner_id
- kind
- path
- created_at
```

## 14. 迁移原则

现有订单历史必须保留。迁移不能破坏旧命令的基础可用性。

迁移步骤应该是：

1. 新增 quote/acceptance/project_type 相关字段和表。
2. 给现有 `orders` 添加 `project_type`，初始为空或 `custom`。
3. 提供 backfill 命令辅助用户补类型。
4. 新 workflow 命令先覆盖新状态，旧命令逐步映射或标记 deprecated。

## 15. 开放问题

以下问题不阻塞设计，但会影响实施顺序：

1. quote draft 的 ID 是否独立显示为 `Q12`，避免和 order `#12` 混淆。
2. `ACCEPTANCE.md` 是否独立存在，还是完全并入 `JOB.md`。
3. `gig ls` 默认是否直接变成看板，还是保留旧列表并新增 `gig dashboard`。
4. 历史 backfill 是一次性交互命令，还是在每次 pricing 时顺手提示补类型。

建议：quote draft 用 `Q{id}`；验收证据独立放 `ACCEPTANCE.md`，同时在 `JOB.md` 摘要引用；`gig ls` 默认改成看板；backfill 单独做命令。

## 16. 成功标准

重构完成后，一条新机会应该能这样走完：

1. 用户粘贴需求摘要并选择项目类型。
2. `gig` 生成报价区间、建议价、加价触发和历史参考。
3. 用户报价后，机会进入 `quoted`，并在看板里显示为 waiting client。
4. 客户接受后，`gig` 创建正式项目目录。
5. agent 根据 prompt 生成 `PLAN.md` 和 `PLAN.html`。
6. 用户显式批准计划。
7. agent 执行项目。
8. 每条验收标准都有证据。
9. `gig` 根据 `DELIVERY.md` 生成内部版 `DELIVERY_INTERNAL.html`，再生成只写结论的客户版 `DELIVERY_CLIENT.html` 和 `DELIVERY_CLIENT.pdf`。
10. 交付物进入 `delivery/<YYYY-MM-DD>`，客户可见文件、内部留存和原始 artifacts 分清楚。
11. 用户确认客户版可以发送后，`gig` 允许进入 ready_to_deliver。
12. 用户交付、记录反馈、收款、归档。

整个过程中，用户不需要凭记忆判断“下一步该干嘛”。`gig` 应该直接把下一步摆在面前。
