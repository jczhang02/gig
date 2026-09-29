# status

会话开始, 或 JC 问 "现在到哪了" 时用. 只读, 不改任何东西.

## 步骤

1. `gig ls` 和 `gig draft ls`. 两个都失败且错误码是 `legacy_db` 或 `config`, 停下来报告, 这是环境问题.
2. 判断当前目录:
   - 在某个项目目录内 (`gig show` 成功): 读 `data.order`, `data.next_action`, `data.packages`, 然后读 `.gig/JOB.md`, 重点是 "已确认决策" 最后几条, "待客户确认", "状态" 最后两条.
   - 不在项目内 (比如 `~/dev/partjobs`): 不算错误. 用 `gig ls` 的结果.
3. 从 `gig ls` 里挑出: `unpaid == true` 的单 (交付未收款, 带 `days_in_status`), `next_action` 以 `warranty until` 开头的单, `queued` 的单.

## 回复

- 当前项目 (如果有): slug, 状态, 下一步 (`next_action`), JOB.md 里最后一条状态的日期和内容摘要, "待客户确认" 里是否有未答的问题.
- 等 JC 决定的事: 只列 JOB.md 和 gig 里真实存在的, 不编.
- 其他单: 未收款的 (几天了), 售后期内的 (到哪天), 排队未开工的.
- 有开着的 draft 也列出来.

不要打印整个 JSON. 不要建议 JC 做流程之外的事.
