# start <slug>

客户下单, 这一单正式开始. 登记订单, 建正式目录, 配 agent 规范, 然后进入 grill.

## 前置

- 从 JC 那里拿到: 成交价 (主单位, 如 800), 用户原话 (客户或 JC 说成交的那句话, 原样), 项目类型 (`tool` 工具类, `cv_ml`, `data_processing`, `research_writing`, `custom`), 材料路径 (draft 里有就不用再问).
- 有同名 draft 就用 `--from-draft`, 笔记会进 JOB.md 的 "接单前笔记".
- 目录已经存在 (比如之前手工建的项目): 用 `--adopt --status in_progress`, 不动任何文件, 之后手工补 `.gig/JOB.md` 和 `.gig/QUOTE.md` 缺的部分.

## 步骤

1. 把用户原话写到 `.scratch` 之外的临时文件不合适; 直接用 `--client-words "<原话>"`, 太长就先写到 `/tmp` 下一个文件再 `--client-words @文件`.
2. `gig new <slug> --title "<标题>" --price <价> --type <类型> --material <路径> [--platform <平台>] --client-words ... [--from-draft]`.
   返回 `created_files` 和 `warnings` (git 不可用会在这里). 目录在 `~/dev/partjobs/<slug>/`, 已有 `.gig/JOB.md`, `.gig/QUOTE.md`, `AGENTS.md`, `README.md`, `.gitignore`, `data/`, `references/`.
3. 读一遍生成的 `.gig/JOB.md` 和 `.gig/QUOTE.md`, 把 "(待填)" 的地方补上已知事实. QUOTE.md 的价格和原话必须和 JC 说的一致.
4. 在项目目录里运行 `setup-matt-pocock-skills` skill (issue tracker, triage labels, domain docs 的配置). 它会在 AGENTS.md 追加 "Agent skills" 一节; 保留模板里已有的内容.
5. 按项目类型把 AGENTS.md "项目专属" 里的 "(待填)" 补上: 只读路径, 是否允许训练和在哪跑, 本地工具链, 是否授权连续工作, git 授权范围. 不确定的先留着, 在 grill 里问.
6. `gig start --order <slug>`. 状态变 `in_progress`.
7. 提示 JC 下一步用 `grill-me` (小单) 或 `grill-with-docs` (需要 CONTEXT.md 和 ADR 的单) 确定路线. grill 的结论用 `/partjob decide` 逐条写进 JOB.md.
8. 首次 commit: 骨架文件. 不推送.

## 回复

建了哪些文件, QUOTE.md 里记的价格和日期, AGENTS.md 里还有哪些 "(待填)", 下一步是 grill.
