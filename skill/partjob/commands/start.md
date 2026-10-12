# start <slug>

The client placed the order; the job officially begins. Register the order, create the project directory, set up agent rules, then go to grill.

## Preconditions

- From JC: the agreed price (major units, e.g. 800), the client's words confirming the deal (verbatim), the project type (`tool`, `cv_ml`, `data_processing`, `research_writing`, `custom`), the material path (not needed again if the draft has it).
- Ask once, in one message: whether the price is what the client pays or what JC receives after the platform's cut (pass the client's payment as `--price` and JC's share as `--cut-ratio`: 480 received of 800 paid is `--price 800 --cut-ratio 0.6`); whether the client already paid or put the money in escrow; whether the words are the client's own or JC's summary or plan (a summary is passed as words but labelled "(JC's summary)").
- A draft with the same slug exists: use `--from-draft`; its notes land in JOB.md under "Pre-order notes".
- The directory already exists (a project set up by hand earlier): follow "Adopting a project set up by hand" below instead of the steps.

## Steps

1. Pass the client's words with `--client-words "<words>"`; if long, write them to a file under `/tmp` first and pass `--client-words @file`.
2. `gig new <slug> --title "<title>" --price <amount> --type <type> --material <path> [--platform <platform>] --client-words ... [--from-draft]`.
   The result lists `created_files` and `warnings` (git being unavailable shows here). The directory is `~/dev/partjobs/<slug>/` with `.gig/JOB.md`, `.gig/QUOTE.md`, `AGENTS.md`, `README.md`, `.gitignore`, `.gitattributes`, `data/`, `references/`.
   Claude Code only: if the session's working directory is not `~/dev/partjobs/<slug>/`, stop and ask JC to run `/cd ~/dev/partjobs/<slug>`; continue from step 3 after the move. Other agents skip this.
3. Read the generated `.gig/JOB.md` and `.gig/QUOTE.md` and fill each "(to fill)" with a fact JC stated; the rest stays "(to fill)" and goes into one question. The price and words in QUOTE.md must match what JC said. A prepaid order gets "Payment: received YYYY-MM-DD (prepaid)" with the date JC gives; `gig paid` still runs after delivery (see TODO-gig.md).
4. Judge whether the project is large: several modules or deliverables, work expected to span many sessions, or a route that needs CONTEXT.md and ADRs. State the judgement and the reason to JC in one line; JC may overrule. The same judgement picks the grill in step 7.
   Large: in the project directory run the `setup-matt-pocock-skills` skill (issue tracker, triage labels, domain docs). It appends an "Agent skills" section to AGENTS.md; keep the template content.
   Large, in Claude Code: the skill is user-only. Ask JC to run `/mattpocock-skills:setup-matt-pocock-skills` and continue from step 5 when it finishes.
   Small: skip it; task tracking stays in JOB.md "Status".
5. Fill the "Project-specific" section of AGENTS.md for this project type: read-only paths, whether training is allowed and where, local toolchain, whether continuous work is authorised, git authorisation. Leave what is unknown and ask in the grill.
   Mixed work (for example a tool whose core is image recognition): `--type` takes the main type; copy the rules of the other type's section in `templates/AGENTS.<type>.md.j2` that apply (for example from "CV / ML projects": evaluation set and acceptance criteria written before tuning, failure examples kept) into "Project-specific" by hand. Tell JC the main type and what was copied.
6. `gig start --order <slug>`. Status becomes `in_progress`.
7. Tell JC the next step is `grill-me` (small job) or `grill-with-docs` (a job that needs CONTEXT.md and ADRs) to settle the route; in Claude Code these are `/mattpocock-skills:grill-me` and `/mattpocock-skills:grill-with-docs`. Grill conclusions go into JOB.md one by one via `/partjob decide`.
   `research_writing` orders, and any order whose client gave no concrete requirement, grill before any literature search or topic work. Cover: school stage or audience, target journal class, the methods JC is strong in, data policy (may a dataset be built), who collects evidence, what the work adds beyond what frontier models already do, and who the client is. Candidate topics go to `docs/research/topics.md`; only the choice and its constraints become decisions.
8. First commit: the skeleton files. No push.

## Adopting a project set up by hand

For an order that began before this workflow. Show JC the whole list first, each item with its approval class; a yes covers the items as listed.

1. Not in gig yet: `gig new <slug> --title "<title>" --type <type> --adopt --status <state> [--price ...] [--cut-ratio ...]`, which touches no file. Already in gig (a v1 order that `gig doctor` lists as legacy): skip this step; a second `gig new` would add a second order row. (no approval)
2. Render what the project lacks from `templates/`: `.gig/JOB.md`, `.gig/QUOTE.md`, the template sections of `AGENTS.md`, `.gitattributes`, missing `.gitignore` lines. Existing files are merged, never overwritten. (no approval)
3. Copy decisions found in old notes into "Confirmed decisions", each tagged "(migrated from <file>)". The old progress log stays where it is; a JOB.md "Status" line points to it. (no approval)
4. QUOTE.md: only facts JC states (price, cut ratio, payment, warranty); everything else stays "(to fill)". (item 6)
5. Tracked files the workflow keeps out of git (client originals, `.scratch/`): list them; untracking or deleting them is approval item 3.
6. Renames and moves of existing files: list each with its target. (item 3 when anything is removed)
7. Commit the adoption; JOB.md "Status" records what changed.

## Reply

Which files were created, the price and date in QUOTE.md, which "(to fill)" items remain in AGENTS.md, and that the next step is grill.
