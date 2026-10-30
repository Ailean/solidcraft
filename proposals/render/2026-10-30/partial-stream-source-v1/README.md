# Actual partial-film producer and host - source-only proposal

Written source, not applied, formatted, compiled or run. The receiver/engine/UI consumer is still being composed privately and is NOT in this packet. This is not an enabled feature, native acceptance, physical GPU acceptance, FPS claim or full CPU/GPU parity.

Apply dependencies only after review: the complete current nested-certified-glass V2 plus Engine rebind comes first (the adjacent cpu-nested-boxes-v2-source-review packet). Producer image.rs expects its private candidate37257990; host.rs expects6ecc923f, rather than the earlier production files. Then the producer-A1-A2.patch contains BOTH original A1 film access and A2 producer; do not additionally apply the separate earlier A1 patch. The host.patch binds the same interface. No application of this packet is authorized by this archive.

The source implements actual film representatives with per-cell sample counts, Unknown alpha0 versus real sampled black/miss, one existing RGBA scratch/encoder and checked work prepayment. Host retains at most TWO total payload slots including in-flight encoding, coalesces demand, freezes producer progress through hashing and keeps buffers billed until their last owning Arc dies. Old whole-pass APIs remain separate. Static host review found no host-local lifetime/slot blocker; complete producer+host memory/work review is still pending. Actual Rust layout guards, allocation capacities and test assertions are UNEXECUTED.

Producer includes5 original A1 and8 original producer declarations; host includes8 original appended declarations. Old test prefixes and numerical expectations are preserved. Tests distinguish original real CPU producer paths from explicitly synthetic lease fixtures. Neither written test bodies nor text patch reconstruction is an observed test pass.

The checked W=12M+65538 optional snapshot debit uses the existing cumulative budget, with no reset or refund. Optional work can consume later trace headroom; this source does not promise unchanged historical work or unconditional completion at an exhausted work cap. A detailed-scene acceptance must measure presentation overhead and final completion honestly.

Interactive receiver authority, stale-job rejection, native display/Unknown UX and manual-final progressive display remain separate requirements. Progressive glass remains explicitly unsupported in this source; nested V2 glass only covers its certified fixed-final opt-in domain. General glass, material-library routing, indirect transport and finishing/export parity are not certified here.

Tracked in https://github.com/tangency-labs/solidcraft/issues/6201
