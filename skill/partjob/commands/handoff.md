# handoff

交接给下一个会话或子 agent. 调用 `handoff` skill, 但交接文档必须包含下面这些, 缺一项下一个 agent 就要重新问 JC.

## 必含项

- 订单 slug, 当前 gig 状态 (`gig show` 的 `order.status` 和 `next_action`).
- 项目绝对路径, 要先读的文件 (`.gig/JOB.md`, `.gig/QUOTE.md`, `AGENTS.md`, 相关 CONTEXT.md / ADR).
- 允许写的范围 (哪些目录), 不允许碰的 (原件路径, QUOTE.md, delivery/ 里已校验的包).
- 完成标准: 引用 JOB.md 已确认决策的编号, 不另写一套.
- 最近的验证命令和它上次的结果.
- 未决事项: "待客户确认" 里没答的, 等 JC 批准的动作.
- 子 agent 的边界: 不能批准, 发送, 改范围, 删除, 归档; 不能改 QUOTE.md 和已确认决策.

## 回复

交接文档路径, 和一句下一个会话应该先跑 `/partjob status`.
