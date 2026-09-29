# preview [version]

After JC is satisfied, show the client evidence, not the product. A preview convinces the client the work is done while being useless on its own.

## Contents (defaults per project type; confirm per order in JOB.md)

- Tool jobs: a few processed results from the client's own samples (before/after comparisons), a GIF or screenshots of the program running.
- Reproduction / analysis jobs: the key figures, the report's summary pages (first one or two pages as images or a separate PDF).
- Writing jobs: the table of contents plus one section.
- Any job: a one-page metrics table.

Never: source code, executables, the full batch of results, the full report, copyable data files. Watermarking is JC's call.

## Steps

1. The version defaults to the latest vX.Y.Z in JOB.md "Status", else v1.0.0. The package id is `<slug>-vX.Y.Z-preview`.
2. Put the preview files under `delivery/<package-id>/` with English names (client-named originals may stay under `results/`, see step 3).
3. `gig package build <package-id> --kind preview --write-manifest`. Client-named files (Chinese, spaces) go in one subdirectory, declared with `--client-named results/`.
4. On failure (`unsafe_package`) fix what the message names: usually hidden files, non-ASCII names outside a `--client-named` directory, key-like files.
5. Show JC `data.files` and `warnings`.
6. JOB.md "Status": the preview package id, file count, first 12 characters of the sha256, "not sent out yet".

## Reply

The package id, the file list, warnings, and that the next step is `/partjob send <package-id>`.
