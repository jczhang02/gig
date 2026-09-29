# partjobs 工作流 v2 (第 1 层: 知识与规则)

草稿 2026-09-29, 第 2 版, 按 JC 口述的实际流程重排. 这份文档是整套系统的源头: 新 skill 的正文由它生成, gig v2 只存这里要求记住的东西, 项目模板只生成这里要求存在的文件.

标注:
- [实践] 现有项目 (tk-dtf-compact, bllc-reproduction, sers-colitis-analysis, patent-value-identification) 和 gig.db 里已经在这么做的.
- [JC] JC 口述的做法, 直接采纳.
- [提案] 我的建议, 需要 JC 确认.
- [定] 2026-09-29 已确定的.

## 0. 定位

[JC] 这套流程覆盖接兼职订单的方方面面: 从收到需求到售后期结束. JC 不直接操作 gig, agent 读写 gig. 客户沟通和报价由 JC 在平台上自己做, 流程只记录结果和提供材料.

[提案] 优先级: (1) 项目在多次会话和多个 agent 之间不丢记忆; (2) 交付物安全, 不泄漏客户材料和内部文件; (3) 钱收回来, 售后期过完, 归档; (4) agent 不越权做不可逆或对外的动作.

## 1. 流程

分四段, 段与段之间不是硬性门禁, 只是记录点.

### 1.1 接单前

[JC]
- 接到订单, 拿到客户的初步简短需求 -> 建立临时的, 不可见的工作目录.
- 和客户交流, 确定具体需求.
- 报价, 谈价. 一般根据客户预算报价, 希望先拿到预算.
- 客户同意并下单 -> 这一单正式开始, 临时目录升级为正式目录 `~/dev/partjobs/<slug>/`. 客户不同意 -> 删除临时目录.

[提案]
- 临时目录放 `~/dev/partjobs/.drafts/<slug>/` (隐藏, 不在 partjobs 顶层露出). 里面只允许 `NOTES.md` (客户原话, 材料位置, 疑问, 预算, 报价过程) 和从原件复制的少量样本. 不建 git, 不建 `.gig/`.
- gig 在这一段记一条 `draft`: slug, 材料路径, 创建日期. 升级时这条变成 order; 删除时 `NOTES.md` 导出到 gig 的备注后目录删掉, 留一条 dropped 记录和原因, 以后同类需求可查.
- agent 在这一段能做的: 看材料, 摸底可行性, 整理疑问清单给 JC 去问客户, 估工作量给 JC 参考. 不写代码, 不建正式项目.

### 1.2 开工

[JC]
- 在正式目录里建立 agent 规范, 用 `setup-matt-pocock-skills`.
- 用 `grill-me` 或 `grill-with-docs` 确定具体路线和目标.
- agent 和 JC 正式干活.
- 中途可能有问题要客户确认.

[实践] 正式目录的共同做法:
- 独立 git 仓库, 分支 main. 原件留在原路径只读 (一般 `/mnt/virtiofs/<编号>/`), 样本复制到 `data/` 或 `references/`.
- 客户样本, 含订单号 / 个人信息的图, 交付目录, 构建产物不入库, `.gitignore` 每条注明原因.
- `.gig/JOB.md` 和 `.gig/QUOTE.md` 是仅有的两个流程文件 (第 2 节). 根目录 `AGENTS.md` 写项目专属 agent 规则.
- grilling 的结论写进 JOB.md "已确认决策", 带日期和 "grilling 确认" 字样.

[提案] 升级为正式目录时的第 0 天骨架:

```
<slug>/
  .git/  .gitignore  .python-version
  .gig/JOB.md  .gig/QUOTE.md
  AGENTS.md            # setup-matt-pocock-skills 生成后, 再补第 4 节的项目专属规则
  CONTEXT.md  docs/adr/   # grill-with-docs 产出, 用 grill-me 时不建
  README.md
  data/  references/   # gitignore 其中的客户样本
  src/  tests/         # 按项目类型
```

