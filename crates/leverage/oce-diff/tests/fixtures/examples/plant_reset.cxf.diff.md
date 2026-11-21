| Change | Subject | Was | Now |
| --- | --- | --- | --- |
| renamed (exact) | `TMax` | `THigh` | `TMax` |
| renamed (exact) | `TMin` | `TLow` | `TMin` |
| removed | `ena` | `CDL.Logical.And` | |
| removed | `hold` | `CDL.Logical.TrueDelay(delayOnInit = true, delayTime = 300.0)` | |
| added | `onDelay` | | `CDL.Logical.TrueDelay(delayOnInit = true, delayTime = 600.0)` |
| added | `plaOn` | | `CDL.Logical.And` |
| renamed (high) | `sw` | `sel` | `sw` |
| rewired | `sw.u2` | `hold.y` | `onDelay.y` |
