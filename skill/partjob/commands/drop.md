# drop <slug> [reason]

The client declined the quote, or the deal fell through. Notes go into gig; the directory is removed.

## Steps

1. Ask for the reason if none was given. It is stored in gig so similar requests can be looked up later.
2. `gig draft drop <slug> --reason "<reason>"`. This is a rehearsal; it returns `would_delete`.
3. List the files that would be deleted for JC. This is a deletion; JC must explicitly agree.
4. After JC agrees: `gig draft drop <slug> --reason "<reason>" --yes`. The returned `draft.notes_snapshot` is the saved note.

## Reply

What was deleted, and that the notes are kept in gig (`gig draft ls --all` finds them). Nothing else to do.
