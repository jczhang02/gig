# `partjob-workflow` — 兼职接单工作流协议设计

**状态:** Approved workflow direction, implementation not started  
**日期:** 2026-05-26  
**背景:** 基于用户当前接单方式、既有 `gig` 设计、以及“skill 主入口，`gig` 作为管理工具”的新边界

## 0. 这份文档解决什么

前一版设计容易把 `gig` 改成“整个接单系统”。这个方向不对。

真正的主体应该是接单 workflow：它决定什么时候整理需求、什么时候报价、什么时候让 agent 写计划、什么时候问用户批准、什么时候验收、什么时候交付。`gig` 只是支撑这个 workflow 的管理工具，类似 `beads`：负责记录状态、索引文件、保存历史、提醒下一步，但不替 workflow 做判断，也不直接干活。

所以这份文档先定义整体 workflow 协议。后续实现要从这里拆成两层：

1. `partjob-workflow` skill：主入口，驱动整个接单过程。
2. `gig` 支撑层：提供人也好用、skill 也能自动调用的管理命令。

## 1. 核心边界

```text
partjob-workflow skill 负责驱动
gig 负责记录和管理
文件负责承载上下文
agent 负责生成与执行
用户负责关键判断
```

这几个角色不能混在一起。

| 角色 | 该做什么 | 不该做什么 |
|---|---|---|
| `partjob-workflow` skill | 作为主入口，按阶段推进接单流程，自动调用 `gig`，读写工作文件，调度 agent，在关键点问用户确认 | 不把状态散落在对话里，不绕过用户决策 |
| `gig` | 管理机会、订单、状态、路径、历史价格、关键检查点、决策看板 | 不当 AI 大脑，不写完整计划，不判断验收是否合理 |
| 工作文件 | 承载需求、报价、计划、验收、交付事实，是 skill 与 agent 的接口 | 不依赖聊天上下文才能理解项目 |
| agent | 根据文件和 skill 约束生成计划、执行任务、填写证据、生成交付文档 | 不擅自改需求，不越过用户批准开始执行 |
| 用户 | 判断是否报价、是否成交、是否批准计划、是否交付、是否接受客户反馈 | 不需要记住全部流程细节 |

## 2. 非目标

- 不在这一阶段设计具体 `gig` CLI 语法。
- 不让 `gig` 直接调用模型或接管 agent 编排。
- 不自动读取或发送企业微信消息。
- 不把客户未接受的机会提前变成正式项目。
- 不把内部证据、执行日志、agent 判断暴露给客户。
- 不把 workflow 管理文件散放到正式项目根目录；项目根目录只放真实工作内容。
- 不默认创建 `input/`、`work/`、`output/` 这类真实工作目录；用户按项目需要自己组织。
- 不追求一次性覆盖所有接单前线索管理；第一版从“客户沟通后，已有需求摘要”开始。

## 3. 第一版入口

第一版 workflow 的入口是：用户已经和客户沟通过，手里有一段需求摘要。

也就是说，workflow 暂时不从“看到群里有活”开始，而从这一步开始：

```text
用户粘贴需求摘要 + 选择或确认项目类型
```

这是最合适的切入点。太早进入流程，会把很多还没成形的信息也纳入管理，反而增加负担。等需求摘要形成后再进入 workflow，信息足够，报价也更有意义。

## 4. 总体阶段

```text
需求整理
  -> 估价与报价
  -> 成交确认
  -> 计划生成
  -> 计划批准
  -> 执行
  -> 验收
  -> 交付文档
  -> 客户反馈 / 收款 / 归档
```

每个阶段都有固定的输入、输出、用户确认点和 `gig` 记录点。

