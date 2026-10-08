# preview [version]

After JC is satisfied, show the client evidence, not the product. A preview convinces the client the work is done while being useless on its own.

## Contents (defaults per project type; confirm per order in JOB.md)

- Tool jobs: a few processed results from the client's own samples (before/after comparisons), a GIF or screenshots of the program running.
- Reproduction / analysis jobs: the key figures, the report's summary pages (first one or two pages as images or a separate PDF).
- Writing jobs: the table of contents plus one section.
- Any job: a one-page metrics table.

Never: source code, executables, the full batch of results, the full report, copyable data files. Watermarking is JC's call.

## Preconditions

- JC has tried this version (`/partjob dogfood`) and said it is good enough to preview, or said to skip that round.

## Steps

1. The version defaults to the latest vX.Y.Z in JOB.md "Status", else v1.0.0. The package id is `<slug>-vX.Y.Z-preview`.
2. Candidate first: put the preview files under `.scratch/preview/<package-id>/` with English names and show them to JC. Change them there until JC says this is the version to send. Nothing is registered in gig yet, so a superseded candidate leaves no package record.
3. Move the agreed files to `delivery/<package-id>/` (client-named originals may stay under `results/`, see step 4).
4. `gig package build <package-id> --kind preview --write-manifest`. Client-named files (Chinese, spaces) go in one subdirectory, declared with `--client-named results/`.
5. On failure (`unsafe_package`) fix what the message names: usually hidden files, non-ASCII names outside a `--client-named` directory, key-like files.
6. Show JC `data.files` and `warnings`.
7. Compare `delivery/` with `gig package ls`. Report entries gig does not know (for example a copied `<package-id> (2)` directory); removing them is deleting and needs JC's approval.
8. JOB.md "Status": the preview package id, file count, first 12 characters of the sha256, "not sent out yet".

## Reply

Step 2: the candidate's path and file list, and that JC decides whether it is the version to send. After step 8: the package id, the file list, warnings, unknown entries in `delivery/`, and that the next step is `/partjob send <package-id>`.
