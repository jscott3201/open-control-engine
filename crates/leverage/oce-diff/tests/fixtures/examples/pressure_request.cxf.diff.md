| Change | Subject | Was | Now |
| --- | --- | --- | --- |
| reparameterized | `damOpen.t` | 0.95 | 0.85 |
| removed | `openLong` | `CDL.Logical.TrueDelay(delayOnInit = true, delayTime = 60.0)` | |
| renamed (exact) | `setFrac` | `seventySet` | `setFrac` |
| added | `starvedLong` | | `CDL.Logical.TrueDelay(delayOnInit = true, delayTime = 120.0)` |
| rewired | `want.u1` | `openLong.y` | `damOpen.y` |
| rewired | `want.u2` | `starved.y` | `starvedLong.y` |
