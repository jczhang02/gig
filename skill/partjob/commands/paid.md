# paid [日期] [金额]

JC 说钱到了.

## 步骤

1. 订单必须是 `delivered`. 不是的话说明当前状态, 不强改.
2. `gig paid --order <slug> [--date YYYY-MM-DD] [--amount 800]`. 日期默认今天; 金额只在实收和 QUOTE.md 不一致时给, 会记进 price_history.
3. 返回的 `warranty_until` 是售后截止日. 把 QUOTE.md "付款状态" 改成 "已收款 YYYY-MM-DD, 售后期至 <warranty_until>" (这是 JC 明确说了收款才能改的商务事实).
4. JOB.md "状态" 追加一条.

## 回复

收款日期, 金额, 售后截止日. 售后期内客户提问由 JC 解答, 修改走 `/partjob revise`; 售后期过后 `/partjob archive`.
