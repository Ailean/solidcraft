# photographic-cursor-commit — source checkpoint

**Unapplied, unformatted, uncompiled and unrun. Not a ready implementation or acceptance result.**

Separately repairs the historical cursor-commit ordering around the final cancellation check. Two original local regression declarations distinguish cancellation before progress commit from cancellation after a successful complete step; finish behavior after recognized completion is preserved.

Authorship: Designer, SolidCraft Render team.

## Base and composition

photographic-test-contract candidateb12cc1818b8e1a2ebf791cfe50728f6ba9f6a5eb3f25e74001bffac4d357f6ab.

Dependencies: photographic-test-contract

The exact patch bytes and source before/candidate hashes are in manifest.json.

## Review and open work

Independent source review in progress at preservation time.

- Not the cause of C14 test37.
- The current host propagates step errors and rechecks cancellation before publication; no host leak or real scheduling-race reproduction is claimed.

No C14 result is transferred to these new source bytes. Engineer owns final composition/publication; the sole builder runs actual source-bound validation. Native mouse/keyboard acceptance and long-scene cadence/quality remain separate gates.
