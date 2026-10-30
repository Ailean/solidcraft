# GPU diagnostic precision

Applied locally, unformatted/uncompiled/unrun. The early session-backend refusal now says Vulkan has not been tested; it does not claim the driver is missing. Failed host reasons reach Check/Start readiness; pending/unstarted states retain the generic reason. Backend guards, discovery, device initialization, ErrorCode and CPU/Raster routing remain intact. No pending Graphics API trial is advertised.

Three original headless tests cover two specific failures, pending states without starting setup, and unchanged Raster success/cooperative-CPU refusal. The current208-test prefix is preserved; these add3 declarations. Independent source review closed a bad cooperative-host fixture and a test-only unreachable macro before adoption. They are not runtime passes.

Karl's boundary and our response are recorded in #4016 (issuecomment-6512046016 and issuecomment-6521173494); Nathan's shared-seam note remains in #8000 (issuecomment-6507877408). No physical GPU is initialized or qualified by this change.
