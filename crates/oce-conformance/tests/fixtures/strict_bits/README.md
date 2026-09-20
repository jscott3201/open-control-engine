# Strict-bit corpus evidence

`corpus.json` is the authoritative signal inventory and retained **local observation**, not a
Linux qualification result or a replacement oracle. See [the exactness boundary](../../../../../docs/strict-bit-evidence.md).
Capture refuses to overwrite evidence; the historical-corpus refresh switch has been removed.
Tier-A goldens and all original captures remain unchanged.

## Current qualification

`qualified-linux/` retains the eight canonical native captures from Actions run **35494403523**:
Linux x86_64/aarch64 × debug/release × first/repeat. Each contains all 21 signals and 161
samples, plus the complete **35-source map**; there are zero mismatches and every first/repeat
pair is byte-identical. rustc is 1.97.1 and libm is 0.2.16. All files preserve the observed
synthetic PR merge checkout `d16d69a49857ea9abd35a12643e83139ca5c6a6f` for run head
`d57e946126af76ed58ff10cef1e35ffbb627832f`. `receipts.json.current` pins that revision and
the reconstructed `matrix.json` SHA-256
`3f6f9efe8e4f9980a790c1a0d8eba1143111a6ae9138f0d486cca841aa19b7fc`.
The uploaded assembled artifact archive digest is
`sha256:fc1a658c71f1b372767aa576bc5100b74c1dd697b4f63f3394fa8bc70d18e426`.

The ordinary retained-qualification test now validates the admitted receipt, all raw samples,
native topology, repeats and current source/oracle/input/CXF integrity. No bound source changed
for admission. The collection run's required admission checks were red before the receipt was
present; its successful capture/cross-cell comparison is not a claim of final green hosted gates.
The exact Linux result remains limited to this pinned corpus, not macOS, arbitrary inputs,
mathematical correctness, whole executables or Open Control Sim.

## Historical evidence

`linux/` retains the original eight captures from Actions run **35492290613**, with the same
four cells, two runs, 21 signals, 161 samples and zero mismatches, but only 17 source digests.
Its files preserve the synthetic PR merge checkout
`8a63d4a042e1ca91d5dfe7bd3fc33d194f5102bb` for PR head
`dbce73fb20ada4a3a91653bb7ad9b48fae7ee87d`. Their candidate labels are capture-time provenance;
neither those labels nor any historical capture bytes are rewritten by current admission.

The historical receipt test reconstructs the original `matrix.json` byte-for-byte and checks SHA-256
`12a2fbdd28c9718c0145c5055240f0b7eab3d7898bc5d1f05ed0ee09db2978ad` rather than retaining a
ninth duplicate of all raw samples. The uploaded assembled artifact digest is
`sha256:d5b21ca70517c793f46a99d5402d236e2c78494466367fa583ccef27431c31c4`.
These distinct digests are not interchangeable. This historical receipt has an incomplete
17-source map and cannot qualify the strengthened checker; the current 35-source receipt does.
Later HEAD equality is deliberately not required; no source-map exception or rewritten provenance
is used.

## Admission boundary

The ignored `admit_downloaded_native_matrix_without_rewriting_provenance` test is the explicit
local admission path used for the current captures. After reviewing a native receipt into
`receipts.json.current`, with `OCE_STRICT_MATRIX_DIR` naming the downloaded artifact directory,
it checks that exact receipt,
the complete current sources and all eight input files before creating `qualified-linux/`. It refuses CI and
an existing destination; it neither reruns Linux on the local host nor regenerates oracle values.
Ordinary tests never write these files. A future evidence update needs deliberate review of the
new native result and pins, not an automatic refresh. Expected Git/matrix hashes live only in
terminal receipt data, outside bound checker source; capture and admission never hash their own
future receipt. Final admission changes data/docs only, so it cannot invalidate its own source map.
