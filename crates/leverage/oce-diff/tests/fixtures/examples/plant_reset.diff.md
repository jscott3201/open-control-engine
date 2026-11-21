| Change | Subject | Was | Now |
| --- | --- | --- | --- |
| renamed (exact) | `TMax` | `THigh` | `TMax` |
| renamed (exact) | `TMin` | `TLow` | `TMin` |
| removed | `ena` | `CDL.Logical.And` | |
| removed | `hold` | `CDL.Logical.TrueDelay(delayTime = 300, delayOnInit = true)` | |
| added | `onDelay` | | `CDL.Logical.TrueDelay(delayTime = 600, delayOnInit = true)` |
| added | `plaOn` | | `CDL.Logical.And` |
| renamed (high) | `sw` | `sel` | `sw` |
| rewired | `sw.u2` | `hold.y` | `onDelay.y` |
| comment-changed | `sequence` |  Chilled-water supply temperature reset, reference. |  agent draft |
