# Strict-bit corpus evidence

`corpus.json` is the authoritative signal inventory and retained **local observation**, not a
Linux qualification result or a replacement oracle. See [the exactness boundary](../../../../../docs/strict-bit-evidence.md).
Ordinary capture refuses to overwrite evidence. An explicit local-only refresh can replace this
observation, never Tier-A goldens; CI cannot use that refresh facility.

`linux/` retains the eight canonical native captures from Actions run **35492290613**:
Linux x86_64/aarch64 × debug/release × first/repeat. Each contains all 21 signals and 161
samples; there are zero mismatches and every first/repeat pair is byte-identical. rustc is
1.97.1 and libm is 0.2.16. All files preserve the observed synthetic PR merge checkout
`8a63d4a042e1ca91d5dfe7bd3fc33d194f5102bb` for PR head
`dbce73fb20ada4a3a91653bb7ad9b48fae7ee87d`. Their candidate labels are capture-time provenance;
the accepted receipt qualifies only these Linux corpus cases, not macOS, arbitrary inputs,
mathematical correctness or Open Control Sim.

The active `retained_native_linux_evidence_is_complete_exact_and_source_bound` test validates
the schema, complete matrix, raw repeat bytes, numerical agreement and current source/oracle/CXF
digests. It reconstructs the original `matrix.json` byte-for-byte and checks SHA-256
`12a2fbdd28c9718c0145c5055240f0b7eab3d7898bc5d1f05ed0ee09db2978ad` rather than retaining a
ninth duplicate of all raw samples. The uploaded assembled artifact digest is
`sha256:d5b21ca70517c793f46a99d5402d236e2c78494466367fa583ccef27431c31c4`.
These distinct digests are not interchangeable. Later HEAD equality is deliberately not required;
no bound-source exceptions or rewritten provenance are used.

The ignored `admit_downloaded_native_matrix_without_rewriting_provenance` test is the explicit
local admission path. With `OCE_STRICT_MATRIX_DIR` naming the downloaded artifact directory, it
checks that exact receipt and all eight input files before creating `linux/`. It refuses CI and
an existing destination; it neither reruns Linux on the local host nor regenerates oracle values.
Ordinary tests never write these files. A future evidence update needs deliberate review of the
new native result and pins, not an automatic refresh.
