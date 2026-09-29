# log

At the end of a phase, or before a session ends, append one entry to "Status" in `.gig/JOB.md`. This is the project's memory: facts, not impressions.

## Each entry has

- The date (several entries on one day are marked "(cont.)").
- What was done, in one sentence.
- Verification results: concrete numbers (for example "real 19/19, synthetic 100/100, silent errors 0"), evaluation file paths.
- Commit hash, CI run id when there is one.
- Where the outputs are: result directories, delivery directory.
- What is not done or not verified (for example "Windows interaction not tested on a real desktop").
- Whether anything was sent out (default "not sent out yet").

## Also

- Anything worth remembering across orders (a platform habit, a pitfall in a type of client material) goes to gig with `gig note "<one sentence>"`, where `status` will show it later.
- List the commits made during the phase; explain any uncommitted changes.

## Reply

The status entry as appended.
