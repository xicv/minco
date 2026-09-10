# Base-writer migration fixture (provenance-pinned)

These twenty SQL files are the ticketing migration stream exactly as the
MERGED BASE TREE shipped it, exported verbatim from commit
`d79399108eb802ca2268b1306661dc781a327e3c` (merge of PR #187, the
pre-isolation candidate) via `jj file show -r d7939910`.

The ISO-5 upgrade proof applies THESE files — not the candidate's
stream — to build its pre-isolation database, then upgrades through the
candidate's migrator and composition. This pins the fixture's
provenance: any divergence between the base tree's published schema and
this fixture is a test failure, not an assumption.

Verification (run by the proof): each fixture file is byte-identical to
the base commit's file, and files `0001`–`0020` of the candidate's
stream are byte-identical to their base counterparts — the candidate's
additions (`0021`, `0022`) are strictly forward.

Do not edit these files; re-export from the pinned commit instead.