不再生成: INDEX.html, PLAN.md/PLAN.html, ACCEPTANCE.md, MEASURE.md, progress-log.md, delivery 元数据. 计划和验收记录进 JOB.md.

中途要问客户的问题: agent 整理成一段可直接转发的中文, 放 JOB.md "待客户确认" 小节; JC 问完后把答复记成新决策.

### 1.3 交付

[JC]
- JC 自己检查满意后, 先发一部分能证明工作完成的好产出的预览给客户看. (定义见 5.1)
- 客户满意就收货 -> 发送完整交付内容. (清单见 5.2; JC 的底线: 源码 + 交付报告含安装和使用说明)
- 客户不满意 -> 继续工作.

[实践] 过去的交付内容:
- tk-dtf-compact (工具类): 程序 (Windows exe + Linux 可执行), 使用说明.pdf (Kami, 带截图), 源代码.zip (git archive, 不含 .gig), 本批处理结果. 另有一个 `对比-v1.1.0/` 目录放 19 张前后对比图, 这实际上就是预览.
- bllc-reproduction (复现类): 程序 + 配置, 四份 Kami PDF (最终交付报告, 未复现内容与阻塞分析, 程序差异分析, 代码详细解读), results.
- sers (数据分析类): 改版后的图和分析结果.

[提案] 预览 (见第 5 节讨论) 和完整包的规则:

```
delivery/                          # 整个目录 gitignore
  <package-id>/                    # 只放给客户的文件. package-id 默认 <slug>-vX.Y.Z
    <文件...>
  <package-id>.manifest.toml       # 包外, 不进 zip. version=1, package_id, files=[...] 相对 <package-id>/
  <package-id>.zip                 # 条目与 manifest 一一对应
  preview-<package-id>/            # 预览, 同样有 manifest 和校验, 见第 5 节
```

- `gig package check` 校验 manifest 和 zip: 每项存在且是普通文件, 无绝对路径, 无 `..`, 无软链接, 无隐藏文件, 不含 `.gig/ .git/ .scratch/ data/` 等, zip 条目 == manifest 条目. 规则沿用旧 file-contracts.md 的安全清单.
- 发送渠道 [JC]: 一般用 `gig upload` (上传 OSS, 生成短链); 也可能用 gsconnect 发到手机, JC 再手动转发给客户. 批准由 JC 给, 动作可以由 agent 做: 批准后 agent 跑 upload 或 gsconnect, 把短链或送达结果报给 JC.
- `gig upload` 先重新 check, 再上传 OSS, 生成短链, 记录到 gig. 预览和完整包都走这一套, 类型不同 (preview / full). gsconnect 渠道同样先 check, 发送后在 gig 记一条 channel=phone, 没有远程 URL.
- 上传或发到手机需要 JC 明确批准, 每次单独批. 批准后 agent 执行, 不必等 JC 亲自跑命令.
- 客户不满意回到 1.2, 新决策记入 JOB.md, 完成后用新版本号重新交付.

### 1.4 售后与收款

[JC]
- 客户收到后可能提问题, 自己使用. JC 解答.
- 客户满意 -> JC 收到钱, 这一单完成. 之后还有 15 天售后期.
- 客户不满意 -> 继续修改直到满意.

[提案]
- 收款: `gig paid <slug>` 记日期, 同步 QUOTE.md 付款状态. 售后期 = 付款日 + 15 天, gig 算出来, `gig ls` 列出仍在售后期的单.
- 售后期内的修改: 记进 JOB.md 决策, 状态回 in_progress, 重新交付. 售后期内不新开单.
- 售后期结束: `gig archive <slug>`. 先报告未提交改动, 大文件, 未发送的包; JC 批准后移到 archive_root. 项目内大文件是否清理, 按 bllc 2026-09-27 先例: 列出删除清单, 明确批准, git 历史保留.
- 状态线 (2026-09-29 定):

