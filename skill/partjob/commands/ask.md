# ask

Something came up that the client has to answer. The agent never contacts the client; it prepares text JC can forward as is.

## Steps

1. One question per item, in the client's language (Chinese for JC's clients), readable without internal terms or reasoning. When there are options, list them with the default.
2. Write them into "Client questions" in `.gig/JOB.md`, each dated. Do not ask what "Confirmed decisions" already answers.
3. For work that can continue while waiting, state which default assumption it continues under and record that in "Status".
4. When JC brings the answer, `/partjob decide` turns it into a decision, and the matching "Client questions" item is marked answered (keep the text, add "answered, see decision N").

## Reply

One block JC can copy and forward whole, plus one sentence on what continues under which assumption meanwhile.
