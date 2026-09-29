# pack [版本]

客户收货后发的完整交付包.

## 内容 (按项目类型的默认, 每单在 JOB.md 里确认)

- 通用: 交付报告 PDF (安装, 使用, 结果, 已知限制, 售后期说明), 源码 (`git archive`, 不含 `.gig/ .scratch/` 和客户样本), README.
- 工具类加: 可执行程序 (Windows + Linux), 本批处理结果, 配置文件样例.
- 复现 / 分析类加: 配置, 结果数据, 图表, 差异或阻塞说明.
- 写作类加: PDF + 源文件.
- 一律不含: 客户原件的副本, 中间产物 (content.json, html, md, -visual 目录, 构建脚本), 内部审计文件, 测试数据以外的数据.

## 步骤

1. 版本: 第一次交付 v1.0.0; 返工后升 minor 或 patch. package-id 是 `<slug>-vX.Y.Z`. 上一版的包目录是否删除由 JC 定.
2. 复现验证 (交付前必做, 结果进 JOB.md):
   - 工具类: 用交付目录里的程序 (CI 构建的那个) 重跑本批输入, 与源码运行结果逐字节比对.
   - 复现 / 分析类: 用交付的配置重跑一遍关键结果, 与报告里的数字一致.
   - Windows 程序只在 CI 上冒烟测试过的, 在 JOB.md 和交付报告的已知限制里写明.
3. 报告: 源在 `.scratch/reports/<name>/`, 用 Kami 或 LaTeX 生成, 成文经 sepia 润色, 只把 PDF 放进包. 报告不宣称流程状态或批准.
4. 源码: `git archive --format=zip -o delivery/<package-id>/source.zip HEAD`, 确认 `.gitignore` 已排除客户样本; 用 `unzip -l` 抽查没有 `.gig/`.
5. 文件名全英文: `manual.pdf`, `report.pdf`, `source.zip`, `program/<name>.exe`, `program/<name>` (Linux), `results/`.
6. `gig package build <package-id> --write-manifest [--client-named results/]`. 客户命名的结果文件 (中文, 空格) 放在 `results/` 这类子目录里, 用 `--client-named` 声明; check 会把每个豁免文件列成 warning, 过目时确认它们确实是客户自己的文件名.
7. 把 `data.files` 和 `warnings` 列给 JC 过目. 包里每个文件都要能说出为什么给客户.
8. JOB.md "状态": 包 id, 文件数, sha256 前 12 位, 复现验证结果, "尚未对外发送".

## 回复

包 id, 文件列表, 验证结果, warnings, 下一步是 `/partjob send <package-id>`.
