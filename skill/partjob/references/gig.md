# gig 速查 (v2)

完整规格在 gig 仓库 `docs/v2/SPEC.md`. 这里只列 agent 每天要用的.

## 约定

- 每条命令 stdout 只有一个 JSON: `{"ok": true, "command": "...", "data": {...}, "warnings": [...]}` 或 `{"ok": false, "command": "...", "error": {"code": "...", "message": "..."}}`. 退出码 0 成功, 1 业务错误, 2 用法错误. 只有 `gig completion` 输出原文.
- 指定订单: 任何命令都接受 `--order <slug>`; 在项目目录内运行可以省略.
- 不可逆或对外的命令必须带 `--yes`, 否则只是预演 (`"dry_run": true`): `draft drop`, `package upload`, `package sent`, `artifact upload`, `archive`, `cancel`, `delete`.
- 金额用主单位小数, 最多两位 (`800`, `800.50`). 日期 `YYYY-MM-DD`.
- 错误码: `not_found`, `invalid_state`, `invalid_input`, `unsafe_package`, `needs_check`, `needs_yes`, `config`, `secrets`, `upload`, `legacy_db`, `io`, `db`.
- 状态线: `queued -> in_progress -> delivered -> paid -> archived`, 另有 `cancelled`. 预览不改状态. 售后期内 (`paid`) 仍可发包.

## 命令

```
gig draft new <slug> [--title T] [--material PATH] [--type tool|cv_ml|data_processing|research_writing|custom]
gig draft ls [--all]
gig draft drop <slug> --reason TEXT [--yes]

gig new <slug> --title T [--price 800] [--type ...] [--material PATH] [--platform P] [--external-id X]
        [--client-words TEXT|@FILE|-] [--from-draft] [--adopt [--status queued|in_progress|delivered|paid]] [--no-scaffold]
gig ls [--all]
gig show [--order <slug>]
gig start [--order <slug>]
gig change [--order <slug>] --desc TEXT [--price-delta 200]
gig price [--order <slug>] --amount 1000 --reason TEXT
gig note [--order <slug>] TEXT
gig paid [--order <slug>] [--date YYYY-MM-DD] [--amount 800]
gig scorecard [--order <slug>] --decisions N --repeat-questions N --cleanups N --report-reworks N --score 1..5 [--note TEXT]
gig archive [--order <slug>] [--yes] [--before-warranty-end] [--no-scorecard] [--purge]
gig cancel [--order <slug>] --reason TEXT [--yes]
gig cd [--order <slug>]

gig package build <package-id> [--order <slug>] [--kind full|preview] [--write-manifest [--client-named results/]...]
gig package check <package-id> [--order <slug>]
gig package upload <package-id> [--order <slug>] [--yes]
gig package sent <package-id> [--order <slug>] --channel phone|other [--note TEXT] [--yes]
gig package ls [--order <slug>]

gig artifact upload <FILE> [--order <slug>] [--yes]
gig artifact ls [--order <slug>]

gig delete <slug> --yes                      只删记录, 不动文件, 只用于登记错了

gig doctor [--fix]
gig config get KEY
gig config set KEY VALUE
gig config path
gig config split-secrets [--yes]
gig migrate --from OLD.db [--to NEW.db] [--dry-run] [--fix-path OLD=NEW]
gig backup
gig completion zsh                           唯一输出原文的命令
gig version
```

## 交付包布局

```
<项目>/delivery/<package-id>/              只放给客户的文件
<项目>/delivery/<package-id>.manifest.toml  白名单, 在包外
<项目>/delivery/<package-id>.zip            条目与 manifest 完全一致
```

`package-id` 默认 `<slug>-vX.Y.Z`, 预览用 `<slug>-vX.Y.Z-preview`. 文件名规则: ASCII 字母数字 `. _ - /`, 不能有隐藏文件, `..`, 软链接, 密钥类文件, `.gig/ .git/ .scratch/ internal/ prompts/`. 客户自己命名的文件 (批处理结果) 可在 manifest 里用 `client_named = ["results/"]` 豁免命名规则, check 会把每个豁免列成 warning.

`build --write-manifest` 从目录生成 manifest 并打 zip; 客户命名的文件所在目录用 `--client-named results/` 一起给, 会写进 manifest. 之后改了包内容要重新 `build` 或 `check`, 否则 `upload` / `sent` 报 `needs_check`.