| 阶段 | 主执行者 | 输入 | 输出 | 用户确认点 | `gig` 记录点 |
|---|---|---|---|---|---|
| 需求整理 | skill + agent | 需求摘要、项目类型 | XDG quote draft 内的轻量 `JOB.md`、缺失问题 | 信息是否足够继续报价 | opportunity/quote draft、项目类型、需求摘要路径 |
| 估价与报价 | skill + agent | quote draft `JOB.md`、历史价格、规则 | XDG quote draft 内的 `QUOTE.md` | 是否认可报价与客户话术 | 推荐价、区间、报价状态、校准依据 |
| 成交确认 | 用户 + skill | 客户反馈、报价记录 | 正式项目目录、`.gig/`、完整 `.gig/JOB.md` | 客户是否接受、是否要接 | order/project index、正式路径 |
| 计划生成 | agent | `.gig/JOB.md` | `.gig/plan/PLAN.md`、`.gig/plan/PLAN.html` | 计划是否可审 | plan 文件路径、plan_ready 状态 |
| 计划批准 | 用户 + skill | `.gig/plan/PLAN.html` | 批准记录 | 是否允许进入执行 | plan_approved 状态、批准时间 |
| 执行 | agent + skill | `.gig/plan/PLAN.md`、项目真实工作文件 | 实现结果、执行记录 | 关键偏离是否接受 | in_progress 状态、检查点 |
| 验收 | agent + skill | 验收标准、实现结果 | `.gig/acceptance/ACCEPTANCE.md` | 证据是否足够 | 验收项状态、证据路径 |
| 交付文档 | agent + skill | `.gig/acceptance/ACCEPTANCE.md`、交付物 | `.gig/delivery/<date>/` 下的事实源、内部版、客户版 HTML/PDF、客户包 zip | 是否可以发送给客户 | ready_to_deliver 状态、交付包路径 |
| 反馈/收款/归档 | 用户 + skill | 客户反馈、付款情况 | revision/payment/archive 记录 | 反馈性质、是否追加收费 | revision、paid、archived、复盘信息 |

## 4.1 文件布局原则

这套 workflow 不能把正式项目目录变成一堆流程文件。用户的偏好很明确：项目根目录只放真实工作内容，workflow 文件统一进隐藏目录。

成交前没有正式项目目录，报价草稿继续放在 `gig` 的 XDG 数据目录：

```text
$XDG_DATA_HOME/gig/quotes/<quote-id>/
├── JOB.md
├── QUOTE.md
└── prompts/
```

成交后才创建正式项目目录，并在项目内创建固定 `.gig/` 管理区：

```text
<project>/
├── 真实工作文件，用户自己组织
└── .gig/
    ├── INDEX.html
    ├── JOB.md
    ├── QUOTE.md
    ├── plan/
    │   ├── PLAN.md
    │   └── PLAN.html
    ├── acceptance/
    │   └── ACCEPTANCE.md
    ├── delivery/
    │   └── <YYYY-MM-DD>/
    │       ├── DELIVERY.md
    │       ├── client/
    │       │   ├── DELIVERY_CLIENT.html
    │       │   └── DELIVERY_CLIENT.pdf
    │       ├── internal/
    │       │   └── DELIVERY_INTERNAL.html
    │       └── export/
    │           └── client-package.zip
    └── prompts/
```

不默认创建 `input/`、`work/`、`output/`。客户给的原始文件也是项目真实工作文件的一部分，怎么摆放由用户按项目决定。

`.gig/INDEX.html` 是用户看的总入口，不是事实源。它链接当前 `JOB.md`、`QUOTE.md`、`PLAN.html`、`ACCEPTANCE.md`、内部交付页、客户交付页和最近导出的客户包，并显示当前阶段和下一步需要用户决定什么。每次报价、计划、批准、验收、交付文档状态变化后，skill 应自动刷新它。

## 5. 阶段协议

### 5.1 需求整理

用户给 skill 一段需求摘要。skill 的第一件事不是报价，而是把摘要变成结构化的轻量 `JOB.md`，并存到 XDG quote draft 区。

`JOB.md` 在这个阶段必须包含：

- 项目标题或临时标题
- 客户/来源信息，如果用户提供了
- 项目类型，使用内置 7 类或 `custom`
- 需求理解
- 输入材料
- 预期交付物
- 初步验收口径
- 缺失信息和建议追问

