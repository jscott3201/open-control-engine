| Change | Subject | Was | Now |
| --- | --- | --- | --- |
| param added | `fresh` | | `Real = 2.5` |
| param removed | `gone` | `Real = 1.5` | |
| param reordered | `params` | | |
| param type-changed | `retyped` | `Integer = 3` | `Real = 3.0` |
| param changed | `tuned` | 20 | 21 |
| reordered | `inputs` | | |
| in removed | `uGone` | `Real @unit K` | |
| in added | `uNew` | | `Real @unit K` |
| in type-changed | `uType` | `Real` | `Integer` |
| in unit-changed | `uUnit` | `K` | `degC` |
| in quantity-changed | `uUnit` | `ThermodynamicTemperature` | `Temperature` |
| in display-unit-changed | `uUnit` | `degC` | `K` |
| in min-changed | `uUnit` | 0 | 1 |
| in max-changed | `uUnit` | 10 | 11 |
| reordered (blocks) | `blocks` | | |
| rewired | `both.u2` | `uSecond` | `uFirst` |
| reordered (connections) | `connections` | | |
| parameter-defaulted | `delay.delayOnInit` | false | |
| rewired | `delay.u` | `uFirst` | `uSecond` |
| removed | `dropped` | `CDL.Reals.Sources.Constant(k = gone)` | |
| added | `fresher` | | `CDL.Reals.Sources.Constant(k = fresh)` |
| reordered (assignments) | `limit` | | |
| reparameterized | `limit.uMin` | 0.0 | 0.5 |
| renamed (exact) | `newName` | `oldName` | `newName` |
| literal-respelled | `delay.delayTime` | `60` | `60.0` |
| derived-from-changed | `derived_from` | | `cxf:fnv1a128:0123abcd` |
| literal-respelled | `first` | `1.0` | `1` |
| comment-changed | `gain` |  The gain, before. |  The gain, after. |
| sequence-renamed | `sequence` | `every_kind_base` | `every_kind_head` |
| comment-changed | `sequence` |  Every kind of change, base. |  Every kind of change, head. |
| semantic-changed | `uTags.brick` | `Zone_Air_Temperature_Sensor` | `Zone_Air_Temperature_Setpoint` |
