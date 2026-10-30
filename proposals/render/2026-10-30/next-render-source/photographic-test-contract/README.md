# photographic-test-contract — source checkpoint

**Unapplied, unformatted, uncompiled and unrun. Not a ready implementation or acceptance result.**

Corrects only the new test that overspecified a Cancelled error from incomplete finish, which historically promises failure but not that exact code. The stronger sequence checks valid pre-cancel construction, one real processed pixel, Cancelled step, no further progress and no finished image. Production, historical tests and declaration count remain unchanged.

Authorship: Designer; independent contract/source review by Engineer review agent.

## Base and composition

Actual C14 post-format photographic.rs4baeb099d56c6310c85bd0931276e41a8aba5f5fbd223956dabe2c0b2c00c061.

Dependencies: No other packet in this folder; the exact source base still applies.

The exact patch bytes and source before/candidate hashes are in manifest.json.

## Review and open work

Independent source review confirms the contract diagnosis. The original C14 failure remains failed and is not rerun by this packet.

- This is not a production fix and does not merely change the expected code to InvalidRequest.
- A separately discovered historical terminal cursor race is addressed only in the following independent packet.

No C14 result is transferred to these new source bytes. Engineer owns final composition/publication; the sole builder runs actual source-bound validation. Native mouse/keyboard acceptance and long-scene cadence/quality remain separate gates.