如果信息不足，workflow 必须停在这里。skill 生成一组可以直接问客户的问题，不进入报价。

这一点很重要：缺信息时强行报价，会导致后面计划、验收和交付全部变形。

### 5.2 估价与报价

报价由 skill 驱动，但需要 `gig` 提供历史校准数据。

报价需要同时看三类信息：

1. 项目类型的默认规则。
2. 当前需求的复杂度、风险、紧急程度和不确定性。
3. `gig` 历史记录中的相似订单价格。

输出是 XDG quote draft 区里的 `QUOTE.md`，至少包含：

- 价格区间
- 建议报价
- 加价触发条件
- 历史价格参考
- 报价理由
- 给客户看的报价话术
- 如果客户砍价，可以接受的调整边界

skill 可以自动调用 `gig` 记录报价草稿和推荐价，但不能假设报价已经发给客户。是否发送、怎么发送，仍由用户决定。

### 5.3 成交确认

只有客户明确接受报价，workflow 才进入正式项目。

成交确认时要做三件事：

1. `gig` 把 quote/opportunity 记录升级为正式 order/project。
2. 创建正式项目目录和项目内 `.gig/` 管理区。
3. 把轻量 `JOB.md` 补全成正式 `.gig/JOB.md`。

正式项目根目录只放真实工作内容。workflow 不往根目录散放 `JOB.md`、`PLAN.md`、`ACCEPTANCE.md`、`DELIVERY.md`，也不默认创建 `input/`、`work/`、`output/`。

未成交时不创建正式项目目录。skill 应引导用户记录丢单原因，例如太贵、需求不清、用户不想接、客户无回应、时间不合适。这个记录以后会反向校准报价和接单判断。

### 5.4 计划生成

成交后才生成计划。

agent 根据正式 `.gig/JOB.md` 写：

- `.gig/plan/PLAN.md`：计划源文件。
- `.gig/plan/PLAN.html`：给用户审阅的计划页面。

`PLAN.html` 固定覆盖六块：

1. 需求理解
2. 交付物
3. 执行阶段
4. 验收标准
5. 风险 / 问题
6. 定价依据

如果计划生成时发现需求仍然不够清楚，workflow 应退回问题澄清，而不是继续进入执行。

### 5.5 计划批准

计划批准是硬门槛。

没有用户明确批准，workflow 不得进入执行阶段。这里不是形式主义，因为用户要承担对客户的最终承诺。agent 可以建议，但不能替用户批准。

批准记录至少包括：

- 批准的 `.gig/plan/PLAN.html` 路径
- 批准时间
- 批准时仍存在的风险或假设
- 用户是否接受这些风险

### 5.6 执行

执行阶段由 agent 干活，skill 负责监督方向。

监督重点不是微观管理每一行代码，而是防止三件事：

- agent 偏离 `.gig/plan/PLAN.md`
- agent 偷偷改变验收标准
- agent 把未确认的新需求混进当前订单

如果执行中发现必须改变方案，skill 应先向用户说明变化，再继续。

`gig` 在这里只记录状态、路径和关键检查点，不承担执行逻辑。

### 5.7 验收

验收阶段的核心规则是：每条验收标准都必须有证据。

`.gig/acceptance/ACCEPTANCE.md` 至少包含四列：

| 验收项 | 方法 | 证据 | 结论 |
|---|---|---|---|
| 用户可逐项检查的标准 | 如何检查 | 测试结果、截图、输出文件、日志摘要、人工检查说明 | pass / blocked / not applicable |

缺证据时不能进入交付。workflow 应停下来让 agent 补证据，而不是让用户凭感觉判断。

### 5.8 交付文档

交付不是把文件丢给客户。交付阶段要生成一套完整文档，并区分内部视角和客户视角。

采用“一源双版”：

```text
.gig/delivery/<YYYY-MM-DD>/DELIVERY.md
  -> internal/DELIVERY_INTERNAL.html
  -> client/DELIVERY_CLIENT.html
  -> client/DELIVERY_CLIENT.pdf
  -> export/client-package.zip
```

