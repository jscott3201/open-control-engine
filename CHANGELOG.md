# Changelog

## Unreleased

- Reject unsafe block parameter values at load time: missing required `SampleTrigger.period`,
  non-positive timing/window parameters, and inverted `Reals.Limiter` bounds now fail validation;
  equal Limiter bounds produce a warning.
- Surface registry-derived static parameter bounds through `ParamAttrs` and reject out-of-range
  tune-at-rest edits through `Engine::set_param`.
