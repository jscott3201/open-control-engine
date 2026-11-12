# Unit spellings

**Design note for issue #622, a draft awaiting the owner's rulings (see
[Questions for the owner](#questions-for-the-owner)).** The engine's OpenBuildingControl (OBC)
§7.10 unit gate reads a connector's unit, quantity and display unit only under the keys
`S231:unit`, `S231:quantity` and `S231:displayUnit`. modelica-json, the upstream translator, never
writes those keys: it writes `qudt:hasUnit`, `qudt:hasQuantityKind` and `S231:hasDisplayUnit`, and
they pass through unread. This note measures every tracked and library document under each way
of reading the other spellings, and asks the owner to rule on eight questions. Loading does not
change until the note is merged with rulings. The implementation is a later PR. Related: the
blank-term rule for connectors ([#602](https://github.com/jscott3201/open-control-engine/issues/602)),
the parameter shape check ([GitHub #594](https://github.com/jscott3201/open-control-engine/pull/594))
and the refusal of unknown S231 types
([#620](https://github.com/jscott3201/open-control-engine/issues/620)).

## What the engine does today

- **Three keys, three shapes.** The CXF reader models `S231:unit`, `S231:quantity` and
  `S231:displayUnit` as typed fields (`crates/oce-cxf/src/dto.rs`, `TermAttr`). Each may be a
  string, a typed literal or an IRI node. An IRI node's `@id` is used verbatim, never expanded, so
  `{"@id": "unit:K"}` is the term `unit:K`. Any other key, `qudt:*` included, stays in the opaque
  pass-through map. A modeled key spelled by its full IRI or another prefix is read as the same
  property, and two spellings of one property on one node are refused (issue #383,
  `crates/oce-cxf/src/property_keys.rs`).
- **The gate compares strings exactly.** `oce-validate` unifies each connected star (one output,
  the inputs it drives, and any declared boundary outputs that alias it). Two declared units or
  quantities that differ refuse the load with `unit-quantity-mismatch`. One declared value
  propagates to the unset members. Divergent display units warn with `display-unit-divergence`.
  Text is compared byte for byte: `K` and `unit:K` differ (`crates/oce-validate/src/rules/unify.rs`).
- **Boundary inputs are outside the gate.** A root input's declared attributes are kept apart from
  the ports it drives and never unified with them (the documented limit in
  [CXF round trip](../cxf-round-trip.md#the-export-subset)). The host's point list
  (`Engine::point_list`) reports each connector's own attributes. A root input's row therefore
  shows the unit of the port it drives, never the unit it declares. The declared unit reaches the
  canonical content id and the export, and no point row.
- **Parameter units are never used.** Since GitHub #594 a parameter's `unit`, `quantity` and
  `displayUnit` are shape-checked where the parameter is read. Their values reach no gate, no
  `ParamAttrs` field and no point.
- **Export writes one spelling.** The exporter writes the three attributes as bare strings under
  the compact `S231:` keys (`crates/oce-cxf/src/export_attrs.rs`). A unit carried by `qudt:hasUnit`
  is dropped on export, since it never reached the graph.
- **`oce-author`.** The importer reads `S231:unit`, `S231:quantity` and `S231:displayUnit` on a
  boundary as strings only (an IRI node is `wrong-shape`), refuses them on a block port
  (`port-attribute`, importer decision 8), and drops and reports them on a parameter (decision 9).
  Any `qudt:*` key is `unknown-key`. The emitter writes `S231:unit`, `S231:quantity` and
  `S231:displayUnit` on boundaries as bare strings. The model's `SemanticPoint.qudt` holds a QUDT
  quantity kind as semantic data, never emitted before M8.

## What the standard and the upstream tools say

The project reads the published OBC text; it has not read the text of ANSI/ASHRAE 231-2026.

- **OBC §7.4.1.1** declares the Real attributes as Modelica strings:
  `parameter StringType unit = "" "Unit used in equations";` and
  `parameter StringType displayUnit = "" "Default display unit";`. It adds: "The value of
  displayUnit is used as a recommendation for how to display units to the user."
- **OBC §7.10**: "If the quantity, unit, min or max attributes are set to a non-default value for
  both connector variables, then they must be equal. If only one of the two connector variables
  declares the quantity, unit, min or max attribute, then this value is applied to both connector
  variables. If two connectors have different values for the displayUnit attribute, then either
  can be used." The rule is about connector variables. It makes no exception for a composite's own
  inputs.
- **OBC §7.17.1**: "tools that parse CDL can infer the physical quantities from the unit and
  quantity attributes." Its example: "tools can verify that the PressureDifference is not
  connected to AbsolutePressure, and they can infer that the input has units of Pascal."
- **OBC §8.2** names CXF-Core.jsonld as the source of CXF's classes and properties. Its Table 8.2
  ("Key properties within CXF-Core.jsonld") lists no unit, quantity or display-unit property.
- **CXF-Core** (vendored at the recorded pins) names the property three ways:
  - its 42 `rdf:Property` declarations include `S231:unit`, `S231:quantity` and
    `S231:displayUnit`, and no `S231:hasUnit`, `S231:hasDisplayUnit` or `qudt:` property;
  - its `Constant` shape uses `S231:unit`, `S231:quantity` and `S231:displayUnit`;
  - its `InputConnector`, `OutputConnector` and `Parameter` shapes use `S231:hasUnit`,
    `S231:quantity` and `S231:hasDisplayUnit`, ranged over `qudt:Unit` and `qudt:QuantityKind`;
  - its own data nodes use `qudt:hasUnit`, `qudt:hasQuantityKind` and `S231:hasDisplayUnit`.
- **modelica-json** (`lib/cxfExtractor.js` at the vendored pin) writes a declared `unit` as
  `qudt:hasUnit`, a `displayUnit` as `S231:hasDisplayUnit` and a `quantity` as
  `qudt:hasQuantityKind`, each an IRI node.
  - **Units** go through a fixed table of 27 Modelica strings (`K` to `unit:K`, `degC` to
    `unit:DEG_C`, `1` to `unit:UNITLESS`). A string outside it is written in the S231 namespace
    (`S231:1/s`, `http://data.ashrae.org/S231#s/K`).
  - **Two table entries name another unit than their QUDT identifier does.** `cfm` becomes
    `unit:M3-PER-MIN`, cubic metres a minute, about 35 times a cubic foot a minute. `inH2O` becomes
    `unit:IN_H2O_60DEG_F`, the inch of water at 60 °F. The commissioning verifier's table keeps that
    inch apart from the conventional one (`inH2O_60F` at 248.84 Pa against `inH2O` at 249.08891 Pa,
    [commissioning note](commissioning.md)).
  - **Quantities are lossy:** `PressureDifference` becomes `q:Pressure`, the same kind as
    `Pressure`. That is the distinction §7.17.1 uses as its example.
- **Namespaces differ by document.** modelica-json binds `unit` to `http://qudt.org/vocab/unit#`
  and `q` to `http://qudt.org/vocab/quantitykind#`. QUDT publishes `http://qudt.org/vocab/unit/`,
  `http://qudt.org/vocab/quantitykind/` and `http://qudt.org/schema/qudt/`, each ending in a slash. The 34 translations that carry a unit bind
  `qudt` to `http://qudt.org/schema/qudt#`. CXF-Core binds it to `http://qudt.org/schema/qudt/`. The
  same compact key `qudt:hasUnit` therefore names two IRIs.

## Measurements

A throwaway probe, not committed, was built against this branch's crates. It walked every tracked
`.json` and `.jsonld` file holding an `@graph`, and the 140 CXF documents of a fresh clone of
`open-control-library` (`development` at `fe7060a`). For each candidate reading it rewrote each
document so the reading is expressed in the spellings the engine reads today. Under the
closed-table reading, for example, `"qudt:hasUnit": {"@id": "unit:K"}` became `"S231:unit": "K"`.
It then loaded the rewrite through `Engine::load_cxf` and compared it with the document as written:
load outcome, diagnostics by code, the unit columns of `point_list(None)` and `content_id()`. On the
graph after `import_cxf` and `unify_and_validate`, it also unified every root input declaration with
its targets. Everything below is computed, except the one row marked as an estimate.

**Where each spelling occurs.** Roles:
- *boundary*: a composite's own `hasInput` or `hasOutput` node;
- *port*: an instance's port or `hasInstance` member;
- *class*: an elementary class's own connector, which only CXF-Core has;
- *parameter*: a `hasParameter` or `hasConstant` node, or a node typed `Parameter` or `Constant`.

| Spelling | 44 G36 translations (`cxf/`) | CXF-Core | 212 in-repo documents | 140 library documents |
| --- | --- | --- | --- | --- |
| `qudt:hasUnit` | 411 in 34: 207 boundary, 6 port, 198 parameter | 73: 21 class, 52 parameter | 0 | 0 |
| `qudt:hasQuantityKind` | 160 in 29: 90 boundary, 70 parameter | 52: 11 class, 41 parameter | 0 | 0 |
| `S231:hasDisplayUnit` | 62 in 19: 39 boundary, 6 port, 17 parameter | 13: 9 class, 4 parameter | 0 | 0 |
| `S231:hasUnit` | 0 | 0 (three shapes name it) | 0 | 0 |
| `S231:unit` | 0 | 0 | 455 in 61: 280 boundary, 17 port, 156 parameter, 2 other | 0 |
| `S231:quantity` | 0 | 0 | 241 in 47: 149 boundary, 3 port, 87 parameter, 2 other | 0 |
| `S231:displayUnit` | 0 | 0 | 42 in 26: 36 boundary, 6 port | 0 |

- **The issue's totals hold:** 484 `qudt:hasUnit` in 35 documents, 212 `qudt:hasQuantityKind` in
  30 and 75 `S231:hasDisplayUnit` in 20, counting CXF-Core with the translations.
- **Its role split does not.** Its 21 "block ports" are CXF-Core's own class connectors, which no
  load reads. The G36 translations carry 6 units on instance ports, all `hasInstance` members.
- **Value shapes:**
  - every upstream value is an IRI node, written as a compact `unit:` or `q:` CURIE, except the
    S231 fallback, which is written as a full IRI where its local part is no valid name;
  - in the engine's spelling, 453 of 455 units are bare strings, with one IRI node
    (`modelica_json_style.jsonld`) and one malformed test value;
  - one display unit is the IRI node `{"@id": "degC"}` (`connector_attrs.jsonld`).
- **No node in any corpus carries two spellings of one attribute.**
- **Elsewhere:** the library carries no unit, quantity or display unit in any spelling, and
  `tools/` holds no CXF document. Rust test sources hold 22 `qudt:hasUnit`, 20
  `qudt:hasQuantityKind` and 12 `S231:hasDisplayUnit`, in
  `crates/oce-cxf/src/resolve/declaration_scope_tests.rs` and three `oce-vocab` tests.

**The IRIs in use.** Every QUDT IRI is a compact CURIE under modelica-json's `#` namespaces. None
uses QUDT's published namespaces.

| Attribute | 44 G36 translations | CXF-Core |
| --- | --- | --- |
| unit | `UNITLESS` 180, `M3-PER-SEC` 76, `SEC` 58, `K` 56, `PA` 29, `J-PER-KiloGM` 9, S231 fallback `s/K` 2 and `1/K` 1 | `SEC` 42, `RAD` 10, `UNITLESS` 9, `K` 5, S231 fallback `1/s` 4, `HZ` 1, `J-PER-KiloGM` 1, `PA` 1 |
| display unit | `DEG_C` 39, `PA` 14, `K` 6, `M3-PER-SEC` 3 | `DEG` 10, `HR` 3 |
| quantity kind | `VolumeFlowRate` 56, `ThermodynamicTemperature` 39, `Time` 28, `Pressure` 19, `TemperatureDifference` 10, `SpecificEnergy` 8 | `Time` 41, `ThermodynamicTemperature` 5, `Angle` 3, `Frequency` 1, `Pressure` 1, `SpecificEnergy` 1 |

- **Every unit IRI maps** through modelica-json's table or its S231 fallback. None is unmapped,
  and none is `M3-PER-MIN` or `IN_H2O_60DEG_F`.
- **All 19 translation `q:Pressure` mean `PressureDifference`.** The vendored G36 sources declare
  `quantity="PressureDifference"` 19 times and `quantity="Pressure"` never. `Pressure` appears only
  in two CDL Psychrometrics classes.

**What loads today.** This is the baseline for "newly fails".

| Corpus | Load today | Why the rest are refused (documents per code) |
| --- | --- | --- |
| 44 G36 translations | 7 | `grounding-failed` 23, `conditional-guard-unknown-parameter` 15, `single-assignment` 12, `malformed-document` 10 (the enumeration-only `Types/`), `unresolved-reference` 7, `class-not-found` 4 |
| CXF-Core | 0 of 1 | `non-subset-construct` |
| In-repo documents | 136 of 212 | many are refusal fixtures by design |
| Library | 0 of 140 | all bind the retired `S231P#` namespace (`vocabulary-namespace-mismatch`); with it rewritten, 140 of 140 load |

The seven that load are `ReliefDamper`, `ReliefFan`, `SupplySignals`, `TimeSuppression`, Reheat
`Overrides`, `ControlLoops` and `ZoneStates`.

**Outcome per reading, against today, over all 537 documents.** The readings are defined under
question 2. *R* is the recommendation: C without quantity kinds, every IRI-node unit term (the
engine's own keys included) through the table less its two contradicting entries, and an unmapped
IRI refused.

| Reading | Newly fail | Newly load | Newly warn | Point rows changed | Content ids changed |
| --- | --- | --- | --- | --- | --- |
| A. `S231:*` only (today) | 0 | 0 | 0 | 0 | 0 |
| B. A plus `S231:hasUnit`, `S231:hasDisplayUnit` | 0 | 0 | 0 | 0 | 0 |
| C. B plus `qudt:hasUnit`, `qudt:hasQuantityKind`, closed table | 0 | 0 | 0 | 18 in 5 translations | 6 translations |
| C′. C without quantity kinds | 0 | 0 | 0 | 18 in 5 translations | 6 translations |
| D. B plus the QUDT keys, IRIs opaque | 0 | 0 | 0 | 18 in 5 translations | 6 translations |
| R. the recommendation | 1, `connector_attrs.jsonld` | 0 | 0 | 19 in 6 documents | 7 documents |

- **The 18 rows under C** gain unit `1` (11) and `Pa` (7), display unit `Pa` (3), and quantity
  `Pressure` (2). Both quantities are on `ReliefFan` points whose source declares
  `PressureDifference`, so both are wrong. Under D the same rows read `unit:UNITLESS`, `unit:PA`
  and `q:Pressure`.
- **`TimeSuppression` changes only its content id.** Its units sit on root inputs, which reach the
  id and no point row.
- **The 37 translations that do not load keep their import diagnostics, code for code, under every
  reading.** The vendored-corpus delta table (`crates/oce-cxf/tests/vendored_corpus_delta.rs`)
  does not move.
- **Under R, the one newly failing document is a synthetic fixture.** `connector_attrs.jsonld`
  writes the display unit `{"@id": "degC"}`, which is no QUDT IRI. `{"@id": "unit:DEG_C"}` keeps
  its IRI-node coverage and its export bytes, which stay `"degC"`.
- **The 19th row** is `modelica_json_style.jsonld`'s `gai.y`, which reads `K` instead of `unit:K`.
- **The library changes under no reading.**
- **Estimate for the 37 translations that do not load**, read on their authored `isConnectedTo`
  stars rather than a resolved graph. There are 1,193 stars. 238 carry at least one unit, 3 carry
  two or more, and none conflicts under C, whether or not root inputs join their stars. In the
  upstream corpus the new spellings mostly feed propagation and what the host sees. They give the
  gate little to compare.

**What the corpus cannot show.** No tracked star mixes the engine's spelling with an upstream one,
so readings C and D cannot be told apart above. A probe of two gains in series (root input `TSup`,
`gai1`, `gai2`, root output `ySup`) can:

| Case | Today | C or R | D |
| --- | --- | --- | --- |
| `gai1.y` `"S231:unit": "K"`, `gai2.u` `qudt:hasUnit unit:DEG_C` (the issue's probe) | loads; `gai2.u` reads `K` | `unit-quantity-mismatch` | `unit-quantity-mismatch` |
| the same with `unit:K` | loads, `K` | loads, `K` | `unit-quantity-mismatch` (`K` against `unit:K`) |
| `gai1.y` `"S231:unit": {"@id": "unit:K"}`, `gai2.u` `"K"` | `unit-quantity-mismatch` | R loads, `K` | `unit-quantity-mismatch` |
| `ySup` `qudt:hasUnit unit:DEG_C`, driver `K` | loads | `unit-quantity-mismatch` | `unit-quantity-mismatch` |
| `S231:hasDisplayUnit unit:DEG_C` against `"S231:displayUnit": "degF"` | loads, no warning | `display-unit-divergence` | `display-unit-divergence` |
| `"S231:quantity": "PressureDifference"` against `qudt:hasQuantityKind q:Pressure` | loads | C refuses; R loads (kind not read) | `unit-quantity-mismatch` |
| fallback `S231#s/K` against `"s/K"` | loads | loads | `unit-quantity-mismatch` |
| root input `K` into a port declared `degC` | loads; the host's `TSup` row reads `degC` | the same | the same |
| root input `K` into a unit-free port | loads; the `TSup` row has no unit | the same | the same |

**Closing the root-input limit, measured.** The probe unified each root input declaration with
every port it drives, as one star under §7.10. It ran on every document that resolves.

| Corpus | Root inputs | Declaring a unit | Conflicts | Ports gaining a unit / quantity | Documents changed |
| --- | --- | --- | --- | --- | --- |
| In-repo (139 resolve) | 307 | 127 (78 also a quantity) | 0 | 188 / 128 | 40, of them 32 G36 fixtures |
| 7 loading translations, under R | 18 | 11 | 0 | 16 / 0 | 5 |
| Library, namespace rewritten | 376 | 0 | 0 | 0 | 0 |

Today 125 of the 127 declared in-repo root-input units never reach the point list. The other two
show only because the driven port declares the same unit. No declaration gains a unit from a port,
so no content id moves. The state manifest and the point list do.

## The proposal

1. Read `qudt:hasUnit` and `S231:hasUnit` as spellings of `unit`, and `S231:hasDisplayUnit` as a
   spelling of `displayUnit`. The S231 terms stay, and two spellings on one node are refused.
2. A term's shape decides how it is read, not its key. A string or typed literal is a unit string,
   read verbatim as today. An IRI node is a unit reference: it is expanded against the document's
   `@context` and looked up in a closed table compiled into `oce-cxf`. An unmapped IRI is refused.
   The host sees the table's Modelica strings.
3. `qudt:hasQuantityKind` is not read in this work.
4. The exporter is unchanged. A round trip writes the `S231:` spelling.
5. The root-input limit is closed by its own PR, right after this work.

## Questions for the owner

1. **Which spellings the engine reads, and what two on one node means.**
   - **Recommendation:**
     - read `qudt:hasUnit` and `S231:hasUnit` as `unit`, and `S231:hasDisplayUnit` as
       `displayUnit`, matched after `@context` expansion like every modeled key (issue #383);
     - accept `qudt:` keys in both QUDT schema namespaces: `#`, as the translations bind it, and
       `/`, as QUDT and CXF-Core bind it;
     - refuse two spellings of one property on one node as `malformed-document`, even when they
       agree, as #383 already does for two S231 spellings. No corpus node has two.
   - **Alternatives:**
     - S231 terms only, with the upstream keys refused or warned about. The 34 translations that
       carry a unit would then fail or warn on every one.
     - CXF-Core's `S231:hasUnit` and `S231:hasDisplayUnit` only. These read no corpus node.
     - Two spellings refused only when they disagree. That needs a comparison after mapping and an
       order between spellings, for a case no document has.

2. **How an IRI becomes the string the gate compares and the host sees, and what an unmapped IRI
   is.**
   - **Recommendation:**
     - the shape rule of proposal 2, applied to every unit and display-unit IRI node, the engine's
       own `S231:unit` included;
     - the table is a `const` array in `oce-cxf`: modelica-json's `getQudtUnit` inverted at the
       vendored pin, keyed by local name under modelica-json's `unit:` namespace and QUDT's
       published one, plus the S231 fallback read as its original string;
     - it drops the two entries whose QUDT identifier names another unit (`M3-PER-MIN`,
       `IN_H2O_60DEG_F`), which no corpus document uses;
     - an unmapped IRI is `non-subset-construct`, subject the node, with the IRI quoted and
       truncated;
     - the host sees Modelica strings (`K`, `Pa`, `1`, `m3/s`), the spelling of the engine corpus,
       vocabulary v1 and the commissioning table;
     - it stays database-free and deterministic: the table is static, no IRI is ever
       dereferenced, nothing can register entries at run time, and the same bytes give the same
       strings on every host.
   - **This reverses one recorded rule.** `TermAttr::Iri` says a unit IRI is never expanded and is
     compared verbatim. Under R, a document that connects an `S231:unit` IRI node to an equal unit
     string newly loads (the third probe case). No corpus document does.
   - **Alternatives:**
     - **D, opaque exact-match IRIs.** The measured corpus outcome is the same. It refuses
       `unit:K` beside `K`, and the host sees `unit:PA`.
     - **An unmapped IRI read as an opaque term, or warned about.** A typo would then unify with
       nothing and fail only where a declared unit meets it.
     - **The table kept in `oce-vocab`.** A core crate cannot depend on a leverage crate.
     - **The whole QUDT catalogue.** That is a database by another name, and conversion is M8's.
     - **The full 27-entry inverse.** It keeps `cfm` and `inH2O` readable from an IRI whose QUDT
       meaning is another unit.
   - **(b) Quantity kinds.**
     - Recommendation: do not read `qudt:hasQuantityKind` now, and leave `S231:quantity` as it is.
       modelica-json's table is lossy exactly at `PressureDifference`, and the measured result of
       reading it is wrong in 2 of 2 new host rows and 19 of 19 translation uses.
     - Alternatives: read every kind but `q:Pressure`; or read all of them, with `q:Pressure` as
       `Pressure`.

3. **What the exporter writes, and whether a round trip keeps the authored spelling.**
   - **Recommendation:** the exporter is unchanged. It writes bare strings under `S231:unit`,
     `S231:quantity` and `S231:displayUnit`. A document written with
     `qudt:hasUnit {"@id": "unit:K"}` exports `"S231:unit": "K"`, where today it exports no unit.
     The round trip normalises the spelling, as it already normalises a full-IRI key to the
     compact one. RT-2 holds, because re-import reads the same string. No export golden moves.
   - **Alternatives:**
     - **Write the upstream spellings.** This changes the export bytes of every document with a
       unit (question 7) and needs a forward table. 3 of the 10 distinct unit strings the in-repo
       corpus writes (`s/K`, `ppm`, `5`) have no QUDT entry.
     - **Keep each attribute's authored spelling.** That adds a model field per attribute and
       raises a content-id question, and nothing would read it.

4. **Is the measured impact acceptable?**
   - **Recommendation:** yes. Under R no real document changes load outcome. The one newly failing
     document is a synthetic fixture the implementing PR rewrites. The 19 changed point rows and 7
     changed content ids are units that were silently dropped. The library changes under no
     reading, because it carries no units.
   - **Alternative:** wait for the library's namespace re-stamp before landing. Nothing in the
     library would move either way.

5. **Is the root-input limit closed in the same work?**
   - **Recommendation:** close it, but in its own issue and PR, right after this one and in the
     same milestone. It unifies a root input's declaration and every port it drives as one star,
     as §7.10 reads.
     - Measured, it fails no document and gives 188 in-repo ports a unit and 128 a quantity, in 40
       documents.
     - Without it, 111 of the 213 connector units in the translations, those on root inputs, stay
       outside the gate. The other 96 boundary units are on root outputs, which join it.
     - The host keeps reading a root input's port unit rather than its declared unit, which is
       wrong whenever they differ (the eighth probe case).
     - It changes what those 40 documents export, because their minted child ports gain the unit
       (question 7).
   - **Alternatives:**
     - The same PR. That is two behaviour changes in one review.
     - Leave the limit. Then at least document that a root input's point row shows the driven
       port's unit, which 125 of 127 declared units never reach.

6. **`oce-author`'s importer and emitter, and vocabulary v1's declared units.**
   - **Recommendation:**
     - **Importer:** read the same spellings through the same table on a boundary (into
       `SemanticPoint.unit` and `display_unit`), accepting an IRI node where it now says
       `wrong-shape`. Refuse them on a block port as `port-attribute`, not `unknown-key`. On a
       parameter, check their shape, drop them and report them, as for `S231:unit`.
       `qudt:hasQuantityKind` stays `unknown-key`.
     - **Emitter:** unchanged.
     - **Table:** `oce-api` re-exports the engine's table, and `oce-vocab`'s generator reads units
       through it instead of its own copy (`tests/vocabulary/generator/units.rs`), so there is one
       table. CXF-Core uses neither dropped entry, so `generated.json` stays byte-identical.
       `oce-vocab` keeps its quantity inverse and the hand-resolved `pAtm` row. They are
       descriptive data, resolved against `.mo` sources, which the engine cannot do at load.
   - **Alternatives:**
     - Leave the importer on the S231 spellings, so the two readers disagree on the same document.
     - Keep two copies of the table, with a test that they match. The test could only live
       behind the facade.

7. **Sequencing, and rules 9 and 10.**
   - **Recommendation:**
     1. this note, merged with the rulings;
     2. the spellings PR, engine lane (`bash .agents/gate.sh`);
     3. the root-input PR, if question 5 is ruled so.
   - **Neither PR bumps to 0.6.0.** No format's bytes or meaning changes: not the content-id
     encoding, the blob, the snapshot layout, or what the exporter writes for a given graph. What
     changes is what an accepted document means, its units. That is the kind of change the owner
     ruled on 2026-11-12 is not a rule-10 change for tightening.
   - **This is partly a loosening, which that ruling did not name.** Some documents gain units, and
     the third probe case newly loads. The owner is asked to confirm the ruling covers it.
   - **Ids still move.** Content ids and state manifests of documents whose units are newly read
     change (7 tracked documents), and the CHANGELOG says so. The state manifest matters little:
     snapshots and blobs already refuse a different engine version.
   - **Changed export bytes would be different.** If question 3 chooses the upstream spellings, the
     exporter's own output changes: that PR bumps the workspace to 0.6.0 and re-blesses the export
     goldens. Closing the root-input limit also changes export bytes, but through the graph. The
     exporter's layout is the same, so the owner is asked to rule whether that counts.
   - **Formats stay at revision 1** in every case (rule 9).

8. **Hostile-input bounds for what is added.**
   - **Recommendation:**
     - **Lookup:** one prefix strip and a search of a sorted table of 25 entries a term, using the
       resolver's existing prefix expansion. Work stays linear in document bytes.
     - **Memory:** one `Arc<str>` a term, as today. Mapped strings are `&'static str`.
     - **No I/O:** no IRI is dereferenced, and nothing touches a network or a file.
     - **Messages:** a refusal quotes at most 128 bytes of the IRI, cut on a character boundary.
       This answers #620's note that a megabyte token makes a megabyte message.
     - **Near misses:** a key that names a new spelling only after trimming (`"qudt:hasUnit "`) is
       refused like any near miss (#383). An `@id` that is blank or padded is unmapped, so refused;
       #602 states the blank rule for connector terms.
     - **Scoped contexts:** a scoped `@context` on the term object stays refused.
   - **Alternative:** a per-document cap on unit terms. Each term is already bounded by its node,
     and its node by the 8 MiB document limit.

## What the implementing PRs change

**The spellings PR (engine lane):**
- **`oce-cxf`:**
  - the new spellings in `property_keys.rs`;
  - a new module holding the table;
  - the shape rule and the unmapped refusal in `resolve/attrs.rs` and `resolve/parameter_attrs.rs`;
  - the `TermAttr` documentation in `dto.rs`.
- **`oce-api`:** the re-export.
- **`oce-author`:** `import/nodes.rs` and `import/cxf.rs`.
- **`oce-vocab`:** the generator's unit inverse.
- **Tests:**
  - each spelling on each role;
  - both QUDT namespaces;
  - a `unit:` prefix bound elsewhere, which is not read as QUDT;
  - two spellings refused;
  - unmapped and dropped entries refused;
  - the probe cases above;
  - every unit IRI in the vendored translations and CXF-Core maps.
- **Fixtures and expectations that move:**
  - `connector_attrs.jsonld`'s display unit, rewritten to `unit:DEG_C` with its export bytes
    unchanged;
  - `modelica_json_style.jsonld`'s `unit:K`, which becomes `K` in
    `oce-cxf/tests/resolve_context_expansion.rs` and `oce-api/tests/context_expansion_identity.rs`;
  - two lines of `oce-cxf`'s fixture-corpus import golden.
- **What does not move:** the vendored-corpus delta table, and the 46 fixture goldens and 422
  provenance goldens.
- **Documents:**
  - [CXF round trip](../cxf-round-trip.md): the spellings read, and the spelling a round trip
    writes;
  - [host responsibilities](../host-responsibilities.md): the property-key paragraph;
  - the CHANGELOG.

**The root-input PR (engine lane, if ruled):**
- **Code:** `oce-validate`'s star gathering.
- **Expectations:** the point-list and state-manifest expectations of the 40 documents, and the
  export goldens among them.
- **Documents:** the limit's sentence in [CXF round trip](../cxf-round-trip.md#the-export-subset),
  and the point-identity section of
  [host responsibilities](../host-responsibilities.md#cxf-point-identities-are-the-authored-ids).

## Out of scope

- **Unit conversion and arithmetic on units.** `degC` against `K` stays a mismatch, never a
  conversion.
- **QUDT, Brick and other semantic mappings (M8)**, quantity kinds included.
- **Units declared by an elementary class on its own ports** (`CDL.Reals.Sources.ModelTime.y` in
  `s`). The engine reads no class declaration. Vocabulary v1 carries them as text.
- **Vector units written by `fill(...)`.** modelica-json drops them.
- **The library's point catalogues.** These are `points/*.points.json` files, not CXF; the engine
  never reads them. Their 300 unit values mix QUDT local names (117), Modelica strings (92) and
  free text (91).
