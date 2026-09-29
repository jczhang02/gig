# partjob

Agent workflow skill for JC's freelance orders. Invoked by hand: `/partjob <subcommand>` in Claude Code, `/skill:partjob <subcommand>` in Pi. Plain-language arguments are routed to the closest subcommand.

- `SKILL.md`: routing and the shortest constraints.
- `commands/*.md`: how each subcommand works.
- `references/workflow.md`: the complete workflow (the source). `references/gig.md`: gig cheat sheet.
- `templates/`: files gig renders when scaffolding a project; gig's `general.templates_dir` points here.
- `CHANGELOG.md`, `TODO-gig.md`: maintained by the `rule` subcommand.

## Install

The source lives in the gig repository under `skill/partjob/`. Link it into the agent skill directories:

```bash
ln -s ~/Documents/dev-tools/gig/skill/partjob ~/.agents/skills/partjob
ln -s ~/.agents/skills/partjob ~/.claude/skills/partjob
```

## Tests

```bash
cd ~/Documents/dev-tools/gig/skill/partjob && python3 -m unittest discover -s tests -v
```

The integration tests need the gig v2 binary: `GIG_BIN=/path/to/gig`, default `~/Documents/dev-tools/gig/target/debug/gig`.
