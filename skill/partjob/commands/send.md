# send <包id> [--via oss|phone]

把校验过的包送出去. 批准由 JC 给, 动作由 agent 做. 没有 JC 在本轮明确说 "发", 只做预演.

## 前置

- 包已经 `build` 或 `check` 过, 且之后没改过. 改过会报 `needs_check`, 先 `gig package check <包id>`.
- 渠道: 默认 `oss` (上传对象存储, 出短链, JC 把链接发给客户). `phone` 是用 gsconnect 发到 JC 手机, JC 再转发给客户.

## 步骤

1. 预演: `gig package upload <包id>` (不带 `--yes`). 返回 `dry_run: true`, `size`, `warnings`. 把包 id, 大小, kind (preview / full), 会导致的状态变化 (full 会把订单变成 delivered), 以及准备走的渠道告诉 JC, 一句话问: "走 OSS 出短链, 还是发到手机? 发不发?". JC 没指定渠道就默认 OSS, 但问句里要带上另一个选项, JC 回一个词就能换.
2. JC 明确同意后:
   - oss: `gig package upload <包id> --yes`. 返回 `short_url` (没开短链就是 `url`), `expires_at`. 报 `secrets` 或 `config` 错误说明环境没配好, 停下来说明, 不要绕过.
   - phone: 先调 `gsconnect-send` skill 发送 `delivery/<包id>.zip`; 送达后 `gig package sent <包id> --channel phone --yes --note "gsconnect"`.
3. JOB.md "状态": 发送时间, 渠道, 链接或 "已发手机", 过期时间. 把 "尚未对外发送" 改掉.
4. full 包发送后订单是 `delivered`, 下一步是收款; 售后期内 (`paid`) 发的包不改状态, 只记一条 warranty revision.

## 回复

链接 (完整的, 方便复制) 和过期日期, 或 "已发到手机". 订单现在的状态. 不要把链接以外的东西包装成给客户的话, 客户沟通是 JC 的事.
