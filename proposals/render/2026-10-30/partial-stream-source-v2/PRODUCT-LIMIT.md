# Long-final-render acceptance remains outstanding

This source-only note applies to the common A2 contract and both producer V1 and the proposed tick-only V2. It does not change their contract or authorize execution.

Every successful optional snapshot charges W=12M+65538 to the SAME finite WorkBudget as tracing and final resolution. At the maximum preview M=1048576, that is12648450 logical units per accepted snapshot. UI requests at a250ms minimum interval are coalesced and subject to safe-frontier/credit availability; that interval is not a measured achieved snapshot rate, throughput or overhead guarantee.

begin checks only remaining>=W. It does not reserve work for remaining rays or final conversion/encoding. WorkBudget::charge only checks and increments used against its fixed limit. The host's separate final lease and original source-sized PNG/RGBA/codec admission reserve MEMORY, not units of rendering work. They prevent a third preview lease from replacing final ownership, but do not assure final completion.

At a target-complete source frontier, the unchanged normal final path still owes its ordinary pass-transition tick, P pixel-resolution ticks plus encoder-setup tick, P encoding-row units plus encoder-finish tick, and the actual PNG-length handoff charge. There is no protected work balance for those remaining operations. Earlier accepted partial snapshots can consume that headroom or prevent the target frontier from ever being reached. The existing exact-W interior test deliberately expects the next real trace step to fail; presentation-exhausted only prevents further optional snapshots and cannot restore earlier spent units.

This is an explicit accepted limitation of the present increment, not a new implementation blocker or evidence of final quality. A later manual-final consumer needs original acceptance on detailed scenes for presentation overhead and actual final completion, with an agreed policy that keeps feedback from exhausting required rendering work. Do not infer that acceptance from static bounds, tiny fixtures, memory reservations, C13 history or a future compile pass. No benchmark, product test or numerical probe was executed here.