```
draft -> queued -> in_progress -> delivered -> paid -> archived
  |         |           ^             |
  +-dropped +-cancelled +-------------+ (返工或售后修改)
```

  `cancelled` 可以从 `queued` 或 `in_progress` 进入 (delivered 之后不再取消, 走返工). `draft` 接单前; `queued` 成交已登记未开工; `in_progress` 干活中; `delivered` 完整包已发; `paid` 已收款, 售后期内 (warranty_until = paid_at + 15 天); `archived` 归档. 发预览不改状态, 只在 package 表记一条 kind=preview.

## 2. JOB.md 和 QUOTE.md

[实践] tk 和 bllc 的 JOB.md 是范本:

```
# <标题>
- 项目标识, 材料编号, 原件路径, 项目内副本位置
- 客户原始需求 (原话, 注明来源文件和编码)
- 报价与付款以 QUOTE.md 为准

## 素材事实            客观观察, 不掺判断
## 已确认决策 (日期, 确认方式)   编号列表, 每条是可执行约束. 推翻的不删, 记新决策覆盖
## 待客户确认          agent 写, JC 转发, 答复后转成决策
## 状态                按日期的日志: 做了什么, 验证数字, 交付目录, CI run / commit, 还没做的
```

QUOTE.md: 项目, 材料编号, 接受日期, 币种及总价, 商务状态, 付款状态, 用户原话引用, 未约定事项 (交付日期, 支付节点, 售后期).

[提案]
- JOB.md 是项目记忆. agent 每次开工先读, 每阶段结束必须追加状态. "已确认决策" 只能由 JC 新增覆盖, agent 不改.
- QUOTE.md 是商务快照, 只有 JC 改. 付款状态变化同时跑 `gig paid`.
- 需要 JC 明确批准, 沉默或 "继续" 不算的动作: 对外发送; 推送远程仓库; 删除, 清理, 归档; 使用付费远程资源; 超出已确认决策的范围变更; 改 QUOTE.md.

## 3. 整体要求

[JC]
- 所有报告用 Kami 或 LaTeX, 不保留中间产物. 必须 non ai-slop, 经 sepia 或 humanizer 润色.
- 工作目录和文件名全英文.
- 目录尽可能干净, 无多余文件.

[实践] 与此冲突的现状, 新单要改掉:
- bllc 的 `reports/` 里留了 content.json, html, md, -visual 目录, build 脚本, 与 "不保留中间产物" 相反. 应只留 PDF, 生成脚本和源放 `.scratch/` 或不入库.
- tk 的交付文件名是中文 (使用说明.pdf, 源代码.zip, 程序/). 工作目录内全英文没有异议; 给客户的包内文件名也改英文 (5.3).
- sers 的 `.gig/` 有 194MB checkpoint 和日志. `.gig/` 只放两个 md.

[提案] "目录干净" 落成可检查的规则:
- 项目内允许的顶层项由模板固定, 多出来的目录要在 README 或 AGENTS.md 里说明用途.
- `.scratch/` 是唯一的临时区, gitignore. 阶段结束时清空或归档.
- 报告流程: 源 (Kami content / LaTeX) 在 `.scratch/reports/<name>/` 生成, 经 sepia 润色, 只有最终 PDF 进 `reports/` 或交付包.

## 4. 技术栈与 agent 规范

[实践] 技术栈, 四个项目反复出现的选择:
- Python: uv, pyproject.toml, ruff, pytest, `.python-version`.
- 配置: Hydra + YAML (研究型) 或 ini + 命令行 (给非技术客户的工具). 所有配置项都能被命令行覆盖 (JC 硬性要求).
- 跨平台: Linux 开发验证, 客户多用 Windows. 不用 Windows 专属技术. 交付程序用 PyInstaller onefile, GitHub Actions (windows-latest + ubuntu-latest) 在私有仓库构建, 记 run id 和 commit.
- 文档: 客户文档中文, Kami PDF 带截图; 开发者 README 另写.
- 无本地 Python 的项目用 Node/shell 做本地处理, Python 在远端跑.
- 训练和大计算在远端 GPU 服务器, 环境和缓存放数据盘 (shanhe-computing-server).
- 任务跟踪: 小项目 JOB.md 状态; 大项目 beads 或 setup-matt-pocock-skills 配的 issue tracker.

