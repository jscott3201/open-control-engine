# Changelog

## Unreleased

- `CDL.Reals.Limiter` (and the PID-family output clamp, which upstream wires through a Limiter
  instance) now follows the upstream comparison chain exactly: a NaN input passes through to `y`
  fail-visible (canonicalized) instead of being absorbed into `uMin`, and boundary-equal inputs
  return their own bits (including zero sign).
- Parameters that upstream declares with no default are now required at load time
  (`Round.n`, `AddParameter.p`, `MultiplyByParameter.k`, `Sources.Constant.k` (Real, Integer, and
  Logical), `Integers.AddParameter.p`, `Limiter.uMax`/`uMin`, `Hysteresis.uLow`/`uHigh`,
  `LimitSlewRate.raisingSlewRate`, `MovingAverage.delta`, `Logical.TrueDelay.delayTime`,
  `Logical.TrueFalseHold.trueHoldDuration`, `Utilities.Assert.message`); omitting one previously
  fell through to a silent engine default.
- PID/PIDWithReset range validation now matches the upstream `min=100*Constants.eps` annotations
  on `k`, `Ti`, `Td`, `r`, `Ni`, `Nd` (inclusive floor), and the `yMin`/`yMax` pair is validated
  like `Limiter` bounds (error on inversion, warning on equality). `Hysteresis` validates
  `uLow <= uHigh` the same way. The equal-bounds warning is block-agnostic — it reports that the
  bounded interval collapses to a single value, rather than always naming the `Limiter`.
- `Engine::get_output` and `CollectSpec::Named` resolve OUTPUT connectors only; naming an input
  point returns `OcError::UnknownPoint` instead of reading the staged input value (back-record
  of an earlier contract narrowing).
- Reject unsafe block parameter values at load time: missing required `SampleTrigger.period`,
  non-positive timing/window parameters, and inverted `Reals.Limiter` bounds now fail validation;
  equal Limiter bounds produce a warning.
- Surface registry-derived static parameter bounds through `ParamAttrs` and reject out-of-range
  tune-at-rest edits through `Engine::set_param`.
