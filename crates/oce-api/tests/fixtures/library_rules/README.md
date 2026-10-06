# open-control-library fault-rule fixtures

Byte-for-byte copies of two published fault rules from
[open-control-library](https://github.com/jscott3201/open-control-library), used as inputs by
`crates/oce-api/tests/library_rule_state_continuation.rs`. They are **data**, never edited here: a
rule change goes to the Library as its own pull request, then a fresh copy lands here with this
table updated.

| Fixture | Library source path | What the state holds | SHA-256 |
| --- | --- | --- | --- |
| `AHU-0016/rule.cxf.jsonld` | `faults/ahu/AHU-0016/rule.cxf.jsonld` | `Logical.TrueDelay` persistence timer, 900 s | `7e89fbf9541a6818b53811fc45015760d9c7b655ffc2f5944c36f4e0cafd720d` |
| `AHU-0016/vectors.json` | `faults/ahu/AHU-0016/vectors.json` | 7 scenarios, 60 s step, 1800 s horizon | `df871812b19add3b1f632bd9cefedc47c609999da778389942cf592c97406ce6` |
| `AHU-0004/rule.cxf.jsonld` | `faults/ahu/AHU-0004/rule.cxf.jsonld` | `Integers.Change` previous value, `Reals.MovingAverage` 3600 s window, `Logical.TrueDelay` 3600 s | `e4ecb7e3c9621ac181139a7ccbae05f7891a40f6e6796351128ac2435146a3ea` |
| `AHU-0004/vectors.json` | `faults/ahu/AHU-0004/vectors.json` | 9 scenarios, 300 s step, 14400 s horizon | `e66662964e18060e51a116ebd47f15fd2d304957e8674a9a67547ef1ec54e2f5` |

- **Library commit:** `d90f63b1decb97828b493c5c77d9a463890da7fc` (2026-09-29).
- **Library `ENGINE_PIN` at that commit:** `e2ff2f84577d9be65a49e6cb5440c223f6126817`. Both rule
  cards record `verified.engine_rev: e2ff2f8`. That revision predates the complete-frame facade
  (#325), so the Library verifier's `Engine::tick` / `set_input` driver does not build against this
  tree. The test drives the same zero-order-hold schedule through `prepare_frame` /
  `execute_frame` and re-checks every Library expectation window on the uninterrupted run.
- **License:** open-control-library is dual-licensed MIT OR Apache-2.0, the same terms as this
  repository. The copies keep those terms.

Each rule's `card.md` (parameters, preconditions, deviations) stays in the Library and is not
copied; the CXF graph and vectors are the only inputs the test reads.
