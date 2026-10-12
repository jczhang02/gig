# TODO-gig

Things that need gig code changes. `/partjob rule` does not edit code; it records them here.

- 2026-10-08 (thermal-video-curve): `gig new --type` accepts several types (for example `tool,cv_ml`) and renders each type's AGENTS section; until then `start` copies the second type's rules by hand.
- 2026-10-12 (retro, codex-design-3d-web): prepaid orders. `gig paid` needs `delivered`, so a prepaid order records a made-up payment date; and `warranty_until` counts from that date, not from the real delivery (for example a remote session). Needs `gig new --prepaid DATE` or `paid` from `in_progress`, plus `gig paid --warranty-start DATE`. Waiting on JC's choice.
- 2026-10-12 (retro, codex-design-3d-web): setup or service orders (configuring a client's tools, delivered in a remote session) fit no project type. A `setup` type would need the gig enum, an AGENTS template, and pack and preview rules. Waiting on JC's choice.
- 2026-10-12 (retro, patent-value-identification): a full delivery sent with `gig artifact upload` leaves the order `in_progress`. `artifact upload` could take `--kind preview|full`, or warn when an `in_progress` order gets a file and no full package.