`.gig/delivery/<YYYY-MM-DD>/DELIVERY.md` 是事实源，记录本次交付包含什么、如何使用、哪些验收项已经通过、哪些文件给客户、哪些信息仅内部保留。

`.gig/delivery/<YYYY-MM-DD>/internal/DELIVERY_INTERNAL.html` 给用户看，保留完整证据链、风险、执行记录、修改建议、客户消息草稿。

`.gig/delivery/<YYYY-MM-DD>/client/DELIVERY_CLIENT.html` 和 `.gig/delivery/<YYYY-MM-DD>/client/DELIVERY_CLIENT.pdf` 给客户看，只写结论：交付物、使用方法、完成情况、注意事项、反馈/付款下一步。客户版不能出现“agent 认为”“内部判断”“日志显示”这类内部过程话术。

最终给客户发送的压缩包默认是 `.gig/delivery/<YYYY-MM-DD>/export/client-package.zip`。它必须从客户可见清单导出，不能通过复制项目根目录或整段 `.gig/` 生成；如果包里出现 `.gig/`、`prompts/`、`internal/`、`ACCEPTANCE.md`、`DELIVERY_INTERNAL.html` 等内部路径或文件，workflow 应阻塞导出。

只有客户版和内部版都生成后，workflow 才能进入 `ready_to_deliver`。

### 5.9 客户反馈 / 收款 / 归档

客户反馈回来后，skill 帮用户做边界判断：

- 原需求内 bug：进入 revision。
- 原需求内小调整：可以作为 revision 处理。
- 新需求或需求扩大：建议新报价或新订单。
- 客户接受且付款：记录 paid。
- 项目结束：归档，并记录复盘。

`gig` 负责保存这些状态和历史，方便以后报价校准和复盘。

## 6. 用户确认门槛

workflow 里有几个点必须问用户，不能自动越过。

| 门槛 | 为什么必须问 | 通过后进入 |
|---|---|---|
| 信息足够报价 | 缺需求会导致报价错误 | 估价与报价 |
| 是否认可报价 | 报价是商业判断 | 客户报价等待 |
| 客户是否接受 | 未成交不能建正式项目 | 成交确认 |
| 是否批准计划 | 计划决定后续执行承诺 | 执行 |
| 是否认可验收证据 | 用户最终对客户负责 | 交付文档 |
| 是否可以发送客户版交付文档 | 交付是外部动作 | delivered |
| 客户反馈是否算新需求 | 直接影响是否免费修改 | revision 或新报价 |

## 7. 文件接口

这些文件是 workflow 的核心接口。agent、skill、`gig` 都围绕它们协作。

| 文件 | 阶段 | 谁看 | 作用 |
|---|---|---|---|
| `JOB.md` | 需求整理到归档 | 用户、skill、agent | 项目事实主记录 |
| `QUOTE.md` | 报价 | 用户、skill | 报价依据和客户话术 |
| `PLAN.md` | 计划 | skill、agent | 执行计划源文件 |
| `PLAN.html` | 计划批准 | 用户 | 可审阅的计划页面 |
| `ACCEPTANCE.md` | 验收 | 用户、skill、agent | 逐项证据与结论 |
| `DELIVERY.md` | 交付 | skill、agent | 交付事实源 |
| `DELIVERY_INTERNAL.html` | 交付 | 用户 | 内部交付检查与建议 |
| `DELIVERY_CLIENT.html` | 交付 | 客户 | 正式客户交付文档 |
| `DELIVERY_CLIENT.pdf` | 交付 | 客户 | 可发送/归档的客户交付文档 |

文件必须能独立表达项目状态。不能只有“见聊天记录”或“按上次说的”。

## 8. `partjob-workflow` skill 职责

第一版 skill 应该做这些事：

