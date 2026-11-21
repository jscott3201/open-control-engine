| Change | Subject | Was | Now |
| --- | --- | --- | --- |
| reparameterized | `damOpen.t` | 0.95 | 0.85 |
| removed | `openLong` | `CDL.Logical.TrueDelay(delayTime = 60, delayOnInit = true)` | |
| renamed (exact) | `setFrac` | `seventySet` | `setFrac` |
| added | `starvedLong` | | `CDL.Logical.TrueDelay(delayTime = 120, delayOnInit = true)` |
| rewired | `want.u1` | `openLong.y` | `damOpen.y` |
| rewired | `want.u2` | `starved.y` | `starvedLong.y` |
| comment-changed | `sequence` |  Zone static-pressure reset request, version 1. |  Zone static-pressure reset request, version 2. |
