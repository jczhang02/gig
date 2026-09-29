---
name: partjob
description: JC 的兼职订单流程. 从接单前的临时目录到交付, 收款, 售后和归档, 由 agent 通过 gig 记录状态. 手动调用, 带子命令.
argument-hint: "<子命令> [参数]. 无参数等于 status. 子命令: status draft drop start decide ask log preview pack send revise paid archive handoff rule"
disable-model-invocation: true
---

# partjob

这是 JC 兼职订单的工作流. 全部规则在 `references/workflow.md`, 本文件只做路由和最短的约束. gig 是数据层, 只有 agent 用它, 每条命令输出一个 JSON 文档; 详见 `references/gig.md`.

## 路由

参数的第一个词是子命令时, 读对应的 `commands/<子命令>.md` 再动手, 不要凭记忆执行. 无参数等于 `status`.

参数不是子命令名 (JC 用自然语言说事, 比如 `/partjob 客户说不要水印`, `/partjob 钱到了`): 按意图选最接近的子命令, 回复第一句说明选了哪个 ("按 revise 处理"), 然后照那个文件做. 两个子命令都说得通, 或者看不出要干什么, 问一句再动手, 不要猜. 子命令后面跟的自由文本也一样处理 (`/partjob send 发手机` = `send --via phone`).

| 子命令 | 阶段 | 一句话 |
|---|---|---|
| `status` | 任意 | 当前在哪, 下一步, 等 JC 决定的事, 未收款和售后期内的单 |
| `draft <slug> [材料路径]` | 接单前 | 建临时目录和 NOTES.md, 摸底材料, 整理疑问和工作量 |
| `drop <slug> [原因]` | 接单前 | 放弃: 笔记存进 gig, 删目录 |
| `start <slug>` | 开工 | 登记订单, 建骨架, 配 agent 规范, 进入 grill |
| `decide <内容>` | 开工 | 把 JC 的决定写进 JOB.md 已确认决策 |
| `ask` | 开工 | 把待问客户的问题整理成可转发的中文, 写进 JOB.md |
| `log` | 开工 | 阶段结束, 追加一条状态 |
| `preview [版本]` | 交付 | 建预览包并校验 |
| `pack [版本]` | 交付 | 建完整包, 做复现验证, 校验 |
| `send <包id> [--via oss\|phone]` | 交付 | JC 批准后上传或发手机 |
| `revise` | 交付/售后 | 客户反馈: 返工还是范围变更 |
| `paid [日期] [金额]` | 售后 | 记收款, 算售后截止 |
| `archive` | 售后 | 记分卡, 报告脏状态, 批准后归档 |
| `handoff` | 任意 | 交接给下一个会话 |
| `rule <一句话>` | 任意 | 修改这个 skill 本身 |

## 只有 JC 能批准的动作

沉默, "继续", "好" 之外没有具体对象的话, 都不算批准. 批准只对当次有效.

1. 对外发送: 上传交付包, 发到手机, 发链接.
2. 推送到远程仓库.
3. 删除文件, 清理项目, 归档.
4. 使用付费远程资源.
5. 超出 `.gig/JOB.md` 已确认决策的范围变更.
6. 修改 `.gig/QUOTE.md` 里的商务事实.

批准由 JC 给, 动作由 agent 做. gig 里这些命令都要带 `--yes`; 没有 JC 当轮的明确同意, 不加 `--yes`.

## 通用规则 (每个子命令都适用)

- 开工先读 `.gig/JOB.md`. 价格和付款只信 `.gig/QUOTE.md`, 旧报告里的报价作废.
- 原件只读. 新结果写新目录, 不覆盖旧结果; 失败和不可用状态保持可见.
- 不为了接近论文或客户期望调数值. 报告里的数字来自保存的结果, 说明数据划分, 配置和聚合方式.
- 每个阶段结束在 JOB.md "状态" 里追加: 命令, 验证数字, commit / CI run, 剩余工作, 是否已对外发送.
- 交付物可复现: 用交付的程序重跑, 与源码运行逐字节比对, 记入 JOB.md.
- 验证过的工作单元及时 commit. 长实验前先提交代码, 配置, 评测协议. 检查 staged diff, 保留 JC 的并行修改. 不推送.
- 临时文件, 恢复副本放项目内 `.scratch/`, 不放上级目录. 阶段结束清空.
- 只用现有材料. 要问客户的写进 "待客户确认" (`ask`), 不自行假设, 不向客户索要更多样本 (除非 JC 决定).
- 工作目录和文件名全英文, 交付包内也是. 中文正文用 ASCII 标点. 用具体名称解释概念, 不用未定义缩写.
- 报告用 Kami 或 LaTeX, 只有最终 PDF 进 `reports/` 或交付包; 源放 `.scratch/reports/`, 成文经 sepia 润色.
- 目录保持干净: 模板之外的顶层目录要在 README 或 AGENTS.md 里说明用途.
- 子 agent 只做被指派的实际工作, 不能批准, 发送, 改范围, 删除, 归档. 交接项见 `commands/handoff.md`.

## 回复方式

说实际发生了什么 (命令和真实结果), 接下来要做什么, 有哪些事等 JC 决定. 不打印固定模板, 不从聊天气氛里制造 "完成". 决策没定的就明说没定.

## 目录

- `commands/`: 每个子命令一份, 含前置条件, gig 命令, 写入的文件, 需要的批准, 回复内容.
- `references/workflow.md`: 完整流程与规则, 本 skill 的源头.
- `references/gig.md`: gig 命令速查.
- `templates/`: gig 建项目时渲染的文件 (`JOB.md.j2`, `QUOTE.md.j2`, `AGENTS*.md.j2`, `README.md.j2`, `gitignore`, `NOTES.md.j2`).
- `CHANGELOG.md`: `rule` 子命令的每次改动.
- `TODO-gig.md`: 需要改 gig 代码的事, 不在 `rule` 里直接改.
