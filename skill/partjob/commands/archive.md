# archive

售后期结束 (或取消的单), 记分卡, 然后把项目目录移到归档区.

## 步骤

1. 记分卡. 前四个数从 JOB.md 和本单的会话记忆里数, 数不出来就说 0 并注明; 分数问 JC:
   - decisions: JC 在这单里做了几次决定 (JOB.md 已确认决策的条数是下限).
   - repeat_questions: agent 问过几个 JC 之前已经答过的问题.
   - cleanups: JC 手动清理过几次文件.
   - report_reworks: 报告返工过几次.
   - score: JC 给 1~5, 这单顺不顺.
   `gig scorecard --order <slug> --decisions N --repeat-questions N --cleanups N --report-reworks N --score S [--note "..."]`.
   `check_rejections` 和 `days_to_preview` 是 gig 自己算的.
2. 预演: `gig archive --order <slug>`. 看 `blockers` (售后期未到, 缺记分卡, 状态不对), `git_dirty`, `large_files` (>= 50 MB), `unsent_packages`.
3. 把这些列给 JC. 大文件 (checkpoint, 数据) 是否删除, 由 JC 逐项决定; 删除清单要明确, git 历史保留.
4. JC 同意后: 先按批准删文件并 commit; 然后 `gig archive --order <slug> --yes` (售后期未到但 JC 要归档: 加 `--before-warranty-end`; 不想填记分卡: `--no-scorecard`; 整个目录不要了: `--purge`).
5. 归档后目录在 `~/Documents/archive/work/<slug>/` (config 里的 archive_root).

## 回复

记分卡内容, 归档到哪, 删了什么. 记分卡的趋势看 `gig ls --all` 加 `gig show` 里的 scorecard.
