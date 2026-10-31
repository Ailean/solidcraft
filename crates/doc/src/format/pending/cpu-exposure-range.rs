//! Saved CPU render setups accept finite exposure from -32 through32 EV, matching the
//! existing RenderSetup/UI range. Exposure is still saved only on RenderSetup, applied
//! once after linear accumulation, with default0 unchanged. Existing valid designs keep
//! their numerical intent; no migration/clamping or second exposure field is introduced.
//! The last release refuses this newer design format and requests an update instead of
//! accepting a CPU setup whose wider exposure it cannot execute.
