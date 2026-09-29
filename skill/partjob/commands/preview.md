# preview [版本]

JC 自己检查满意后, 先给客户看证据, 不给成品. 预览让客户确信活干完了, 但拿到预览也用不了.

## 内容 (按项目类型的默认, 每单在 JOB.md 里确认)

- 工具类: 客户自己样本的少量处理结果 (前后对比图), 程序运行的 GIF 或截图.
- 复现 / 分析类: 关键图表, 报告摘要页 (前 1~2 页导出成图或单独 PDF).
- 写作类: 目录 + 一节正文.
- 通用: 指标表 (一页).

不放: 源码, 可执行程序, 完整批次结果, 完整报告, 可复制的数据文件. 是否加水印由 JC 定.

## 步骤

1. 版本默认取 JOB.md 状态里最新的 vX.Y.Z, 没有就 v1.0.0. package-id 是 `<slug>-vX.Y.Z-preview`.
2. 把预览文件放到 `delivery/<package-id>/`, 文件名全英文 (客户原样本名可保留在 `results/` 下, 见第 4 步).
3. `gig package build <package-id> --kind preview --write-manifest`.
4. 失败 (`unsafe_package`) 就按 message 修: 一般是隐藏文件, 非 ASCII 文件名, 密钥类文件. 客户自己命名的文件放在一个子目录, 在 manifest 加 `client_named = ["results/"]` 后再 `gig package check`.
5. 把 `data.files` 列给 JC 过目, 连同 `warnings`.
6. 在 JOB.md "状态" 记: 预览包 id, 文件数, sha256 前 12 位, "尚未对外发送".

## 回复

包 id, 文件列表, warnings, 下一步是 `/partjob send <package-id>`.
