# dogfood [version]

JC tries the work before anything goes to the client. Tests show that the code does what the decisions say; only JC's own use shows whether the decisions are right (a chart that looks broken, a theme JC does not want). Every order goes through it once per version that is meant for a preview: tool jobs run the program, other jobs read the figures or the report draft.

## Preconditions

- The version is committed and its tests pass.

## Steps

1. The version defaults to the latest vX.Y.Z in JOB.md "Status".
2. Give JC a way to use it on JC's machine, with paths:
   - Tool jobs: the exact command from the project directory (for example `uv run <program> data/samples/<file>`), or a local build under `.scratch/dogfood/`. Outputs go under `.scratch/dogfood/`, never next to the original materials.
   - Other jobs: the figures or the report draft, as paths JC can open.
3. A checklist of at most eight items, each naming one thing to look at and where, taken from the confirmed decisions and the client's reference (for example "the chart next to the client's slide `data/samples/reference.png`", "curve shape over a gap", "the error message for a video without spot meters").
4. JC's findings, verbatim, go into JOB.md "Status" as "dogfood vX.Y.Z". Each finding that changes what is built becomes a decision through `/partjob decide`; a finding against an existing decision is rework.
5. The round ends when JC says the version is good enough to preview, or that this round is skipped. Record which.

## Reply

How to run or open it, the checklist, and that the next step is JC's findings or `/partjob preview`.
