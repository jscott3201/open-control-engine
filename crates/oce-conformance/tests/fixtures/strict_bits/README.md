# Strict-bit corpus evidence

`corpus.json` is the authoritative signal inventory and retained **local observation**, not a
Linux qualification result or a replacement oracle. See [the exactness boundary](../../../../../docs/strict-bit-evidence.md).
Capture refuses to overwrite evidence; the historical-corpus refresh switch has been removed.
Tier-A goldens and all original captures remain unchanged.

## Current evidence: the per-PR native matrix

Native Linux evidence is re-established on every pull request. `pr-gate.yml` captures the corpus
in four native cells (x86_64 on `ubuntu-latest`, aarch64 on `ubuntu-24.04-arm`, debug and release),
two independent processes per cell, and its `strict-bit-compare` fan-in runs
`matrix::native_linux_matrix_is_complete_and_exact` over all eight captures: exact agreement with
the Tier-A references and across cells, byte-identical repeats, and a fresh collection from the
same checkout for provenance and inventory. Nothing in this directory admits, skips or relaxes that
check (GitHub #392 retired the digest-bound receipt).

## `receipts.json`: a log, not a gate

- `last_green_native_run` records the last green native matrix — run id, PR head and merge SHAs,
  rustc, libm, the SHA-256 of each cell's first and repeat capture, and the `matrix.json` SHA-256.
  The fan-in job writes exactly this record as `receipt-candidate.json` in its `strict-bit-matrix`
  artifact; updating the log is pasting that file. Today it records run **37382761120**, whose
  captures are retained below, so the test also rebuilds the record from those files.
- `archives` names the two retained capture directories with their run, PR head, recorded merge
  revision and `matrix.json` SHA-256.

The schema is closed. Tests require the logged run to cover exactly the four native cells with
byte-identical first/repeat digests and distinct cells, and refuse the retired `current`, `status`,
`pending` and source-digest fields.

## Retained native archives (history)

`qualified-linux/` holds the eight native captures from Actions run **37382761120** (head
`f8dcaeb861fa18ef4c720b6fa2963b026f3a8300`, synthetic merge checkout
`fbae3b96b053836b916f603a0d86c53d30bce628`): Linux x86_64/aarch64 × debug/release ×
first/repeat, 21 signals and 161 samples each, zero mismatches, rustc 1.97.1, libm 0.2.16, and
the 35-entry selected-source map that admission compared until GitHub #392. Its reconstructed
`matrix.json` SHA-256 is `78c5efaf4881977d306fc1f7a09c51f37cf897be338558d14aee9c2313d035d9`; the
uploaded artifact archive digest was
`sha256:0c94225cc2499e577cb597d0c36cbee138fc88bb65791872f1deb5c38e6b6576`.

`linux/` holds the original eight captures from Actions run **35492290613** (PR head
`dbce73fb20ada4a3a91653bb7ad9b48fae7ee87d`, synthetic merge checkout
`8a63d4a042e1ca91d5dfe7bd3fc33d194f5102bb`) with a 17-entry source map. Its reconstructed
`matrix.json` SHA-256 is `12a2fbdd28c9718c0145c5055240f0b7eab3d7898bc5d1f05ed0ee09db2978ad`; the
uploaded artifact digest was
`sha256:d5b21ca70517c793f46a99d5402d236e2c78494466367fa583ccef27431c31c4`.

`retained_native_archives_reconstruct_their_recorded_matrix_digests` rebuilds both aggregates
byte for byte and checks their digests, revisions, sample counts, reference inventory and zero
mismatches; `native_receipt_refuses_relabeling_and_raw_payload_reblessing` is its hostile control.
No capture bytes, revisions, labels or source maps are rewritten. The source maps are kept
verbatim and are no longer compared with the checkout.
