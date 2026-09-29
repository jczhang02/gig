# drop <slug> [原因]

客户不同意报价, 或单子没谈成. 笔记进 gig, 目录删掉.

## 步骤

1. 没给原因就问一句. 原因会存进 gig, 以后同类需求可查.
2. `gig draft drop <slug> --reason "<原因>"`. 这是预演, 返回 `would_delete` 列表.
3. 把要删的文件列给 JC. 这是删除动作, 需要 JC 明确同意.
4. JC 同意后: `gig draft drop <slug> --reason "<原因>" --yes`. 返回的 `draft.notes_snapshot` 就是保存下来的笔记.

## 回复

删了什么, 笔记已存进 gig (可用 `gig draft ls --all` 找回). 不需要再做别的.
