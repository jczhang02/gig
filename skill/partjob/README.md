# partjob

JC 兼职订单的 agent 工作流 skill. 手动调用: Claude Code 里 `/partjob <子命令>`, Pi 里 `/skill:partjob <子命令>`.

- `SKILL.md`: 路由和最短约束.
- `commands/*.md`: 每个子命令的做法.
- `references/workflow.md`: 完整流程 (源头). `references/gig.md`: gig 速查.
- `templates/`: gig 建项目时渲染的文件. gig 的 `general.templates_dir` 指向这里.
- `CHANGELOG.md`, `TODO-gig.md`: `rule` 子命令维护.

## 安装

源目录在 gig 仓库 `skill/partjob/`. 链接到 agent 的 skill 目录:

```bash
ln -s ~/Documents/dev-tools/gig/skill/partjob ~/.agents/skills/partjob
ln -s ~/.agents/skills/partjob ~/.claude/skills/partjob
```

## 测试

```bash
cd ~/Documents/dev-tools/gig/skill/partjob && python3 -m unittest discover -s tests -v
```

集成测试需要 gig v2 可执行文件: `GIG_BIN=/path/to/gig`, 默认 `~/Documents/dev-tools/gig/target/debug/gig`.
