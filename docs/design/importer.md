# Importer (`oce-author`)

**Design note, written before the importer PR (GitHub issue #483).** It fixes how library CXF in the
flat dialect and the frozen prototype's v0 graph JSON enter the [authoring model](authoring-layer.md),
and it settles the model decisions the minimal importer (GitHub #473) could not make: sequence
parameters of an enumeration type, bounds on boundary declarations, and library constants as block
parameter values. Every decision below was made while drafting and ruled on in review (see
[Review rulings](#review-rulings)); the note is amended to match. The model changes landed in GitHub #493 (issue #492) and the importer in GitHub #498 (issue #496), 2026-11-09; see [As landed](#as-landed). The authoring note's decisions
and review rulings are not reopened; where this note changes the model it says so, and the model
PR amends the authoring note with dated rulings.

The work plan's M2 item 5:

> Importer: library CXF and the prototype's v0 graph JSON into the model (own design note first).

Gate item 1 depends on it: every library rule inside the M2 authoring subset imports, emits and
loads with the same canonical content id as its committed CXF.

## What the importer is, and is not

It is **two pure functions in `oce-author`**, one per source form, each returning a `Sequence` in
the source's own order plus a report of what was read and not carried:

- `import_cxf(bytes, &ImportOptions) -> Result<Imported, ImportError>`: one flat, single-root CXF
  document, the dialect the [composite subset](../cxf-composite-subset.md) calls Rule 2's simplest
  accepted form and the library already uses.
- `import_graph_v0(bytes, &ImportOptions) -> Result<Imported, ImportError>`: one document in the
  frozen prototype's compact block-graph JSON (v0).

It is **not** a validator and not an evaluator. It decodes structure, resolves the names a mapping
needs (array templates, member paths) through `oce-api`, and refuses with a typed error anything it
cannot carry exactly. Whether a class is authorable, a name is reserved, an assignment is in range or
the graph has a loop is the [validator's](authoring-layer.md#validator) judgement, reported in the
author's terms after import. It never evaluates an expression, never fills a default, never renames,
and never drops an attribute the engine reads.

```rust
pub struct ImportOptions { pub derived_from: Option<ProvenanceId> }   // default: none
pub struct Imported { pub sequence: Sequence, pub report: ImportReport }
pub struct ImportReport { pub source: SourceForm, pub dropped: Vec<Dropped> }
pub enum SourceForm { Cxf, GraphV0 }
pub struct Dropped { pub subject: String, pub property: DroppedProperty }  // closed enum, below
```

## Where the corpus stands

GitHub #473 lists each refused fixture by its **first** refusal. A scan of every construct in each of
the 46 G36 fixtures (not only the first) gives a different picture, and it moves the gate's
denominator, so it is recorded here:

| Group | Fixtures | Needs |
| --- | --- | --- |
| Round-trip today | 4 | nothing: `cooling_only_system_requests`, `generic_time_suppression`, `multizone_vav_relief_fan_group`, `reheat_overrides` |
| Boundary bounds, CDL enumeration parameter, parameter bounds | 6 | decisions 1, 4 and 9 only: `thermal_zones_control_loops`, `multizone_vav_return_fan_airflow_tracking`, `multizone_vav_return_fan_direct_pressure`, `multizone_vav_economizer_modulations_reliefs`, `multizone_vav_economizer_modulations_return_fan`, `multizone_vav_relief_damper` |
| G36 enumeration parameter or G36 constant | 21 | decisions 1, 4 and 10 with the widened #468: the twelve `generic_air_economizer_high_limits_*`, `cooling_only_active_air_flow`, `cooling_only_alarms`, `cooling_only_dampers`, `multizone_vav_economizer_enable`, `multizone_vav_economizer_limits_common`, `multizone_vav_freeze_protection`, `multizone_vav_outdoor_airflow_sumzone`, `thermal_zones_zone_states`, `ventilation_zones_ashrae62_1_setpoints` |
| Block-port bounds | 3 | refused (decision 8): `ahu_economizer`, `ahu_supply_air_temp_reset`, `vav_single_zone` |
| Nested composite | 4 | out of subset: `cooling_only_controller`, `multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21`, `multizone_vav_supply_fan`, `multizone_vav_supply_temperature` |
| Conditional component | 7 | out of subset: `multizone_vav_economizer_modulations_return_fan_relief_damper`, `multizone_vav_outdoor_airflow_ahu`, `multizone_vav_outdoor_airflow_title24_ahu`, `multizone_vav_outdoor_airflow_title24_sumzone`, `multizone_vav_plant_requests`, `multizone_vav_supply_signals`, `trim_and_respond_have_hol_false` |
| Expression parameter value | 1 | out of subset: `multizone_vav_relief_fan` (`relFanSpe_min + 0.15` on a block parameter) |

So the M2 subset holds **34** fixtures, not 40: five conditional fixtures were hidden behind an
earlier enumeration or bound refusal, and one expression behind a bound. With this note's decisions
and the widened #468, **31** round-trip; 10 do without #468. The three block-port fixtures are the
only in-subset documents this note leaves refused, and they are the review's call (question 2).
No in-subset fixture carries `nominal`, `unbounded`, `hasInstance`, `hasConstant`, an array
connector, an enumeration-typed boundary, a port `unit` or a local name that breaks the identifier
syntax. One in-subset boundary is typed by a **constant package**:
`multizone_vav_economizer_limits_common.uOpeMod` is an `S231:IntegerInput` whose `isOfDataType` is
`…ASHRAE.G36.Types.OperationModes`. The engine reads it as Integer
(`oce_cxf::resolve::value_types`, a constant package maps to `ValueType::Integer`), so the importer
normalises it to an Integer boundary and records the normalisation in the report (decision 21); the
emitter then writes `S231:Integer`, which loads to the same type and the same content id.

Two corrections to the issue thread, both confirmed in the code:

- **"Library enumeration as a block value" is not an enumeration.** In every flat fixture it is a
  G36 **Integer constant**: `Buildings.Controls.OBC.ASHRAE.G36.Types.OperationModes.occupied` (and
  `ZoneStates`, `FreezeProtectionStages`) on an Integer parameter (`Integers.Sources.Constant.k`,
  `Conversions.BooleanToInteger.integerTrue`). The engine grounds it to `Value::Integer` before
  anything else (`oce_cxf::ground`, `oce_model::g36_integer_constant`). G36 enumerations
  (`EnergyStandard`, `ControlEconomizer`, …) appear as block values only inside the nested
  `…_fixed_21` fixture. #468 as filed lists the four CDL enumerations and would not unblock these.
- **The engine does not clamp an input to its declared domain; it refuses the frame.**
  `oce_graph::frame::check_input` returns `FrameRefusal::InputDomain` for a value outside the
  effective domain that `oce_api::frame_inputs` derives (declaration bounds intersected with every
  target connector's). Dropping a bound on import would silently widen what a frame may carry.

## Model changes

Three changes, each touching the model, both forms, the schema, the validator and the emitter. The
model PR carries the first two and amends the authoring note's model table, decisions and
finding-code table with dated rulings; the third (library constants) lands with the importer PR
(ruling 10).

### Sequence parameters of an enumeration type

`SequenceParameter.value_type` becomes a `ParameterType`: `Real`, `Integer`, `Boolean` or
`Enum(TypeName)`, where the type name is the enumeration's last segment (`SimpleController`,
`EnergyStandard`), the same name the `Type.Member` spelling already uses. `SignalType` stays as it is
for boundaries.

| Form | Spelling |
| --- | --- |
| Text | `param conTyp : SimpleController = SimpleController.PI` |
| Wire | `{"name": "conTyp", "type": "SimpleController", "value": {"member": "SimpleController.PI"}}` |
| CXF (emitted) | root parameter node, `S231:isOfDataType` `{"@id": "http://example.org#Buildings.Controls.OBC.CDL.Types.SimpleController"}`, `S231:value` `"Buildings.Controls.OBC.CDL.Types.SimpleController.PI"` |

The text parser accepts any identifier in the type position and stays catalog-free; the validator
resolves it. The wire reader accepts any identifier string; the schema's `type` enum lists `Real`,
`Integer`, `Boolean` and every **enumeration** row of the facade table, generated like the `class`
enum. Constant packages are never a parameter type and never appear in that enum.
The emitter writes the class IRI under the fixed class base (authoring decision 10) and the full
member path, exactly as the corpus does (`multizone_vav_return_fan_direct_pressure.conTyp`), so
Rule 5 grounds the block's reference to it.

**A reference type-checks by class identity.** A block parameter of `ValueType::Enum(c)` may take
`{"ref": p}` only when `p` is declared `Enum(t)` and `t` resolves to class `c`; the validator then
grounds the reference to `Value::Enum { class: c, ordinal }` for the catalog's rules, as it grounds
Real and Integer references today. Any other pairing is `parameter-kind-mismatch`. An enumeration
sequence parameter's value must be a member of its own type (`unknown-enum-literal` otherwise). An
unknown type name is `unknown-enum-type`, an **existing engine code** (`DiagCode::UnknownEnumType`,
raised in `oce_cxf::resolve::specialize`) that stage 1 reuses with the same string, pinned by the
parity test.

Many of these parameters are referenced by nothing (the twelve `generic_air_economizer_high_limits_*`
fixtures declare `eneStd`, `ecoHigLimCon` and a climate zone that no leaf reads). They do not survive
flattening and do not enter the content id; the importer still carries them, because they are what
the author wrote. A dedicated test covers one: the emitted root parameter node is asserted (type IRI
and member path), and an engine load of the emitted document is warning-free.

### Bounds on boundary declarations

`Boundary` gains `min: Option<Bound>` and `max: Option<Bound>`, where a bound is a number literal:
Real or Integer, never a reference (the engine grounds bounds in an empty scope).

| Form | Spelling |
| --- | --- |
| Text | `out yRelDam : Real @unit 1 @min 0 @max 1`; tags `@min` and `@max` take a number, no unit suffix |
| Wire | `"min": [0], "max": [1]` on the input or output object; absent is `[]`; at most one item |
| CXF (emitted) | `S231:min` / `S231:max` on the boundary node, a typed `xsd:double` for Real (shortest round-trip spelling), a bare JSON integer for Integer, written after `S231:displayUnit` |

The canonical printer places `@min` and `@max` immediately after `@displayUnit`. A Real boundary
accepts an Integer literal, widened as parameters are; an Integer boundary refuses a Real literal; a
Boolean boundary takes no bound.

The strict schema profile cannot say "at most one item" (`maxItems` is outside it), so the wire reader
refuses a second item with a typed `WireError` and the schema's `description` for `min` and `max`
says so. On import, a bound's text (`xsd:double` or `xsd:decimal` typed literal, or a bare JSON
number) is converted to `f64` exactly as the loader converts it (`str::parse::<f64>`, the conversion
`oce_cxf::ground` uses), and a test pins bit equality between the imported bound and the engine's
`InputDefinition` bound for every bounded input in the corpus: the effective input domain is inside
the content id, so one ulp of drift would move it.

What the engine does with them today, by side:

- **Inputs.** The bounds define the effective acceptance domain (`InputDefinition::min`/`max`, the
  declaration intersected with every target connector's); a frame value outside it is refused with
  `FrameRefusal::InputDomain`. The canonical content id records the effective domain (content-id
  note, decision 10), so a dropped or altered input bound moves the id. An Integer input with no
  declared bound already reports the CDL `i32` range, so declaring exactly that range does not move
  the id; the validator does not flag it.
- **Outputs.** Nothing on the tick. At load the deep gate checks them (finite, uncrossed) and §7.10
  unification compares them with the driving port (`bound-mismatch` on conflict, propagation when
  one-sided); export round-trips them. They are outside the content id. They are carried anyway: the
  importer never drops an attribute the engine reads, and a declared output range is a load check.

The validator checks, at stage 1: a crossed pair is `crossed-bounds` (the engine's code, pre-empted
with the same string); a bound on a Boolean boundary or a Real literal on an Integer boundary is the
new author-only `invalid-bound` (pre-empting the engine's `malformed-document`); an Integer literal
above 2^53 on a Real boundary is `invalid-bound` (no exact Real, as for parameters), and so is an
Integer bound on an Integer boundary outside the engine's `i32` domain (decision 6, amended by
#493). Non-finite
bounds cannot be spelled in either form; the builder refuses them with `non-finite-bound`, an
**existing engine code** (`DiagCode::NonFiniteBound`) reused with the same string and parity-pinned.
`bound-mismatch` stays a stage-2 engine code.

`nominal` and `unbounded` are **not** carried. No fixture has them, `unbounded` is not parsed by the
engine, and `nominal` is never read on the tick; the importer refuses either with a typed error
rather than dropping it (decision 7).

### Library constants as block parameter values

The tagged member value widens from "an enumeration member" to "a member of a named type": an
enumeration member, or a named Integer constant of a library constant package. The model's
`Literal::EnumMember` becomes `Literal::Member(MemberRef { type_name, member })` (landed in #493,
issue #492); the wire tag
`{"member": "OperationModes.occupied"}` and the text spelling `OperationModes.occupied` are
unchanged. The type name decides which it is, through one facade table (#468, widened), never the
parameter's kind, so a document's meaning never depends on the catalog (authoring decision 11).

| Member of | Fits | Emitted `S231:value` | Emitted `S231:isOfDataType` |
| --- | --- | --- | --- |
| an enumeration | an Enum parameter of the same class | full member path | the enumeration's class IRI |
| an Integer constant package | an Integer parameter | full member path | the package IRI, as the corpus writes it |

The engine grounds the full constant path to its Integer before any expression evaluation, so the
loaded value, and the content id, equal the committed fixture's. A constant member on a Real
parameter is `parameter-kind-mismatch` until a document needs the widening.

**The facade table (#468, widened).** One accessor lists every named type the engine knows, each
with its class path, last segment and kind: the four CDL enumerations, the ten G36 enumerations
(`oce_model::g36_enum_class_id`) with their ordered members, and the four G36 constant packages
(`DemandLimitLevels`, `FreezeProtectionStages`, `OperationModes`, `ZoneStates`) with their member
values. Today the constant packages exist only as hand-written `match` arms in `oce-model`
(`is_g36_integer_constant_package`, `g36_integer_constant_in_package`) with no list a test can
enumerate, so #468 (amended) replaces those arms with **one data table** that both the existing
functions and the new accessor read. Every row has one class-path form, relative to
`Buildings.Controls.OBC.` (`CDL.Types.SimpleController`, `ASHRAE.G36.Types.OperationModes`), so the
emitter's fixed class base serves CDL and G36 alike, and every row is marked **enumeration** (a valid
parameter type) or **constant package** (members are Integer constants; never a parameter type).
`oce-author` deletes its pinned `ENUM_TYPES` table. A test pins that last segments are unique
across the whole table, which is what makes `Type.Member` unambiguous; a future collision is refused
until the spelling is decided, never resolved by guess.

**#468 lands first** (see [sequencing](#dependencies-and-sequencing)). Should it slip, the importer
and the validator know only the four pinned CDL rows. A G36 type
or package is refused at import (`unknown-datatype` for a parameter's type, `unknown-member-type`
for a member path) and at validation (`unknown-enum-type`, `unknown-enum-literal`). No G36 rows are copied into `oce-author` meanwhile:
#468 exists to delete the copied table, and the G36 lists are long (eighteen climate zones).

## The CXF importer

| CXF | Model |
| --- | --- |
| root `@id` `{namespace}{sequence}` | `Sequence.name` = the trailing identifier; the namespace is not kept |
| root `hasParameter` | sequence parameters in order; `isOfDataType` names the type (scalar or enumeration); value a literal or a member |
| root `hasInput` / `hasOutput` | boundaries in order; `@type` and `isOfDataType` must agree; `unit` to `SemanticPoint.unit`; `quantity`, `displayUnit`, `min`, `max` to their fields |
| root `containsBlock` | blocks in order; class path from the `@type` fragment after `Buildings.Controls.OBC.` |
| block `hasParameter` | assignments in order; arrays row-major under the catalog's template names |
| `isConnectedTo` | connections by source node in `@graph` order, each source's targets in array order |

**Read and dropped, and reported** in `ImportReport.dropped` (closed `DroppedProperty`): the root's
`@type` (the Modelica class the sequence was translated from); `S231:label` where it differs from the
node's local name; a parameter's descriptive `unit`, `quantity`, `displayUnit`, `min` and `max`. The
engine reads none of these on a parameter node: only connector nodes reach
`oce_cxf::resolve::attrs::connector_attrs`. Carrying parameter bounds would give the author a range
the engine never enforces (decision 9). In the in-subset corpus, root parameters carry `unit` in 16
fixtures, `quantity` in 13 and `min`/`max` in 3. The report also lists one **normalisation**: a
boundary typed by a constant package becomes an Integer boundary (decision 21).

**Refused, never guessed** (each a typed kind, see [`ImportError`](#importerror)): every construct in
the authoring note's out-of-scope list, an expression value, a block-port attribute, `nominal` and
`unbounded`, an unknown key or `@context` term, a `@context` that binds the S231 vocabulary in
another namespace, a value whose shape disagrees with its `isOfDataType`, and an unknown named
type.

**Read and not carried, with no report entry:** the translation-software label
(`S231:translationSoftware`, `S231:translationSoftwareVersion`; OBC §8.4) on the root or on a
block node, whatever its value. The emitter writes its own on the root, so an emitted document
still re-imports with an empty report and to the same bytes. On any other node the two keys are
unknown keys.

## The v0 graph JSON importer

The frozen prototype's compact block graph (its `docs/block-graph-schema.md` and
`docs/block-graph.schema.json`, serialized by `crates/mm-graph/src/graph.rs`) is a strict-JSON object
with five required keys and closed objects throughout:

```json
{"name": "ahu_0027",
 "inputs":  [{"name": "override_active", "type": "bool", "unit": "bool", "description": "Manual override active flag"}],
 "outputs": [{"name": "yFault", "type": "bool", "description": "True once the override has been held …"}],
 "blocks":  [{"id": "staleOvr", "class": "Logical.TrueDelay",
              "params": [{"name": "delayTime", "value": 86400.0}, {"name": "delayOnInit", "value": true}]}],
 "connections": ["override_active -> staleOvr.u", "staleOvr.y -> persist.u", "persist.y -> yFault"]}
```

| v0 | Model |
| --- | --- |
| `name` | `Sequence.name`; `derived_from` none unless `ImportOptions` sets it |
| `inputs[]` / `outputs[]` `name`, `type` | boundaries; `bool`, `real`, `int` to `Boolean`, `Real`, `Integer` |
| `inputs[].unit`, `*.description` | comment annotations on the boundary (see below) and a `Dropped` entry; never `SemanticPoint.unit` |
| `blocks[].id`, `class` | block name; class path `CDL.` + `class` |
| `blocks[].params[]` | assignments in order; an integer token is `Integer`, a fraction or exponent `Real`, `true`/`false` `Boolean` |
| `connections[]` | the model's connection parser, as the wire form uses it |
| (no sequence parameters, no bounds, no quantity) | empty |

**Lossy, by construction.** v0 has no sequence parameters, references, bounds, quantity or display
unit, so none are produced. `unit` and `description` existed for the model's own reasoning and the
prototype's emitter dropped both; they become text-form comments (which the wire form drops) and
report entries. `unit` never reaches `SemanticPoint.unit`: the emitter writes that as `S231:unit`,
which would change the emitted CXF against what the prototype emitted, move the content id (`unit`
is inside it) and put non-units such as `bool` into the engine.

**Refused.** A string `value` (v0 reserved it for enumeration parameters and its own validator
refused them, so a string's meaning would be a guess), `null`, an object or an array value; any type
other than the three; an unknown or missing key (the prototype's other graph JSON shapes, such as
its `fixtures/graphs/` files that write `signal` instead of `type` and `unit`, are not v0); a connection the
model's parser refuses. The reader is the strict reader (`import/strict.rs`: duplicate keys and
integers outside `i64` refused) under the wire form's bounds (4 MiB, depth 8, 65,536 entries per
list, 4,096-byte strings).

**Comments from v0 text.** A v0 `description` may be up to 4,096 bytes and contain line feeds, and
`unit` is free text, so neither is pasted into a comment as is. Each becomes one or more comment
lines on the boundary's existing `annotations`: the text is split at line feeds; every other control
character (CR and tab included) becomes one space; each line is truncated at a character boundary so
`# ` plus the line stays within the text form's 4,096-byte line limit (a truncation is recorded in
the report); `unit` is written as a last line `unit: <unit>`. No escaping is needed beyond that,
because a comment runs to the end of its line and the parser accepts any other UTF-8 there. A test
feeds a description with line feeds, CR, tab, NUL and 4,096 bytes through import, `print` and `parse`
and requires the parse to succeed and `fmt` to be idempotent.

**Provenance.** The importer adds one comment line naming the source form
(`# imported from v0 graph JSON`) to `Sequence.annotations`, which the text-form PR (GitHub #485,
issue #482) adds with `Sequence.trailing_comments`; it is not a model change of this note's. The
caller may set `derived_from`. Nothing about provenance reaches the wire form or the emitter.

**Fixtures (vendored in the importer PR, not now).** The prototype's three golden pairs, each a v0
graph and the CXF its emitter wrote, copied verbatim into `crates/leverage/oce-author/tests/fixtures/v0/`
with a `PROVENANCE.md` naming the source repository by its URL (never a local path), the pinned
commit (`f951346`), each source path and the licence, and retaining the source's copyright line:

- `crates/mm-graph/tests/golden/ahu_0005.graph.json` and `ahu_0005.cxf.jsonld`
- `crates/mm-graph/tests/golden/ahu_0027.graph.json` and `ahu_0027.cxf.jsonld`
- `crates/mm-graph/tests/golden/width_max_min.graph.json` and `width_max_min.cxf.jsonld`

They are small conformance fixtures, not sequences, so standing rule 7 is kept; the five benchmark
graphs under the prototype's `specs/` belong to #457's destination decision. The prototype is
licensed `MIT OR Apache-2.0` with the same copyright holder as this repository, so verbatim copies
need only the provenance file. The prototype's CXF uses the namespace `urn:mechanical-muse:<name>#`,
which ends in `#`, so its names reduce cleanly under the content-id rule and the pairs need no
re-stamp. `width_max_min` uses the width-driven `Reals.MultiMax`/`MultiMin` (`nin = 3`), so it
also exercises the port synthesis of #440 and the extent questions of #467. One benchmark graph is
added when #457 decides where the benchmark graphs live: v0 import is their migration path.

## Names and namespace

- **Sequence name.** The trailing identifier of the root `@id` after its last non-identifier
  character. The corpus namespaces are `http://example.org#g36.source.` and, for three hand-written
  fixtures, `http://example.org#g36.`. Four in-subset sequence names are exactly 64 bytes, the
  identifier limit; a longer one is refused (`invalid-name`), never truncated.
- **Local names.** Every node `@id` is `{root}.{local}`, with `local` one identifier (a root member)
  or `block.member`. A part that breaks the identifier **syntax** (a dot, a non-ASCII byte, more than
  64 bytes) is refused at import: the endpoint grammar cannot spell it. A Modelica reserved word, a
  text-form keyword or a duplicate name is carried, because the model holds such names by design so
  that the validator reports them as `invalid-name` / `duplicate-name` in the author's terms. The
  corpus has none. The importer never renames, suffixes or case-folds.
- **Classes.** The class path is taken verbatim; a class the catalog lacks or reserves is reported
  by the validator (`class-not-found`, `class-not-authorable`). A reserved
  `urn:oce:lowering#` class cannot be spelled in a CXF class IRI at all.
- **Re-stamp.** The library round trip keeps rewriting the committed fixture's root prefix to
  `urn:oce:library:{sequence}` before comparison. Under the content-id note's naming rule the corpus
  names keep `g36.source.` (the namespace is stripped only through the last `#`, `/` or `:`), and an
  emitter namespace must end in one of those, so no emitted document can spell the corpus names.
  Library bundle v2, regenerated from the authoring form, removes the need; until then the re-stamp
  is the only difference, and the test proves it.

## `ImportError`

The provisional `Unsupported { subject, construct: String }` becomes a closed kind:

```rust
pub struct ImportError { pub source: SourceForm, pub subject: String, pub kind: ImportErrorKind }
impl ImportErrorKind { pub fn code(&self) -> &'static str }   // stable kebab-case, for reports
```

| Kind | Code | When |
| --- | --- | --- |
| `TooLarge { bytes, limit }` | `too-large` | above `MAX_CXF_BYTES` (CXF) or the wire bound (v0), checked before parsing |
| `NotStrictJson(StrictJson)` | `not-strict-json` | syntax, a duplicate key, an integer outside `i64`, non-UTF-8 |
| `LimitExceeded(Limit)` | `limit-exceeded` | v0 depth, list length or string length |
| `UnknownKey { key }` | `unknown-key` | a property, top-level key or `@context` term the form does not define |
| `NamespaceMismatch { term, found, expected }` | `namespace-mismatch` | a CXF `@context` that binds `S231` to any IRI but `http://data.ashrae.org/S231#`, or binds any term to the namespace written through release 0.4.0 (GitHub #527) |
| `MissingKey { key }` | `missing-key` | a required v0 key, a value-less parameter |
| `WrongShape { key }` | `wrong-shape` | a relation that is not references, a label that is not a string, a v0 value that is null, an object or an array |
| `NoRoot` | `no-root` | no node contains blocks |
| `DuplicateId` | `duplicate-id` | a node `@id` declared twice |
| `Unresolved { reference }` | `unresolved-reference` | a reference to no node, or to a node outside the root's IRI |
| `StrayNode` | `stray-node` | a node no root relation reaches, or one reached twice |
| `InvalidName` | `invalid-name` | a root `@id` or local part that breaks the identifier syntax |
| `NestedComposite` | `nested-composite` | a second node with `containsBlock` |
| `InstanceMember` | `instance-member` | `hasInstance` |
| `ConstantMember` | `constant-member` | `hasConstant` |
| `ConditionalComponent` | `conditional-component` | `isConditionalComponent` or `conditionalExpression` |
| `ArrayConnector` | `array-connector` | an array boundary or port |
| `ArraySequenceParameter` | `array-sequence-parameter` | `isArray` on a root parameter |
| `ExpressionValue` | `expression-value` | a value that is an expression, or a bare name that is not a sequence parameter |
| `PortAttribute { attribute }` | `port-attribute` | `unit`, `quantity`, `displayUnit`, `min`, `max` or `nominal` on a block port |
| `ConnectorAttribute { attribute }` | `connector-attribute` | `nominal` or `unbounded` on a boundary |
| `DrivingSink` | `driving-sink` | `isConnectedTo` on a boundary output or a block input |
| `ClassOutsideObc` | `class-outside-obc` | a block `@type` not under `Buildings.Controls.OBC.` |
| `DatatypeMismatch` | `datatype-mismatch` | a value or bound whose shape disagrees with its declared type, or a connector whose `@type` and `isOfDataType` disagree |
| `UnknownDatatype` | `unknown-datatype` | an `isOfDataType` that is no S231 scalar and no named type |
| `UnknownMemberType` | `unknown-member-type` | a member path whose type the facade table lacks |
| `MalformedLiteral` | `malformed-literal` | a non-finite or unparsable number, an unknown XSD datatype, a bad string escape |
| `ArrayShape` | `array-shape` | sizes that disagree with the value, no catalog template, a rank the catalog does not have |
| `StringValue` | `string-value` | a v0 string parameter value |
| `MalformedConnection` | `malformed-connection` | a v0 connection the model's parser refuses |

The out-of-subset kinds map one to one onto the authoring note's out-of-scope list, so the round-trip
test's exclusion table matches on `kind`, not on a substring of a message. `Display` stays for
humans; the CLI's JSON report (when an `oce import` command exists) carries `code` and `subject`.
`ImportErrorKind` is `#[non_exhaustive]`, the crate's policy since GitHub #466; consumers branch on
`code()`, and an in-crate test pins the full kebab-case code list, so a new kind still breaks a test
(ruling 6).

**Refusal precedence.** A document with several out-of-subset constructs is refused for the first in
this fixed order, whatever its node order: nested composite, conditional component, expression
value, port attribute, name. Examples: `multizone_vav_supply_temperature` (nested, conditional,
`hasConstant`, port bounds) is `nested-composite`; `…_fixed_21` (nested and a 72-byte sequence name)
is `nested-composite`; `trim_and_respond_have_hol_false` (conditional and an expression) is
`conditional-component`. The importer checks those classes in that order before reading values, and
the exclusion table, which names one kind per fixture, tests the order.

## Test plan

- **Gate item 1.** Every in-subset fixture imports, emits under `urn:oce:library:` and loads
  warning-free, and its identity equals that of the committed fixture re-stamped and loaded
  directly. The switch of `tests/support/cxf.rs::identity` and `library_round_trip.rs` to
  `Engine::content_id()` is already in #474, so for the importer PR it is a rebase item, not
  importer work. Until #474 lands, most of the 31 are compared on export bytes plus deferral codes:
  an enumeration-valued block parameter (`controllerType` from a `SimpleController` reference) has no
  CXF literal in the export subset (`oce_cxf::export`), so those exports are partial and have no
  complete export id. Once the canonical id is in, block order no longer bears on the comparison, so
  `multizone_vav_outdoor_airflow_sumzone` (#458) is compared like the others and its exclusion goes.
  Export bytes are still compared where block order agrees. `IMPORTED` grows from 4 to 31.
- **The exclusion table** shrinks to 15 rows, each naming its `ImportErrorKind`: 4
  `nested-composite`, 7 `conditional-component`, 1 `expression-value`, 3 `port-attribute`. A fixture
  that starts importing fails `every_other_fixture_is_refused_with_its_reason`, as today. The
  coverage test asserts the table and `IMPORTED` partition the corpus.
- **Re-import is byte-stable** (`re_emitting_an_imported_document_is_byte_stable`) for every
  imported fixture, enumeration parameters, bounds and constant members included.
- **Hostile corpus.** `import_hostile.rs`'s deterministic mutation sweep runs over all 31 fixtures and
  the three v0 graphs; new cases: a bound as an expression, a string, `"1e999"` or `"NaN"`; a bound on
  a Boolean boundary; crossed bounds (imported, then refused by the validator); `nominal` and
  `unbounded`; an enumeration parameter whose value is an Integer; a constant member on a Real
  parameter; an unknown package and an unknown member; each out-of-subset construct grafted onto an
  importable fixture; a package-typed Integer boundary (normalised and reported) and a
  package-typed Real or Boolean boundary (`datatype-mismatch`); v0 duplicate keys, nesting past depth 8, 65,537 blocks, a 4,097-byte string, a
  string, `null`, object and array `value`, `"type": "boolean"`, an extra key. Every case returns a
  typed error with its exact kind; none panics (`catch_unwind` names the mutant).
- **v0 fixtures.** Each vendored graph imports; emitting it with the default namespace and loading it
  gives the same `Engine::content_id()` as loading the vendored CXF the prototype wrote (no re-stamp:
  `urn:mechanical-muse:<name>#` strips to the same names); the comment annotations and the `Dropped`
  entries match a golden; v0 → model → wire → model is identity.
- **Model changes.** Wire: closed-object refusals for `min`/`max` (a non-array, two items, a string)
  and for `type`; the schema regenerated and pinned, with the profile walk still clean (no new
  keyword: `min`/`max` are arrays of numbers, `type` a string enum). Text: `@min`/`@max` and an
  enumeration type parse and print canonically, a suffix on a bound is a typed refusal, `fmt`
  idempotent. Validator: one test per new or widened code (`unknown-enum-type`, `invalid-bound`,
  `crossed-bounds` at stage 1, `parameter-kind-mismatch` for each mismatched reference pairing,
  `unknown-enum-literal` for a wrong member and for a constant), and the parity test for each
  (stage 1 bypassed, engine code as the table says). Emitter: byte goldens for an enumeration
  parameter, both bounds on both boundary kinds, a one-sided bound and a constant member; each loads
  warning-free; a bound moves the content id of an input and not of an output, as the content-id
  note says; the metadata seam test still passes.
- **Goldens re-blessed, named.** `crates/leverage/oce-author/schema/sequence.schema.json`,
  `tests/fixtures/emit/every_element.cxf.json` (gains bounds, an enumeration parameter and a constant
  member) and the finding goldens that list stage-1 codes. Engine goldens are untouched: the 46
  fixture goldens and 422 provenance goldens stay byte-identical (leverage lane only).

## Dependencies and sequencing

**Ruled order: #468, then the model PR, then the importer PR** (ruling 10).

0. **#468** (facade, amended): the one named-type data table and its accessor. It lands first.
1. **Model PR** (issue cut from this note): `ParameterType` and boundary bounds; the wire DTOs, the
   schema, the text grammar and printer, the validator and the emitter; the authoring note amended
   with dated rulings. It follows the text-form PR GitHub #485 (issue #482), which adds
   `Sequence.annotations` and `Sequence.trailing_comments`, and extends the merged parser rather than
   racing it.
2. **Importer PR**: the constants half of decision 10 (`Literal::Member`, the constant column of the
   member table and its emitter row), `ImportErrorKind`, `ImportReport`, the CXF importer widened,
   `import_graph_v0`, the vendored v0 fixtures, the round-trip and hostile suites. It needs the model
   PR, #468 for 21 of its 27 new fixtures, and rebases onto #474 for the canonical comparison.

One PR was considered: the importer is the model changes' first consumer, so one PR proves both at
once. Rejected because the model change is a decision-record amendment that #482, the CLI and the
vocabulary work all build on, reviewed on its own merits; mixing it with the importer's large test
corpus hides it.

Relation to other work:

- **#475** (gate item 1 residual) closes with the importer PR; its 36 becomes 27 newly importing plus
  the corrected out-of-subset count, and the #455 gate row's denominator becomes 34.
- **#458** closes: with the canonical id, node order no longer bears on the comparison.
- **#467** is unaffected; the importer carries out-of-extent element names and the validator (and,
  after #467, the load gate) reports them.
- **#468** must widen to the G36 enumerations and constant packages (decision 11); it is the only
  facade change this note needs.
- **#474** supplies `Engine::content_id()`.
- **#457** owns the prototype's benchmark graphs; this note vendors only the three golden pairs.
- **#485** (text form, issue #482) adds the sequence-level annotations the v0 provenance comment uses.
- **#486** (block-port attributes) and **#487** (conditional components) are the post-M2 design
  passes for the two largest refused classes (rulings 2 and 7).

## Decisions

**1. Sequence parameters may have an enumeration type.** `ParameterType` adds `Enum(TypeName)`,
spelled by the last segment in both forms (`"type": "SimpleController"`). Rejected: a tagged object
`{"enum": "…"}` (an `anyOf` in a field that has none today, and two spellings of a type); the full
class path (inconsistent with `Type.Member`, and longer for agents); lowering to an Integer ordinal
(the engine's Enum kind check refuses it, and the author's type is lost). Why: enumeration-typed
root parameters appear in 24 fixtures, 17 of them in the subset (the twelve
`generic_air_economizer_high_limits_*`, `cooling_only_dampers`, `multizone_vav_economizer_limits_common`,
`multizone_vav_freeze_protection`, `multizone_vav_return_fan_airflow_tracking`,
`multizone_vav_return_fan_direct_pressure`); the engine already types them.

**2. A reference to an enumeration parameter type-checks by class identity.** Rejected: matching
the type name as a string (two names could one day reach one class). Why: the engine checks
`ValueType::Enum(class)`.

**3. The emitter lowers an enumeration parameter as the corpus does.** A root parameter node with
`isOfDataType` the class IRI under the fixed class base and the full member path as value.
Rejected: any other spelling (the corpus spelling is the one the engine is proven on).

**4. Boundary declarations carry optional `min` and `max`.** Number literals, Real or Integer
boundaries only. Rejected: leaving them out (an imported input would accept frames its author
refused; boundary bounds appear in 14 fixtures, 9 of them in the subset, beside 3 in-subset
fixtures with block-port bounds); carrying only input bounds (output bounds are load-checked and
exported, and the importer never drops what the engine reads). Why: safety; the input domain is
what the frame driver enforces.

**5. Bounds are spelled `@min`/`@max` and `"min": []`.** Rejected: `""` as the absent value (a
number field that is sometimes a string needs `anyOf`); `null` (not in the strict profile); one
`"bounds"` object (one-sided bounds exist: four boundary nodes in the corpus declare only one
side). Why: the profile's absent convention for a list, and a reader that refuses a second item.

**6. Bound findings.** `crossed-bounds` at stage 1 with the engine's code, the new author-only
`invalid-bound`, the existing engine code `non-finite-bound` from the builder, `bound-mismatch` at
stage 2. Rejected: a range
check against the CDL `i32` range for Integer bounds (the engine applies none). *(Amended
2026-11-09, #493: an Integer bound on an Integer boundary outside `i32::MIN..=i32::MAX` is now
`invalid-bound` at stage 1, because the engine does narrow it to `i32` without a warning, so the
loaded domain would differ from the declared one; a parity test pins the narrowing.)*

**7. `nominal` and `unbounded` are refused, not carried.** Rejected: carrying them now (no
fixture, `unbounded` unparsed, neither in the id); dropping them silently. Why: model surface follows
the corpus; a refusal is reversible, a silent drop is not. *(Landed in GitHub #498, 2026-11-09.)*

**8. Block-port attributes are not authorable in M2.** `min`/`max` (and `unit`, `quantity`,
`displayUnit`, `nominal`) on a block port node are refused (`port-attribute`); three fixtures stay
out. Rejected: a per-instance port attribute statement (a new model element for three hand-written
fixtures, and the emitter writes ports from the catalog); moving the bound onto the connected
boundary output and relying on §7.10 propagation (a guess about intent). Why: the importer never
guesses; question 2 asks the owner. *(Landed in GitHub #498, 2026-11-09.)*

**9. Sequence-parameter bounds and units are read, dropped and reported.** Rejected: carrying them
with an author-side range check (a second semantics the engine does not share, authoring decision
4); refusing them (root-parameter `min`/`max` appear in 5 fixtures, 3 in the subset:
`multizone_vav_economizer_modulations_reliefs`, `multizone_vav_economizer_modulations_return_fan`,
`multizone_vav_relief_damper`; root-parameter `unit` and `quantity` appear in 16 and 13 in-subset
fixtures, so refusing descriptive parameter attributes would block far more). Why: the engine reads
no parameter `min`/`max` (verified in review: `connector_attrs` is reached only for connectors and
boundaries); `ImportReport` makes the drop visible. *(Landed in GitHub #498, 2026-11-09.)*

**10. Library constants are members.** `{"member": "OperationModes.occupied"}` covers enumeration
members and Integer constants, resolved by the type name. Rejected: importing the ground Integer (the
author's name is the point of an authoring form); a separate `{"constant": …}` tag and text keyword
(two spellings for one syntactic thing). Why: one closed table resolves both, and type names are
unique across it. Lands in the importer PR, not the model PR (ruling 10). *(The model half
landed in #493, issue #492: `Literal::Member`, the validator and the emitter; the importer half
stays with the importer PR.)* *(The importer half landed in GitHub #498, 2026-11-09.)*

**11. One facade table for named types, through #468 widened.** Rejected: extending `oce-author`'s
pinned table with the G36 rows (the drift #468 removes, multiplied); a second facade issue for
constants (two tables would each need the uniqueness invariant against the other). Why: one source,
one invariant.

**12. Out-of-subset constructs stay refused.** Nested composites, `hasInstance`, `hasConstant`,
conditional components, array connectors and expression values, each a typed kind; twelve fixtures.
Rejected: importing a conditional component's pruned result (evaluation in `oce-author`). Why: the
authoring note's out-of-scope list. *(Landed in GitHub #498, 2026-11-09.)*

**13. Names are carried, never repaired.** Syntax breaks are refused at import; reserved words,
keywords and duplicates are carried for the validator. Rejected: renaming or suffixing (an imported
sequence would no longer name the library's blocks, and the content id holds names). *(Landed in GitHub #498, 2026-11-09.)*

**14. The library re-stamp stays until library bundle v2.** Rejected: an emitter namespace that does
not end in a delimiter (refused by the content-id note's review ruling 2). v0 fixtures need none.

**15. Provenance is a report and a comment, not `derived_from`.** `ImportReport` names the source
form and every dropped property; one leading comment names the form; `derived_from` is none unless
the caller sets it. Rejected: the root `@type` in `derived_from` (M3 owns its prefixes, and
"translated from class X" is not "revision of Y"); the source's content id in `derived_from` (a CXF
round trip makes it the sequence's own id, a self-loop); a `source` field on `Sequence` (metadata
the engine never sees; M3's registry owns provenance). *(Landed in GitHub #498, 2026-11-09.)*

**16. `ImportError` is a subject plus a typed kind with a stable code.** `ImportErrorKind` is
`#[non_exhaustive]` (crate policy, GitHub #466) and its code list is pinned by an in-crate test;
refusal precedence is fixed (nested, conditional, expression, port attribute, name). Rejected: the
provisional free-text construct (tests matched substrings); an exhaustive enum (amended in review,
ruling 6: crate policy wins, and the pinned list still breaks a test on a new kind). Why: an agent and
a test branch on `code`. *(Landed in GitHub #498, 2026-11-09.)*

**17. v0 import maps only what v0 has.** `unit` and `description` become comments and report entries;
string values are refused. Rejected: `unit` into `SemanticPoint.unit` (emitted as `S231:unit`, moving
the id against the prototype's own CXF); `description` into `SemanticPoint.label` (a description is
not a point label, and M8 owns the record). *(Landed in GitHub #498, 2026-11-09.)*

**18. v0 fixtures are the prototype's three golden pairs, vendored verbatim.** Rejected: the five
benchmark graphs (#457 decides where they live); regenerating pairs here (the point is the
prototype's own emitter output). *(Landed in GitHub #498, 2026-11-09. GitHub #527 rebound the
`S231` context IRI in the three CXF files, the one string in which they now differ from the
prototype's bytes; `PROVENANCE.md` records both digests.)*

**19. #468, then the model PR, then the importer PR.** Rejected: one PR, and a model PR that does not
wait for #485; see [Dependencies and sequencing](#dependencies-and-sequencing).

**20. Gate item 1 compares canonical content ids once #474 lands.** Export bytes are compared only
where block order agrees. Rejected: keeping the order exclusion (#458) after the id stops bearing
order. *(Landed in GitHub #498, 2026-11-09.)*

**21. A boundary typed by a constant package is normalised to Integer and reported.** Rejected:
refusing it (`unknown-datatype` would block `multizone_vav_economizer_limits_common`, which the
engine loads as an Integer input); an enumeration-typed or package-typed boundary in the model (no
fixture needs it, and the engine has no package type to carry). Why: the engine reads the package
IRI on a connector as `ValueType::Integer`, so the normalised boundary loads to the same type and
the same content id; the report keeps the change visible. *(Landed in GitHub #498, 2026-11-09.)*

## Out of scope

- Nested composites, `hasInstance`, `hasConstant`, conditional components, array connectors,
  expression values and block-port attributes (decision 8).
- `nominal`, `unbounded` and Enum-typed boundaries.
- Importing CXF dialects other than the flat one, or the prototype's other graph JSON shapes.
- An `oce import` command (the CLI PR or a follow-up wires it).
- Vocabulary v1 (descriptions, units) and M8 semantic records.

## Questions for review (ruled; see below)

1. **The subset count.** The scan finds 12 out-of-subset fixtures (4 nested, 7 conditional, 1
   expression), not 6; the #455 gate row's denominator becomes 34. Accept, and record it there?
2. **Block-port bounds.** Refuse them in M2 (decision 8, three hand-written fixtures out), or add a
   per-instance port attribute to the model now?
3. **#468.** Widen it to the ten G36 enumerations and the four G36 constant packages in one accessor
   (decision 11), or file a second facade issue?
4. **Parameter bounds.** Drop and report (decision 9), or carry them in the model?
5. **Wire spelling of bounds.** `"min": []` / `[0]` (decision 5) acceptable for structured-output
   decoders?
6. **`ImportErrorKind` exhaustive** (decision 16), unlike the model types, which are
   `#[non_exhaustive]`?
7. **Conditional components** are the largest remaining class (7 fixtures). File a post-M2 design
   issue for them now?
8. **The v0 importer.** The prototype is frozen; is `import_graph_v0` still wanted for M2, and are the
   three golden pairs the right fixtures?
9. **Constant `isOfDataType`.** The emitter writes the package IRI, as the corpus does; or
   `S231:Integer`, the parameter's catalog kind?
10. **Sequencing.** Model PR after #482 and before the importer PR, as recommended, or one PR?

## Review rulings

The [review comment on GitHub #484](https://github.com/jscott3201/open-control-engine/pull/484#issuecomment-6737566636)
(2026-11-09) ruled on the ten questions; the note is amended to match:

1. **Subset:** accepted, 12 of 46 outside it; the M2 gate denominator becomes 34 and the tracking
   issue's row is corrected.
2. **Block-port bounds:** refused in M2 with a typed error (3 fixtures); per-instance port
   attributes are a post-M2 model decision, #486.
3. **#468 widened** to one data table (4 CDL enumerations, 10 G36 enumerations, 4 G36 constant
   packages) replacing the hand-written `match` arms, rows marked enumeration or constant package,
   one class-path form relative to `Buildings.Controls.OBC.`; it lands before the model PR.
4. **Sequence-parameter `min`/`max`:** dropped and listed in the `ImportReport`; the engine reads no
   parameter bounds at load.
5. **Bounds on the wire** as a 0- or 1-item list: accepted; the reader refuses a second item and the
   schema `description` says so.
6. **`ImportErrorKind`:** `#[non_exhaustive]` per crate policy, with the code list pinned by a test.
7. **Conditional components:** a post-M2 design pass, #487.
8. **v0 import:** kept as the migration path for the benchmark graphs (#457); the three golden pairs
   are vendored with a `PROVENANCE.md` (repository URL, copyright line), and one benchmark graph is
   added when #457 lands.
9. **Constant members:** the emitter writes the package IRI as `isOfDataType`.
10. **Sequencing:** #468, then the model PR (after #485), then the importer PR; the constants half of
    decision 10 moves to the importer PR. *(The model PR landed in #493, issue #492, with the
    constant member in the model, validator and emitter; importing it stays with the importer PR.)*

The same review's fix round added decision 21 (a package-typed Integer boundary, blocking), corrected
the decision counts to in-subset figures, reclassified `unknown-enum-type` and `non-finite-bound` as
existing engine codes, fixed refusal precedence, the bound-conversion test, the v0 comment rules and
the gate-item-1 rebase note.

## As landed

GitHub #498 (issue #496, 2026-11-09) implements the importer as specified; where the note left a
choice open, or the implementation reads a rule more narrowly, the choice is recorded here.

- **29 kinds.** `ImportErrorKind` had the 29 rows of the [`ImportError`](#importerror) table, one
  code each, in table order (`IMPORT_ERROR_CODES`); the issue's "30-row" count was off by one.
  GitHub #527 added the thirtieth, `namespace-mismatch`.
- **Kinds for cases the table leaves open.** A bound written as a string is `expression-value`; a
  bound on a Boolean boundary, a Real literal on an Integer boundary and a typed Boolean bound are
  `datatype-mismatch` at import (the validator's `invalid-bound` stays for models built in the
  other forms); an `S231:String` sequence parameter, a v0 `type` other than the three and a class
  IRI no table row names are `unknown-datatype`; an enumeration-typed boundary is
  `datatype-mismatch`; a parameter `@type` other than `S231:Parameter`, a typed literal with
  another key and a non-string label are `wrong-shape`; a member named under another block or a
  path deeper than `block.member` is `invalid-name`; a node reached twice is `stray-node`; text
  after a string expression's closing quote is `expression-value`.
- **Members.** A member path whose type the facade table holds is carried whatever its member
  name, so an unknown member of a known type reaches the validator (`unknown-enum-literal`); a
  path whose type the table lacks, `CDL.Types` included, is `unknown-member-type` (the minimal
  importer carried any `CDL.Types` path).
- **Literals as the loader reads them.** A typed literal's lexical form is parsed untrimmed, as
  `oce_cxf::ground` parses it (the minimal importer trimmed it); every bound on a Real boundary is
  read as a Real (an Integer token with `as f64`), so a re-emitted bound reads back equal.
- **The report.** `ImportReport` carries `normalised` and `truncated` beside `dropped`, and each
  `Dropped` its value as written. A block parameter's descriptive attributes are reported like a
  root parameter's; a block port typed by a constant package is normalised and reported like a
  boundary (no in-subset fixture has one).
- **v0 comment lines.** "Every other control character" is read as the set the text form refuses
  on a line: C0 (CR and tab included), DEL, C1 and the bidirectional overrides and isolates
  (U+202A to U+202E, U+2066 to U+2069), each one space, so the printed text always parses.
  Trailing spaces are removed, as the parser removes them, and an empty description writes no
  line. The line bound is the note's 4,096 bytes for `# ` plus the line
  (`MAX_V0_COMMENT_LINE_BYTES`); the text form's own line bound has since become 8,192, so every
  imported line is inside both. v0 refusal subjects are JSON paths (`blocks[2].params[0]`); report
  subjects are boundary names.
- **Text bundles stay at four corpus fixtures.** The text goldens (`tests/fixtures/text/*.oce`)
  still cover the four fixtures the text-form PR chose; every one of the 31 imported fixtures is
  instead proven by re-emit byte stability and the content-id round trip, so 27 new text goldens
  were not added.
- **The vendored prototype CXF is named `.cxf.json`.** `oce-cxf`'s `parameter_grounding_corpus`
  test pins the count of tracked `.jsonld` files as an engine census, so the three prototype
  documents keep their bytes and take the emit goldens' extension; `PROVENANCE.md` records the
  rename.
- **The mutation sweep lives in `tests/import_sweep.rs`**, split by fixture family to stay under the
  700-line cap; `import_hostile.rs` keeps the hand-written cases.
- **Gate item 1's export comparison** runs for 30 of the 31 fixtures; the 31st,
  `multizone_vav_outdoor_airflow_sumzone` (#458), is compared on its canonical content id only,
  because its `@graph` block order differs from `containsBlock` order.