[JC] 还有一些技术栈和工具倾向要补充 (JC 提到但未展开).

[实践] agent 规范, 从 bllc 和 patent 的 AGENTS.md 和 tk 的 JOB.md 提炼:

通用 (进 skill):
- 开工先读 `.gig/JOB.md`; 价格和付款只信 `.gig/QUOTE.md`.
- 原件只读. 新结果写新目录, 不覆盖; 失败和不可用状态保持可见.
- 不为接近论文或客户期望调数值. 报告数字来自保存的结果, 说明划分, 配置和聚合方式.
- 每阶段结束在 JOB.md 状态里记命令, 验证结果, 剩余工作.
- 交付物可复现: 用交付的程序重跑, 与源码运行逐字节比对, 记入 JOB.md.
- 验证过的工作单元及时 commit; 长实验前先提交代码, 配置, 评测协议. 检查 staged diff, 保留用户并行修改.
- 临时文件, 恢复副本放 `.scratch/`, 不放上级目录.
- 只用现有材料; 要问客户的写进 "待客户确认", 不自行假设.
- 中文正文 ASCII 标点. 具体名称解释概念, 不用未定义缩写.
- 子 agent 交接: 当前状态, 项目绝对路径, 要读的文件, 允许写的范围, 完成标准, 最近的验证命令. 子 agent 不能批准, 发送, 改范围, 删除, 归档.

项目专属 (进项目 AGENTS.md, 按类型生成初稿再补):
- 只读路径和快照目录; 是否允许训练, 在哪跑; 本地工具链; 是否授权连续工作不停顿; 是否授权 git 提交 / 推送.

## 5. 预览与完整包 (2026-09-29 按提案定)

### 5.1 预览

[定] 预览的目的是让客户确信活干完了, 同时客户拿到预览也用不了. 所以:
- 包含: 结果的可视证据 (前后对比图, 截图, 指标表, 报告的前几页或摘要), 客户自己样本的少量处理结果 (tk 的 19 张对比图就是), 短视频或 GIF 演示程序运行.
- 不包含: 源码, 可执行程序, 完整批次的处理结果, 完整报告, 可复制的数据文件.
- 形式: 一个 `preview-<package-id>/` 目录, 同样走 manifest 校验和 OSS 短链, 客户在浏览器看. 图片可加水印 (JC 定).
- 按项目类型的默认预览: 工具类 = 对比图 + 演示 GIF; 复现 / 分析类 = 关键图表 + 报告摘要页; 写作类 = 目录 + 一节正文.

### 5.2 完整交付内容

[定] 按项目类型的默认清单, 具体每单在 JOB.md 里确认:
- 通用: 交付报告 PDF (安装, 使用, 结果, 已知限制, 售后期说明), 源码 (git archive, 不含 .gig / .scratch / 客户样本), README.
- 工具类: 加可执行程序 (Windows + Linux), 本批处理结果, 配置文件样例.
- 复现 / 分析类: 加配置, 结果数据, 图表, 阻塞或差异说明.
- 写作类: 加 PDF 和源文件 (tex / docx).
- 一律不含: 客户原件的副本, 中间产物, 内部审计文件, 测试数据以外的数据.

### 5.3 其他已定 (2026-09-29, 均按提案)
- 状态线见 1.4.
- 交付包内文件名也用英文 (报告内容仍是中文). 理由: 与工作目录规则一致, 且避免 zip 里中文文件名在 Windows 上的编码问题 (tk 的原件 zip 就是 GBK 条目).
- 临时目录 `~/dev/partjobs/.drafts/<slug>/`.
- `delivery/` 放项目根目录, gitignore; package-id 默认 `<slug>-vX.Y.Z`.
- 项目 `AGENTS.md` 由模板按类型生成初稿, 再补项目专属规则.
- 旧库未 promote 的 quote draft 迁为 dropped draft.
- `gig new` 兼做登记和建骨架 (由 `start` 子命令调用).
- skill 改名 `partjob`, 子命令见第 6 节.
- 技术栈: 以第 4 节整理的为准, JC 有补充时用 `/partjob rule` 加.

