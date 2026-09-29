# rule <一句话>

修改这个 skill 本身. 干活过程中踩了坑, 或 JC 说 "以后都这样", 就用它. 无参数时从当前会话里提炼这次该记住的教训, 先列给 JC 选.

## 步骤

1. 判断这条是通用规则还是项目专属:
   - 通用 (以后每单都适用): 进 skill. 位置是 `SKILL.md` 某节, `commands/<子命令>.md`, `references/workflow.md`, 或 `templates/` 里的文件.
   - 项目专属 (只对这单): 进项目根目录 `AGENTS.md` "项目专属" 一节, 不进 skill.
   把判断和理由说给 JC, JC 可以改判.
2. 通用规则: 找到应放的位置, 给出精确 diff (改前 / 改后原文). 和现有规则矛盾的, 把矛盾条目列出来, 让 JC 选覆盖还是收窄.
3. JC 批准 diff 后:
   - 写入文件.
   - `CHANGELOG.md` 追加一行: `- YYYY-MM-DD (<来源 slug 或 "对话">): <一句话>`.
   - 运行 `python3 -m unittest discover -s tests` (在 skill 目录下). 不过就回滚并报告.
   - 提示 JC 这个 skill 在 `~/Documents/dev-tools/gig` 仓库里, 需要 commit (agent 可以本地 commit, 不推送).
4. 需要改 gig 代码才能实现的: 不改代码, 在 `TODO-gig.md` 追加一条 (日期, 需求, 来源), 告诉 JC.
5. 改动下次调用 skill 时生效. 当前会话里也按新规则做.

## 边界

- 不删既有规则, 只覆盖或收窄, 并在 CHANGELOG 留痕.
- 不改 `references/workflow.md` 里 [JC] 标注的段落, 除非 JC 明确说改的就是那段.
- 模板改动要保持 gig 渲染需要的变量名 (`slug`, `title`, `price`, `currency`, `today`, `material_path`, `client_words`, `draft_notes`, `project_type`, `platform`, `cut_ratio`, `warranty_days`).

## 回复

判断 (通用 / 项目), diff, 测试结果, CHANGELOG 那一行.
