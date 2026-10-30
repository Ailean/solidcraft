# Actual CPU Interactive Area/LED end-to-end cases

Source-only checkpoint of two original tests, appended to the existing UI test file. These use the actual snapshot, CPU host, reply PNG and receiver cache rather than injected images. One covers real Area/LED pixels reaching Interactive without final-export ownership. The other holds already-completed real replies across a light edit and final-job preemption, requiring stale replies to be discarded without erasing the final job. It does not prove cancellation of an in-flight render or long-render convergence.

Independently reviewed and applied locally before the current consolidated C12 validation. No compilation or test result for these two cases is claimed by this archive. The patch precedes the validation formatter. Actual source/artifact-bound results must be reported separately. Full CI, fresh desktop acceptance, GPU and general glass parity remain pending.