## 6. skill 子命令

[JC] 这不只是一份流程, 也是一个 skill, 要有真正的子命令; 其中一个子命令是在干活过程中修改这个 workflow skill 本身.

[提案] skill 改名为 `partjob`, 调用形式 `/partjob <子命令> [参数]` (Pi 里是 `/skill:partjob ...`). 参数由 SKILL.md 开头的路由表分发, 每个子命令一个 `commands/<name>.md`, 只在被调用时读. 无参数时等于 `status`.

按流程段:

| 子命令 | 段 | 做什么 |
|---|---|---|
| `status` | 任意 | 读 gig + JOB.md, 报告当前在哪, 下一步, 待 JC 决定的事; 顺带列出未收款和售后期内的单. 会话开始时用 |
| `draft <slug>` | 接单前 | 建 `.drafts/<slug>/NOTES.md`, gig 记 draft. 之后 agent 摸底材料, 整理疑问和工作量估计 |
| `drop <slug> [原因]` | 接单前 | NOTES 存进 gig, 删目录, 记 dropped |
| `start <slug>` | 开工 | draft 升级为 order, 建骨架, 写 JOB.md / QUOTE.md 初稿, 跑 setup-matt-pocock-skills, 提示接下来 grill |
| `decide <内容>` | 开工 | 把 JC 的一条决定写进 "已确认决策", 带日期 |
| `ask` | 开工 | 把待问客户的问题整理成可直接转发的中文, 写进 "待客户确认" |
| `log` | 开工 | 阶段结束, 追加一条状态 (命令, 数字, commit, 剩余) |
| `preview` | 交付 | 按类型默认清单建 `preview-<id>/`, 生成 manifest, `gig package check` |
| `pack [id]` | 交付 | 建完整包, manifest, check. 复现验证 (交付程序重跑比对) 在这里做 |
| `send <id> [--via oss\|phone]` | 交付 | 经 JC 批准后 `gig upload` 或 gsconnect. 报告短链或手机送达 |
| `revise` | 交付 / 售后 | 拿客户反馈对照决策: 返工还是范围变更; 返工记新决策, 范围变更走 `gig change` |
| `paid [日期]` | 售后 | `gig paid`, 更新 QUOTE.md, 算出售后截止日 |
| `archive` | 售后 | 售后期过后, 报告脏状态和删除清单, 批准后 `gig archive` |
| `handoff` | 任意 | 调 handoff skill, 附上 partjob 交接必备项 (状态, 路径, 可写范围, 验证命令) |
| `rule` | 任意 | 修改 skill 本身, 见下 |

`rule` 子命令的机制:
- 用法: `/partjob rule <一句话>`, 例如 `/partjob rule 交付包里的报告只放 PDF`. 也可以无参数, 让 agent 从当前会话里提炼这次踩的坑.
- agent 先判断这条是通用规则 (进 skill) 还是项目专属 (进项目 AGENTS.md), 说明理由; JC 可以改判.
- 通用规则: 定位到 skill 里应放的位置 (SKILL.md 某节或 `references/*.md`), 给出精确 diff, JC 批准后写入, 在 `CHANGELOG.md` 记一行 (日期, 来源项目, 一句话), 跑 skill 自带的契约测试.
- 冲突: 新规则与现有规则矛盾时, 列出矛盾条目, 让 JC 选覆盖还是收窄.
- skill 文件在 dotfiles 管理下, 写入后提示 JC 提交 dotfiles. 改动下次调用生效.
- 只改 skill 的文字和 references, 不改 gig 代码; 需要 gig 改动的记成 `TODO-gig.md` 里的一条.

## 附录 A. gig v2 需要记住的东西

