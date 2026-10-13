# TODO-gig

Things that need gig code changes. `/partjob rule` does not edit code; it records them here.

- 2026-10-08 (thermal-video-curve): `gig new --type` accepts several types (for example `tool,cv_ml`) and renders each type's AGENTS section; until then `start` copies the second type's rules by hand.
- 2026-10-12 (retro, patent-value-identification): a full delivery sent with `gig artifact upload` leaves the order `in_progress`. `artifact upload` could take `--kind preview|full`, or warn when an `in_progress` order gets a file and no full package.
