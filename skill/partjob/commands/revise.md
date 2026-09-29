# revise

客户看了预览或收到包之后有反馈. 先分类, 再动手.

## 步骤

1. 把客户反馈原话记进 JOB.md "状态" (带日期, 注明是预览反馈还是交付后反馈).
2. 对照 "已确认决策" 和 QUOTE.md 逐条判断:
   - 漏做了承诺的项, 或做得不对: 返工. 不改价.
   - 客户要的东西不在决策和报价范围里: 范围变更. 由 JC 决定是免费做, 加价, 还是拒绝.
   - 分不清的: 列出差异, 让 JC 判.
3. 返工:
   - 订单是 `delivered`: `gig start --order <slug>` 回到 in_progress.
   - 订单是 `paid` (售后期): 不改状态, 直接改.
   - 反馈里的新要求用 `/partjob decide` 写成新决策 (取代旧条目的写明).
   - 完成后新版本号 `/partjob pack` 或 `/partjob preview`, 再 `/partjob send`.
4. 范围变更 (JC 决定后): `gig change --desc "<变更>" [--price-delta <差价>]`. 价格变了, QUOTE.md 的价格行由 JC 改, 或 JC 明确说了新价后 agent 改并注明日期.

## 回复

分类结果 (逐条: 返工 / 范围变更 / 待 JC 判), 打算怎么做, 需要 JC 决定的点.