- draft: slug, 材料路径, 创建日期, 结果 (promoted / dropped), 放弃原因, NOTES 快照.
- order: slug, title, material_path, platform, external_id (可空), project_type, status, currency, price, cut_ratio, dev_path, archive_path, notes, 用户原话, 各状态时间戳, paid_at, warranty_until (= paid_at + 15 天).
- requirement_change: order, 描述, 价格增量, 日期.
- package: order, package_id, kind (preview / full), 校验时间和结果, 发送时间, channel (oss / phone), 远程 URL, 短链, 过期时间.
- artifact (包之外单发的文件): 沿用旧 delivery_artifacts.
- 不再需要: quote_drafts 的定价字段, order_workflow, clients, sources, tags, templates.

配置: dev_root, archive_root, drafts_dir, 默认分成, 默认币种, 售后天数, OSS 连接信息. 密钥不进 config.toml, 走环境变量或 keyring.

## 附录 B. 迁移清单

旧库 `~/.local/share/gig/gig.db`:

| 表 | 行数 | 处理 |
|---|---|---|
| orders | 22 | 全部迁移, 每列都有去处: source_org -> platform, notes -> notes, external_id -> external_id, quoted_price/final_price -> price (final 优先, quoted 落到 price_history), client_id/source_id 丢弃 (对应表为空). 时间戳是 epoch 秒字符串, 金额是分, 需转换. #27 的 dev_path 指向已改名的目录, 迁移时修正为 patent-value-identification. |
| price_history | 13 | 迁为 order 备注或独立表 |
| requirement_changes | 5 | 迁移 |
| delivery_packages | 38 | 迁移为 kind=full, 标记本地文件是否仍存在 (sers 的两条已丢失) |
| delivery_artifacts | 40 | 迁移 |
| quote_drafts | 8 | 已 promote 的挂到对应 order 作备注; 未 promote 的迁为 draft (dropped) |
| order_workflow | 9 | 丢弃 |
| clients, sources, tags, order_tags | 0/0/5/4 | 丢弃 |

`~/.config/gig/config.toml` 里的 OSS access key / secret 和短链 token 迁移时改成环境变量, 原文件删除或脱敏.

迁移只涉及数据库和配置. v2 的项目规则只对新单生效, 现有四个项目目录不回改.

## 附录 C. 验收标准 (2026-09-29 与 JC 确定)

### 交付前 (agent 做, JC 看结果)

1. 回放测试: 用 tk-dtf-compact 的原始客户需求和材料, 从 `/partjob draft` 走到 `/partjob pack`. 对照真实 JOB.md 历史: 不问 JC 已答过的问题; 生成的文件只有模板规定的; 交付包内容与当时实际交付一致.
2. 冷启动测试: 在 bllc-reproduction 和 tk-dtf-compact 各开新会话, 只跑 `/partjob status`, agent 说出当前在哪和下一步, JC 无需补充背景即可确认. 这两个项目目前不在 gig 里, 交接时先用 `gig new --adopt` 按各自 QUOTE.md 登记 (tk: CNY 800, 2026-09-28; bllc: 按其 QUOTE.md), 不改动项目文件.
3. 压力用例进自动测试: 包里埋软链接, `.gig/` 文件, 密钥文件, 中文文件名, `..` 路径, 全部被 check 拒绝; 无批准 "继续" 不发送; agent 试图改 QUOTE.md 必须停.
4. 迁移对账: `gig migrate --dry-run` 逐条与旧库 `gig ls --all` 一致, 22 单不少, 金额日期正确.

四条都过才交付.

### 交付后 (每单归档时的记分卡, 存进 gig)

| 指标 | 好的方向 |
|---|---|
| JC 做了几次决定 | 少, 无重复 |
| agent 问了几个 JC 之前答过的问题 | 0 |
| 成交到第一版预览的天数 | 对比历史 |
| JC 手动清理文件的次数 | 0 |
| 交付包被 check 拒绝的次数与原因 | 拒的都该拒 |
| 报告返工次数 | 趋近 0 |
| JC 主观评分 1-5 | 上升 |

`/partjob archive` 提示填写; `/partjob rule` 的改动进 CHANGELOG, 可以看出流程是在变好还是变复杂.