1. 接收用户粘贴的需求摘要。
2. 结构化生成轻量 `JOB.md`。
3. 判断是否缺关键需求，并生成客户追问。
4. 调用 `gig` 获取历史价格记录和创建 quote/opportunity 记录。
5. 生成 `QUOTE.md`。
6. 在用户确认成交后，调用 `gig` 创建正式项目索引和路径。
7. 调度 agent 生成 `PLAN.md` / `PLAN.html`。
8. 要求用户批准计划。
9. 按 `PLAN.md` 监督 agent 执行。
10. 要求 agent 填写 `ACCEPTANCE.md`，并检查证据完整性。
11. 生成 `DELIVERY.md` 和一源双版交付文档。
12. 在客户反馈后帮助判断 revision/new requirement/payment/archive。

skill 的语气应该像一个接单助理，不像表单系统。它应该少问、问关键问题、每次告诉用户下一步为什么重要。

## 9. `gig` 支撑层职责

`gig` 需要重构，但它不是第一主角。

它应该提供稳定、双用的能力：

- 人可以直接用，默认输出就是决策看板。
- skill 可以自动调用，输出清楚、可解析、可重复。
- 能记录 quote/opportunity/order 状态。
- 能记录项目类型、价格建议、最终价格、丢单原因。
- 能索引 `JOB.md`、`QUOTE.md`、`PLAN.html`、`ACCEPTANCE.md`、`DELIVERY_*` 路径。
- 能阻止明显越门槛的状态推进，例如未批准计划、验收证据不全、客户版交付文档未生成。
- 能保存历史记录，支持之后报价校准。

具体 CLI 语法暂不在本文档展开。后续需要单独写 `gig` 支撑层实现计划。

## 10. 状态记录原则

workflow 状态不应该只存在于聊天里。

最低限度需要沉淀这些记录：

- opportunity / quote draft
- quoted / dropped / accepted
- plan_ready / plan_approved
- in_progress
- acceptance_ready 或 evidence_blocked
- ready_to_deliver / delivered
- revision / paid / archived

这些状态名可以在后续 `gig` 设计中微调，但语义不能丢：报价、成交、计划批准、验收证据、交付文档、客户反馈、收款归档都必须可追踪。

## 11. 阻塞规则

workflow 要敢于停下来。

| 场景 | 应该怎么停 |
|---|---|
| 需求摘要不足 | 生成客户追问，不报价 |
| 历史价格不足 | 仍可报价，但明确这是规则估价，不是假装有校准 |
| 客户未接受 | 不创建正式项目目录 |
| 计划未批准 | 不执行 |
| 执行中发现需求变大 | 停下让用户判断是否改价或新订单 |
| 验收证据不全 | 不进入交付 |
| 客户版交付文档缺失 | 不进入 ready_to_deliver |
| 客户反馈越界 | 建议新报价，不默认为免费 revision |

## 12. 后续实现顺序

后续不应该直接改 `gig`。推荐顺序是：

1. 先写 `partjob-workflow` skill 草案。
2. 用 2-3 个真实历史项目走一遍纸面演练，检查文件接口是否够用。
3. 再设计 `gig` 支撑命令，让它刚好服务这个 workflow。
4. 修改 `gig` 数据模型和命令交互。
5. 回到 skill，把自动调用 `gig` 的步骤写实。
6. 为 skill 建立测试提示词和评估标准。

这个顺序可以防止再次把 `gig` 当成整个系统。

## 13. 成功标准

第一版成功，不是因为 `gig` 命令很多，而是因为用户可以这样工作：

1. 粘贴需求摘要。
2. skill 整理需求并提醒缺口。
3. skill 给出报价建议和客户话术。
4. 客户接受后，workflow 创建正式项目上下文。
5. agent 写计划，用户看 `PLAN.html` 批准。
6. agent 执行，skill 盯住计划边界。
7. 每条验收都有证据。
8. 交付时同时有自己看的内部版和客户看的正式版。
9. 客户反馈、收款、归档都沉淀到 `gig`。

如果用户不用记命令、不用回翻聊天、不用猜 agent 到底做完没有，这套 workflow 就是有效的。
