# status

For the start of a session, or when JC asks "where are we". Read-only; changes nothing.

## Steps

1. `gig ls` and `gig draft ls`. If both fail with `legacy_db` or `config`, stop and report: the environment is broken.
2. Work out the current directory:
   - Inside a project directory (`gig show` succeeds): read `data.order`, `data.next_action`, `data.packages`, then `.gig/JOB.md`, chiefly the "Now" section, the last entries of "Confirmed decisions", the "Client questions" section, and the last two "Status" entries.
   - Not inside a project (for example `~/dev/partjobs`): not an error. Use the `gig ls` result.
3. Inside a project whose decisions name the CodeBuild channel and whose final build is not recorded yet: run `aws sts get-caller-identity --region us-west-2` (read-only, free). An expired session goes under what waits on JC: "run `! aws login`". For the `vm` channel, run `virsh -c qemu:///system domstate win11` instead (read-only); `vm-build.sh` starts a shut-off VM itself, so only a missing domain goes under what waits on JC.
4. From `gig ls` pick out: orders with `unpaid == true` (delivered, not paid, with `days_in_status`), orders whose `next_action` starts with `warranty until`, and `queued` orders.
5. Orders whose project directory has no `.gig/JOB.md` (`gig doctor` lists them): legacy orders already in gig, to be adopted with `start` ("Adopting a project set up by hand").
6. Inside a project: full packages in `gig package ls` that were built but never recorded as sent. Ask JC whether the client got one by hand (send, "Sent by JC").

## Reply

- The current project, if any: slug, status, next step (`next_action`), the date and gist of the last JOB.md status entry, whether "Client questions" has anything unanswered.
- What waits on JC: only what really exists in JOB.md and gig; invent nothing.
- Other orders: unpaid (how many days), in warranty (until when), queued, legacy orders not yet adopted.
- Open drafts, if any.

Do not print the whole JSON. Do not suggest steps outside the workflow.
