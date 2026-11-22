# oce-cli

The `oce` command-line tool (M2; design note [`docs/design/authoring-layer.md`](../../../docs/design/authoring-layer.md),
"CLI", decisions 9 and 20; issue #489): `emit`, `validate`, `fmt`, `catalog` and `vocab` over
[`oce-author`](../oce-author/README.md), [`oce-vocab`](../oce-vocab/README.md) and the facade, and
(M3; [vectors note](../../../docs/design/vectors.md), "CLI", decision 34; issue #522) `verify`
over [`oce-verify`](../oce-verify/README.md), with `--commission` for a commissioning spec (M3;
[commissioning note](../../../docs/design/commissioning.md), "CLI"; issue #664), (M3; [content-id note](../../../docs/design/content-id.md),
"Mapping table for the library re-stamp"; issue #513) `id` over the facade, (M3; [importer note](../../../docs/design/importer.md); issue #514) `import`
over `oce-author`'s CXF importer and (M3; [diff note](../../../docs/design/diff.md), "CLI",
decisions 1 and 16; issue #668) `diff` over [`oce-diff`](../oce-diff/README.md). A leverage crate
and the first binary above the facade (the engine crates ship none); it depends on `oce-api` and
the leverage crates `oce-author`, `oce-vocab`, `oce-verify` and `oce-diff` only, opens no
listener, and every engine it loads is a throwaway: the validator's, the verifier's, the one `id`
loads to name a document, or the two `diff` loads to compare two. Package `oce-cli`, binary `oce`,
`publish = false`. Arguments are parsed with `clap` (derive, no colour).

```text
oce emit <FILE> [--namespace <NS>] [-o <OUT>]    validate both stages, then flat CXF
oce validate <FILE> [--author-side]              findings as a JSON report
oce fmt <FILE> [--check | --write]               canonical text (wire input becomes text)
oce catalog [<CLASS>] [--json] [--param N=V ...] classes; one class with its block vocabulary
oce vocab [--json]                               keywords, tags, units, scale factors, codes
oce verify <VECTORS> --subject <FILE> [--subject-form cxf|text|wire] [--out <DIR>]
           [--require-mutants-caught] [--deny-warnings] [--timings]
                                                 a vector set replayed; the receipt on stdout
oce verify --commission <SPEC> [--reference <N>] [--out <DIR>] [--bounds]
           [--data trended|synthetic] [--deny-warnings] [--timings]
                                                 one commissioning reference run on its trends
oce id <FILE> [--from cxf|text|wire] [--namespace <NS>] [--json]
                                                 the canonical content id, alone on stdout
oce import <FILE> [-o <OUT>] [--wire]            flat CXF into canonical text (or the wire form)
oce diff <BASE> <HEAD> [--from cxf|text|wire] [--no-renames] [--format json|markdown]
                                                 the structural diff of two sequences or documents
```

`catalog` answers "what does this block take": the engine's facts (parameters, resolved ports)
joined with the block vocabulary (what each one means, its declared unit and quantity, its range,
the notes, the upstream links; [vocabulary note](../../../docs/design/vocabulary.md), decision 12,
issue #500). `vocab` answers "what can I write": the text form's own words, the unit scale
factors and every code the tool reports.

`FILE` is a `.oce` (text form) or `.json` (wire form) file, or `-` for stdin; `--from text|wire`
overrides the extension and is **required** for stdin. Every command takes `--render
auto|always|never` (default `auto`: a human-readable rendering of the report on stderr only when
**stderr** is a terminal, whatever stdout is; stdout is the same bytes whatever `--render` says),
`--help`, and `--version` (the workspace version). `--json` exists only on `catalog`, `vocab` and
`id`, the three commands whose default stdout is text; every other command's report is already
JSON (`diff --format markdown` asks for a table instead). `id` and `diff` read a third form, CXF,
and have their own form rule ([below](#oce-id)); `import` reads CXF only and takes no `--from`
([below](#oce-import)). A
failing `catalog` (an unknown class, a refused table) prints the JSON report on stdout even
without `--json`.

```sh
oce validate econ_enable.oce                 # report on stdout, exit 0 / 1 / 2
oce emit econ_enable.oce -o econ_enable.cxf.json # writes the CXF atomically, report on stdout
oce fmt --check econ_enable.oce              # exit 1 when not canonical
oce fmt econ_enable.json > econ_enable.oce   # wire to text
oce catalog CDL.Reals.MultiSum --param nin=3 # ports u1, u2, u3
oce catalog CDL.Logical.TrueDelay --param delayTime=5
```

The last one prints (the two `links:` lines are shortened here):

```text
CDL.Logical.TrueDelay
  Delay a rising edge of the input, but do not delay a falling edge

parameters:
  delayTime : Real (required)
      Delay time
      unit: s; quantity: Time
  delayOnInit : Boolean = false
      Set to true to delay initial true input

inputs:
  u : Boolean
      Input signal to be delayed when it switches to true

outputs:
  y : Boolean
      Output with delayed input signal after it switched to true

notes:
  convention (delayOnInit): The open-control-library sets delayOnInit = true on every TrueDelay, …
  loop: Passes the current input through: a feedback loop through TrueDelay alone is an algebraic loop …

links:
  source at the pinned Buildings commit: https://github.com/lbl-srg/modelica-buildings/blob/…/TrueDelay.mo
  help in the latest Buildings release (may drift from the pin): https://simulationresearch.lbl.gov/…
```

Under each parameter and port: its description, then the declared `unit` and `quantity` where
the CDL source declares one, then (parameters only) `range:` with the catalog's value rules
joined by `; `. A section with nothing in it prints `(none)`. Lines are never wrapped.

## `oce verify`

One vector set and one subject per run; there is no batch mode ([vectors
note](../../../docs/design/vectors.md), "CLI"; the owner's rulings in "Part 4's readings").

- **The subject's form** is `--subject-form` when given; else CXF for a name ending in `.jsonld`
  or `.cxf.json` (tested before the `.json` rule, so `rule.cxf.json` is CXF), text for `.oce`,
  wire for any other `.json`; else `unknown-subject-form`. A CXF subject goes to the engine
  untouched. A text or wire subject runs through both validation stages and `oce_author::emit`
  with `EmitOptions::default()`; an error finding is `subject-load-failed` (exit 2), with the
  findings in the report. There is no `--namespace`: run `oce emit` first for another one.
- **Reads.** Each size is checked from metadata before a byte is read: the vector file at 4 MiB,
  a CXF subject at 8 MiB (`oce_api::MAX_CXF_BYTES`), a text or wire subject at its form's 4 MiB,
  each mutant at 8 MiB, and the mutants together at 64 MiB (`too-many-mutant-bytes`). `-` is a
  file name here, not stdin.
- **Mutants** are always CXF. Each listed path is joined onto the vector file's directory and
  canonicalized, following links. A path that resolves outside that directory is
  `mutant-outside-directory`, and two listings that are one file are `duplicate-mutant`. One file
  is one (device, inode) on Unix, which catches case twins, normalization twins and hard links
  where the file system folds them, and one canonical path on Windows. Each mutant must be a
  regular file by its metadata before it is opened, so a named pipe or socket is `not-a-file`
  and never blocks the run. It is then opened once, and its identity, size and bytes come from
  that handle.
- **Stdout** carries one document. When the run reaches a verdict it is the receipt
  (`oce/receipt/v1`, exactly `oce-verify`'s canonical bytes), for exit `0` and for exit `1`.
  Otherwise it is the JSON report, exit `2`.
- **With `--out DIR`** the receipt is written atomically to `DIR/receipt.json`: a fresh temporary
  file beside it, synced, then renamed over its directory entry, so no reader ever sees part of a
  receipt. Stdout is the report, with `passed`, `status`, `receipt_id` and `output`. `DIR` must
  exist. On exit `2` nothing is written. A regular file already at `DIR/receipt.json` is
  replaced (ruled by the owner on 2026-11-22). Two guards, each `at` `receipt.json` (issue #758,
  ruled by the owner on 2026-11-22; **breaking in place**: until then a link there was written
  through to its target, a named pipe there was replaced, and a vector set or subject there was
  replaced by its own receipt):
  - before anything is read, an empty `DIR` is `write-failed` (a missing directory, as
    `--commission` refuses it), and a `DIR/receipt.json` that is a symbolic link, dangling or
    not, a directory, a named pipe, a socket or a device is `output-not-a-file`, whose detail
    says to remove or replace that entry ([below](#writing-a-file));
  - after the reads and before the run, a `DIR/receipt.json` that is the vector file, the subject
    or a mutant, by path, through a link the input is named by, or (on Unix) as a hard link, is
    `output-is-input`, naming the input as given.
- **Exit-only flags.** These never change a receipt byte:
  - `--require-mutants-caught` makes exit `1` when a mutant survived or the set lists none (owner
    ruling of 2026-11-11);
  - `--deny-warnings` makes exit `1` when `load_warnings_total` or `totals.warnings` is not zero.
- **`--timings`** fills the header's `timings`, which `receipt_id` never covers:
  - `load_ms`: reading the files and emitting a text or wire subject;
  - `run_ms`: the whole `run_vectors` call;
  - `compare_ms`: `0`, because comparing cannot be timed apart from running outside the library.
- **The in-repo replay gate.** `tests/replay_gate.rs` runs the built binary over both in-repo
  library rules, read in place, and requires exit `0` and stdout byte-equal to `oce-verify`'s
  golden receipts. It runs in the leverage lane on every PR. This is the M3 "In-repo rules
  replay" evidence.

## `oce verify --commission`

One reference of a commissioning spec (`oce/commissioning-spec/v1`, OBC §12.7's Listing 12.1 read
as published) run against its trends over `oce_verify::run_commissioning`, and the
`kind: "commissioning"` receipt ([commissioning note](../../../docs/design/commissioning.md),
"CLI", decisions 21 and 33 to 37; issue #664). One spec and one reference per run; there is no
batch mode and no `oce commission` verb (decision 35; vectors note decision 34).

- **Usage.** `--commission` excludes `<VECTORS>`, `--subject`, `--subject-form` and
  `--require-mutants-caught`, and `--reference`, `--bounds` and `--data` need `--commission`;
  `--bounds` also needs `--out`. Each is clap's usage error, exit `2`, on stderr. The human
  rendering is the global `--render`; there is no `--format` (decision 36).
- **The reference.** `--reference N` is the zero-based index into `references[]`. A spec with one
  reference needs no flag; one with several and no flag is `reference-required`, whose detail
  lists each index with its `sequence` and `model`. An index past the end is the library's
  `reference-out-of-range`. A reference with no `cxf` or no `window` (`reference-incomplete`), or
  under the `aligned-grid` envelope with no `sampling`, its own or the spec's
  (`sampling-required`; issue #792), is refused by the library **before any other file is
  read**. A `funnel` reference with no `sampling` runs: its outputs are compared by change of
  value, each series held from sample to sample and judged over the stretches between them
  (issue #746), and its receipt has no `time.sampling` and gives each output's
  `last_sample_t`.
- **Reads.** The spec at 4 MiB (`SpecLimits::MAX.max_bytes`), then the files the reference names:
  its `cxf`, its `pointNameMapping` when that is a path, and its trend sources in the receipt's
  order (`inputs`, `expected` or, with no `expected` key, the `controllerOutput`,
  `indicatorSources`), a path named twice read once. Each path resolves against the directory of
  `SPEC` as given (a spec reached through a symbolic link reads the files beside the link, not
  beside its target), which is opened once and is the handle the spec itself is read through
  ([below](#reading-a-file-a-document-names)), and may not leave it: its segments are joined onto that directory and
  canonicalized, following links, and a path that resolves outside is `file-outside-directory`
  (the rule `mutant-outside-directory` holds a mutant to; the spec reader has already refused an
  absolute path, an empty, `.` or `..` segment and a backslash as `path-invalid`). The canonical
  path is then opened without following a link anywhere below the directory
  ([below](#reading-a-file-a-document-names)): its last entry must be a regular file
  (`not-a-file`; a named pipe never blocks the run), a link or a changed directory met on the way
  was swapped in after the canonicalization and is `read-failed`, the file is opened once, and its
  size is judged from that handle before a byte is read. A file that grows after its metadata is
  read is read up to its bound and one byte more, then refused:

  | File | Bound | Refusal |
  | --- | --- | --- |
  | the spec | 4 MiB | `input-too-large` |
  | the CXF | 8 MiB (`oce_api::MAX_CXF_BYTES`) | `input-too-large` |
  | the mapping | 4 MiB | `input-too-large` |
  | a trend source | 1 GiB (`TrendLimits::MAX.replay.max_source_bytes`) | `input-too-large` |
  | the mapping and the trend sources together, each path once, the CXF not counted | 1 GiB (`TrendLimits::MAX.max_total_bytes`) | `source-refused`, cause `too-many-total-bytes` |

  These are the library's own limits, counted its way and refused under its words, so a run the
  CLI lets through is never refused for its size later (the orchestrator's call on issue #664).
  The Building 33 files, 11.6 MB, fit. **Memory:** a run holds the file bytes and, beside them,
  a few times the bytes of its expected and indicator files, the merged inputs and the trace
  (the commissioning note's "What a run holds": four expected `csv` files of 636 MB peaked at
  4.09 GB resident in the review of GitHub #744). Every refusal of a file is at the spec field that
  names it (`references[0].inputs[1].path`) and names the path as the spec writes it.
- **Stdout** carries one document: the receipt (exactly `oce-verify`'s canonical bytes) when the
  run reaches a verdict, exit `0` when every compared output passed and `1` for `fail` or
  `no_data` (the receipt says which); the JSON report otherwise, exit `2`, with the refusal in
  `errors[]` under its own code: the spec reader's, the source reader's (`source-refused` and its
  cause), the unit table's (`unit-refused` and its cause), a key selection's (`spec-refused` and its
  cause), an engine refusal (`frame-refused` and its class, `run-refused`), a comparison
  (`compare-refused`), the tool's own.
- **With `--out DIR`** stdout is the report, with `passed`, `status`, `receipt_id`, `output`
  (`DIR/receipt.json`) and `curves`: one `{ "files", "index", "output" }` per compared output,
  its index in the receipt's `outputs[]`, its canonical path and every file written for it, so no
  reader infers anything from a file that is absent. The files (decision 37):
  - `DIR/outputs/<k>/errors.csv` for every compared output, `<k>` the index in decimal with no
    padding: the exceedance at each test instant;
  - under `--bounds`, beside it, `reference.csv`, `test.csv`, `lowerBound.csv` and
    `upperBound.csv` for each output whose envelope was built (a Real output with at least one
    compared instant and no non-finite engine sample: the receipt's `half_width` is present
    exactly for those; the orchestrator's call on issue #664). With no `sampling` they hold the
    funnel's own corners, while the verdict reads corners snapped within `4 ε (m + w)` of a
    series instant (issue #746);
  - `DIR/receipt.json`, last, so a `receipt.json` this run wrote means its curves are complete.

  Each CSV file is pyfunnel's shape: the line `x,y`, then one line per point with both numbers in
  C's `%.16g` (`oce_verify::format_g16`). An instant where the engine computed nothing (in a
  stretch a missing sample left uncovered) is `nan`; every compared instant of an output failed by
  a non-finite engine sample is `inf`. A CSV file holds numbers under a fixed header and no name,
  and is named by index, so no name a spec, mapping or CXF writes reaches a file name or a cell.
  Before anything is read, an empty `DIR` is `write-failed` (a missing directory), and a
  `DIR/receipt.json` that is a symbolic link, dangling or not, a directory, a named pipe, a socket
  or a device is `output-not-a-file`, the rule of every verb that writes, whose detail says to
  remove or replace that entry ([below](#writing-a-file); issue #758, ruled by the owner on 2026-11-22; until then
  these were `write-failed`, and the orchestrator's call on issue #758 moved them so one cause has
  one code). After the reads and **before the run**: `DIR` must be an existing directory (it is
  never created) and `DIR/outputs` must not exist (`write-failed`: the curves of two runs are
  never mixed); a `DIR/receipt.json` that is the spec or a file the run reads, by path, through a
  link the input is named by or (on Unix) as a hard link, is `output-is-input`; and a regular file
  at `DIR/receipt.json` is `write-failed`, so a receipt this run did not write never stands beside
  its curves (the orchestrator's call in the review of GitHub #757). A refusal leaves nothing
  written. The checks open `DIR` once, and everything below it goes through directory handles
  (issue #771; [below](#writing-a-file)): `outputs/` and each `outputs/<k>/` are made with
  `mkdirat` and opened with `O_DIRECTORY | O_NOFOLLOW` from their parent's handle, and each file,
  `receipt.json` included, goes through the atomic writer relative to the handle of the directory
  it goes into, which never follows a link at its name (a link planted after the check is
  replaced, its target untouched). So no directory swapped for a link, and no `DIR` exchanged,
  during the run can carry a curve outside the directory the checks opened. A `DIR` that cannot
  be listed (mode `--x` or `-wx`) is now `write-failed` ("the output directory cannot be read:
  permission denied"), and a `DIR` removed during the run is `write-failed` at `outputs`. Each
  file is created owner-only (`0600` on Unix; the directories `outputs/` and `outputs/<k>/` take
  the process's default mode). A directory made and then not opened (a umask that leaves it
  unreadable, no descriptor left) is removed again and named as such ("it was made but could not
  be opened (...), and is removed"). A write that fails part-way is `write-failed` naming the
  file; what was made before it stays: `outputs/`, an `outputs/<k>/` that may be empty, and the
  curves already written, with no receipt (without `receipt.json` they are not a run's output),
  and the report's `curves` lists the files (an output none of whose files was written is not
  listed). No trace file is written.
- **Exit-only and label flags.** `--deny-warnings` makes exit `1` when `load_warnings_total` or
  `warnings_total` is not zero. `--timings` fills the header's `timings` (`load_ms`: reading every
  file; `run_ms`: the whole `run_commissioning` call; `compare_ms`: `0`). `--data` is the
  receipt's `data` label, `trended` when absent (decision 41), inside `receipt_id`. No receipt byte
  outside `timings` depends on `--deny-warnings`, `--timings` or `--render`.
- **What it does not resolve.** Masked ticks keep OBC §12.7.2's zero fill (issue #745), and the
  library's other open items are issue #747. The Building 33 worked example is issue #755. A
  step-shaped `aligned-grid` band (#792), a limit on how long a silent trend is held (#793) and a
  time tolerance for Boolean and Integer outputs (#794) are follow-ups of #746.

## `oce id`

`oce id <FILE>` prints the canonical content id of one input (`oce_api::Engine::content_id`:
`oce:canon1:` and 64 lowercase hex digits) and one line feed, and nothing else
([content-id note](../../../docs/design/content-id.md); issue #513). It is the id a library card
is stamped with, so a gate can compare a stamp with this output byte for byte.

```sh
oce id rule.cxf.jsonld            # oce:canon1:fadd…2d7b
oce id econ_enable.oce            # the id of the CXF `oce emit econ_enable.oce` writes
oce emit econ_enable.oce | oce id - --from cxf   # the same line
oce id rule.cxf.jsonld --json     # the report, with content_id
```

- **The form** is `--from cxf|text|wire` when given, and `--from` is required for stdin (`-`,
  `missing-from`). Otherwise it is read from the name as `verify` reads a subject's: CXF for a
  name ending in `.jsonld` or `.cxf.json` (tested before the `.json` rule), text for `.oce`, wire
  for any other `.json`; else `unknown-form`. A CXF document named `rule.json` is therefore read
  as wire and refused by the wire reader; pass `--from cxf`.
- **A CXF document** goes to a fresh engine untouched. One the engine refuses is `not-loadable`,
  with the engine's codes under `diagnostic_codes` and `at` `"input"`. A document that loads with
  warnings has an id: warnings are not in the output.
- **A text or wire file** is emitted first, exactly as `oce emit` would: the parse, the emitter,
  then both validation stages. A file `oce emit` would refuse has no id: `not-loadable` (exit
  `2`, not `emit`'s `1`), with the findings and `stages` in the report, a document too large for
  an engine included (`document-too-large`). So `oce id FILE` is always the id of what
  `oce emit FILE` writes. `tests/id_files.rs` holds the two verbs to each other over every
  tracked authored fixture.
- **`--namespace`** follows `emit`'s rule (a malformed one is `invalid-namespace`). A namespace
  never moves the id, because the id leaves the instance namespace out; one whose IRIs the
  engine refuses (`#` alone, which makes every IRI relative) leaves the document with none:
  `not-loadable`, with the engine's codes. Beside a CXF document the flag is
  `namespace-needs-authored`: a CXF document is never rewritten.
- **Reads** are bounded by the form: a CXF document at 8 MiB (`oce_api::MAX_CXF_BYTES`), a text
  or wire file at 4 MiB, a file's size from its metadata before a byte is read, stdin one byte
  past the limit.
- **Stdout and exit.** Exit `0` with the id line (the report under `--json`, with `content_id`
  and `form`); exit `2` with the report for every refusal. `id` never exits `1`: it answers no
  question that has a "no".

## `oce import`

`oce import <FILE>` reads one flat CXF document into the authoring form
(`oce_author::import_cxf`; [importer note](../../../docs/design/importer.md); issue #514) and
prints the sequence as canonical text, or with `--wire` as the canonical wire form. It is how a
rule committed as CXF becomes a file a person or an agent edits.

```sh
oce import rule.cxf.jsonld > rule.oce          # canonical text
oce import rule.cxf.jsonld --wire > rule.json  # the wire form
oce import rule.cxf.jsonld -o rule.oce         # written atomically; the report on stdout
oce import rule.cxf.jsonld | oce emit - --from text | oce id - --from cxf
```

- **The input** is CXF, the one form the verb reads. There is no `--from`, stdin (`-`) needs
  none, and the file's name is not consulted. The frozen prototype's v0 graph JSON
  (`oce_author::import_graph_v0`) is not exposed.
- **The output's form** is text, or wire with `--wire`. The flag alone chooses it; `-o`'s name
  never does. But a name `oce` would read as **another** form is refused before anything is
  read (`output-form-mismatch`, exit `2`, nothing written; owner ruling of 2026-11-21): the
  name is read by the rule `id` reads a file's name by, so `.oce` is text, `.json` is wire, and
  `.jsonld` and `.cxf.json` are CXF, which `import` never writes. `-o rule.json` needs `--wire`,
  `--wire -o rule.oce` is refused, and `-o rule.cxf.json` is refused for either form. A name
  that gives no form (`rule`, `rule.txt`, `rule.JSON`: the rule is case-sensitive) is accepted
  for either.
- **The output is never the input.** `-o` naming the input file is `output-is-input` (exit
  `2`, nothing written), for `import` and for `emit` alike (owner ruling of 2026-11-21).
  "The input file" is file identity, not spelling: the same path written another way
  (`./rule`, `dir/../rule`), the file an input named through a symbolic link resolves to and,
  on Unix, a hard link to it, where one file is one (device, inode). On Windows one file is one
  canonical path, so a hard link is not recognised there. An output that does not exist yet is
  no file and replaces nothing. An `-o` that is itself a symbolic link, dangling or not, a
  directory or another non-file is refused before this guard as `output-not-a-file`
  ([below](#writing-a-file)), whatever it names. Stdin (`-`) is the input when, on Unix, the shell made it a
  regular file (`oce import - -o doc < doc`, `oce emit - --from text -o doc < doc`): file
  descriptor 0 is compared by (device, inode), and a pipe, a terminal or a closed descriptor is
  no file, so `cat doc | oce emit - --from text -o doc` is not refused and replaces `doc`.
  **On Windows stdin is not checked** (std has no stable file index); the check also
  reads the process's own descriptor 0, never a reader a host passes in.
- **Nothing is judged.** The importer decodes; whether a class is in the catalog, a name is a
  reserved word or the graph has a loop is `oce validate`'s answer on the result. So `import`
  never exits `1`, and a sequence it writes may still be one `oce emit` refuses.
- **The document is read back before it is written.** The form's writer runs, then the form's
  own reader over what it wrote, and the document is produced only when that reads back equal
  to the imported sequence: exit `0` means `oce` reads what it wrote. With the name rule above
  that holds by name too: a file written as `rule.oce` or `rule.json` is read by `validate`,
  `emit`, `fmt` and `id` in the form it was written, with no `--from`. Otherwise the refusal is
  `unspellable`. The text form has no spelling for a class path that is not dotted words, or
  for a connection whose source is a statement keyword; neither form has one for a string
  past 4,096 bytes; and the wire form has none for a sequence whose wire document would pass
  its 4 MiB, which the terser text form may still carry. For a name or a class path the text
  form cannot spell, `--wire` is the way to get the sequence out and let `oce validate` name
  what is wrong with it.
- **A refusal by the importer** is exit `2` under the importer's own code
  (`oce_author::ImportError::code`, one of the thirty-one in `oce_author::IMPORT_ERROR_CODES`),
  with what the kind says as `detail` and the refusal's subject as `at`: a node `@id`, or
  `document`. The tool matches no kind, so a kind `oce-author` adds is reported under its own
  code. `oce vocab` lists all thirty-one under `import_codes` with their meanings, in the
  importer's order. Four of them no run earns: the three of the v0 graph JSON reader
  (`limit-exceeded`, `string-value`, `malformed-connection`), and `too-large`, because the
  tool's bounded read stops at the importer's own limit and reports `input-too-large`.
  `tests/import_refusals.rs` earns each of the other twenty-seven.
- **The report** is stdout under `-o` and on every refusal. `dropped` lists what the importer
  read and did not carry, in reading order: each `{ "property", "subject", "value" }`, with
  `property` one of `root-type`, `label`, `unit`, `quantity`, `display-unit`, `min`, `max` and
  `value` as the document wrote it. `normalised` lists each boundary or port typed by a
  constant package and read as the Integer the engine reads it as:
  `{ "data_type", "subject", "to" }`. Without `-o` the document is stdout and the report is
  not printed; the rendering on stderr then counts both lists.
- **Reads** are bounded at 8 MiB (`oce_api::MAX_CXF_BYTES`), a file's size from its metadata
  before a byte is read, stdin one byte past the limit.

### The round trip

`oce import FILE | oce emit - --from text | oce id - --from cxf` prints the line `oce id FILE`
prints wherever `oce emit` accepts the imported sequence, with one exception by design:

**A name prefix on the root** (importer note, decision 14). The importer keeps the trailing
identifier of the root `@id` as the sequence name, and names are inside the id. A root such as
`http://example.org#g36.source.reheat_overrides` carries `g36.source.` between its namespace
and that identifier, which no emitted document can spell (a namespace ends in `#`, `/` or
`:`). The round trip has the id of the original with its root re-stamped to
`<namespace><sequence>`. Every G36 corpus fixture the importer accepts is in this class.

An Integer literal on a Real parameter is not a second case (issue #683). The importer keeps
`k = 0` as written and the emitter writes a Real parameter's value as a Real, and the engine
holds an Integer bound to a Real parameter as that Real, an element of a Real array included,
so both spellings have one id. Until engine 0.6.0 the id told them apart and this round trip
moved it.

`oce emit` answers no wherever the validator has an error finding, and the validator is
stricter than a bare engine load. A document that loads with a warning has an id, and its
imported sequence is refused by `emit` ([`oce id`](#oce-id) has the same asymmetry). An
Integer literal above 2^53 on a Real parameter is refused by the engine and by stage 1 alike
(`parameter-out-of-range`): neither will hold a Real it cannot hold exactly, so such a document
imports and has no id.

`tests/import_corpus.rs` runs this over every `.jsonld` and `.cxf.json` file git tracks under
`crates/` (the vendored tree under `third_party/` is left out, so a pin advance does not move
it) and writes one row per document to
[`tests/fixtures/import_corpus.tsv`](tests/fixtures/import_corpus.tsv): the class, its detail,
how many properties the importer dropped and normalised, and the path. That table is the
record of what each tracked fixture does; it is blessed, so a fixture added to any crate gets
its row from one `OCE_BLESS_CLI=1` run, and no count is written down here. The classes:

| Class | Meaning |
| --- | --- |
| `refused` | the importer refuses the document under one stable code (the detail), and writes nothing |
| `same-id` | imported, emitted, and the id is the original's |
| `same-id-restamped` | imported, emitted, and the id is the original's once its root is re-stamped; the detail is the name prefix |
| `warned` | imported; the original loads with a warning and `oce emit` answers no; what the emitter alone writes has the original's id |
| `author-refused` | imported; the original loads, and stage 1 refuses the sequence, so nothing is emitted |
| `no-id` | imported; the original does not load in an engine, so it has no id, and `oce emit` answers no |

A round trip whose id differs for a reason other than the two above fails the test: that is a
finding, not a class. One split is asserted by hand, the importer note's: of the 46 G36
catalog fixtures, 31 import and round-trip under the re-stamp and 15 are refused. For each
imported document the test also holds the text and the wire output to the sequence
`oce_author::import_cxf` returns, each canonical in its form, `oce emit` to one answer for
both, and the report's lists to the importer's.

## `oce diff`

`oce diff <BASE> <HEAD>` compares two revisions of a sequence and prints the structural diff
([diff note](../../../docs/design/diff.md), "CLI", decisions 1, 10 and 16; issue #668), over
[`oce-diff`](../oce-diff/README.md): which blocks were added, removed, renamed, retuned or rewired,
which sequence parameters, boundaries and metadata changed, with authored block names as the
identity of a block.

```sh
oce diff reset.v1.oce reset.v2.oce                       # the report with the diff; exit 0 / 1 / 2
oce diff reset.v1.oce reset.v2.oce --format markdown     # the diff as a Change | Subject | Was | Now table
oce diff rule.cxf.jsonld draft.cxf.jsonld --render always # the text rendering on stderr as well
oce import rule.cxf.jsonld | oce diff draft.oce - --from text
```

- **Two sides, one form.** `--from cxf|text|wire` reads both sides in that form and is required
  when a side is stdin (`-`). Otherwise each side's name is read by `id`'s rule (owner ruling of
  2026-11-21): CXF for a name ending in `.jsonld` or `.cxf.json`, text for `.oce`, wire for any
  other `.json`; else `unknown-form`. Two names that read as two forms are `mixed-forms`: a diff
  across levels is out of scope. Stdin is read once, so `-` on both sides is `stdin-twice`.
- **Authoring sides are parsed and never validated** (decision 10): an agent's broken draft, one
  `oce validate` refuses, is diffed against the reference it is repairing (exit 1, not 2). A
  parse refusal is the form's own code and span.
- **CXF sides** are each loaded into a fresh engine. One the engine refuses is `not-loadable`,
  with the engine's codes under `diagnostic_codes`; one that loads with warnings is diffed. The
  two topologies and canonical content ids go to `oce_diff::diff_topology_with_ids`, and its
  refusals pass through under their own codes (`oce vocab` lists them under `diff_codes`):
  `outside-root` for a block or boundary not named under its document's root, which is every
  block of a document the engine exported (GitHub #717), and `unresolved-ports`, which no loaded
  document reaches. At this level sequence parameters are grounded into the blocks, so
  `parameters_equal` is `null`, and authored order is not compared.
- **`--no-renames`** turns the rename pass off: every unmatched block is removed or added, the diff
  carries the note `rename-pass-off`, and `summary.graph_equal` does not change.
- **Stdout and exit.** Exit `0` when the diff is `equal` (every section empty), `1` for any
  difference, a `reordered`-only or a comment-only diff included (decision 16: a script that asks
  "same structure?" reads `summary.graph_equal`), with the diff on stdout either way; exit `2` with
  the report and no diff for every refusal. By default (`--format json`) stdout is the report with
  the diff under `diff`, exactly the data `oce_diff::Diff::to_json` writes, so removing the
  envelope leaves `oce-diff`'s own JSON. With `--format markdown` stdout is
  `oce_diff::render_markdown`'s table, for exit `0` and `1` alike (an equal pair is the two header
  lines). There is no `--format human` (owner ruling of 2026-11-21): the text rendering,
  `oce_diff::render_text`, is what `--render` writes to stderr, each line escaped as every other
  rendering is; a refusal is rendered as a report, each error at the file of its side.
- **Reads** are bounded by the form: an authored side at 4 MiB, a CXF side at 8 MiB
  (`oce_api::MAX_CXF_BYTES`), a file's size from its metadata before a byte is read, stdin one
  byte past the limit. The base is read before the head, and each refusal carries `at`, `base`
  or `head`.
- **No file is written.** The verb has no `-o`: redirect stdout.

## Exit codes

| Exit | Meaning | When |
| --- | --- | --- |
| `0` | yes | `validate` and `emit`: no error finding; `fmt --check`: canonical; `catalog`: the ports resolve; `verify`: every scenario passed, under `--require-mutants-caught` the set lists a mutant and none survived, under `--deny-warnings` no warning; `verify --commission`: every compared output passed, under `--deny-warnings` no load or run-time warning; `id`: the input has an id; `import`: the document is imported and written; `diff`: the diff is `equal`; `fmt`, `fmt --write`, `vocab`, `--help`, `--version` |
| `1` | no | an error finding (`validate`, `emit`, which then writes nothing); `fmt --check`: not canonical; `catalog`: an unknown class, or the class rules refuse the parameter table (including a required parameter with no default); `verify`: a scenario failed or the set is `no_data`, or, under their flags, a surviving mutant, a set that lists no mutant, or a warning; `verify --commission`: an output failed or the run is `no_data`, or, under `--deny-warnings`, a warning; `diff`: any difference, a reordering or a comment included |
| `2` | error | the question could not be answered: a usage error (clap), stdin without `--from`, an unknown extension, a missing path or a directory, an unreadable or over-size input, a parse refusal, a malformed `--namespace` or `--param`, `--write` on stdin or a wire file, a failed write, or a stdout that cannot be written; for `verify`, also a refused vector file, a subject that does not load, an unresolved name, an uncovered window, an invalid or unreadable mutant, or a run refusal; for `verify --commission`, also a refused spec, mapping, unit or key, a spec with several references and no `--reference`, a reference with no `cxf` or `window`, or under `aligned-grid` with no `sampling`, an unreadable, over-size or refused trend source, a path out of the spec's directory, a CXF that does not load, an engine refusal of a frame, a comparison the limits refuse, an `--out` whose `outputs/` or a regular `receipt.json` exists; for both forms of `verify`, an `--out` whose `receipt.json` is an input; for `id`, also a document the engine does not load, an authored file with an error finding, or `--namespace` beside a CXF document; for `import`, also a document the importer refuses, a sequence the output form cannot spell, or an `-o` name that reads as another form (`import` never exits `1`); for `emit` and `import`, an `-o` that is the input file; for `emit`, `import`, `fmt --write` and both forms of `verify --out`, an output that is a symbolic link, a directory or another non-file; for `diff`, also a mixed pair, stdin on both sides, a CXF side the engine does not load, or a document whose blocks are not named under its root |

## Output

Stdout carries **exactly one document** per run:

- the command's product when it succeeds and has one — the CXF (`emit` without `-o`), the
  canonical text (`fmt`), a text listing (`catalog`, `vocab` without `--json`), the receipt
  (`verify` without `--out`), the content id and a line feed (`id` without `--json`), the
  imported sequence as text or wire (`import` without `-o`), the diff's Markdown table
  (`diff --format markdown`);
- otherwise the JSON report below (every exit `1` and `2`, `emit -o`, `import -o`, `fmt --check`, `fmt --write`,
  `catalog --json`, `vocab --json`, `id --json`, `verify --out`, `diff`). **Two exceptions:** a
  `verify` verdict of exit `1` prints the receipt, not the report, because the receipt is the
  answer, and so does `diff --format markdown` print its table for exit `1`;
- nothing for a clap usage error (its message is on stderr) and clap's text for `--help` and
  `--version`.

Everything is deterministic: no clock, no locale, no terminal width, LF line endings, one final
line feed, and the program is always named `oce` whatever path started it. No path appears that
was not given as an argument: `input` and `output` are echoed exactly as given (`-` for stdin), and
I/O failures are reported by kind, never with an operating-system message that could carry one.

The rendering on stderr (`--render`) is for a terminal, and a report quotes strings a
document wrote: a wire file's block `class` in a `class-not-found` finding, a CXF node's `@id`
in an `import` refusal. So every Unicode format or control character in a rendered line, and
every line or paragraph separator (U+2028, U+2029), is written as a `\u{…}` escape, the code
point in at least four upper-case hexadecimal digits (`\u{001B}`, `\u{202E}`), which is how
the engine's own messages spell such a character. An escape sequence, a line feed, a
zero-width space or a right-to-left override in a document cannot move the cursor, start a
line, hide or reorder one. A backslash is written as it is, so the rendering does not tell a
U+202E from the characters `\u{202E}` a document spelled out: the rendering is advice for a
person, the JSON on stdout is the exact form (its writer escapes what JSON requires, and
this changes none of it), and doubling backslashes would double every separator of a Windows
path in those lines. Until GitHub #688 a wire file whose `class` held a right-to-left
override, a zero-width space or a line separator reached the terminal as it was through
`validate` and `emit`.

## The JSON report (`schema: "oce-report/1"`)

One JSON object, keys sorted at every depth, two-space indentation. The common keys are always
present:

| Key | Type | Meaning |
| --- | --- | --- |
| `tool` | `"oce"` | |
| `schema` | `"oce-report/1"` | the report contract's version; a breaking change bumps it |
| `command` | string | `emit`, `validate`, `fmt`, `catalog`, `vocab`, `verify`, `id`, `import` or `diff` |
| `input` | string or `null` | the input path as given, `-` for stdin (the vector file for `verify`, the spec for `verify --commission`, the base for `diff`); `null` for `catalog` and `vocab` |
| `ok` | boolean | `true` exactly when the exit is `0` |
| `findings` | array | the answers that make the exit `1` (below) |
| `errors` | array | why the question could not be answered; non-empty exactly when the exit is `2` |

A **finding** is `{ "code", "detail", "origin", "severity", "span", "subject" }`: `code` an exact
string (a stage-1 code, an engine `DiagCode` spelling from stage 2 or the class rules,
`document-too-large`, `load-failed`, `class-not-found`), `origin` `author` or `engine`, `severity`
`error` (every finding stage 1 or 2 reports today) or `warning`, `subject` a model path (`hys`,
`hys.uHigh`, `connection[2]`, `sequence`; the class path for `catalog`), `span` `{ "line",
"column" }` (1-based, byte column) for text input and `null` otherwise. Findings keep the
validator's order (authored position, code, detail).

An **error** is `{ "code", "detail", "span" }`: a text-form parse code (`oce_author::PARSE_CODES`,
with the refusal's span), a wire-form code (`oce_author::WIRE_CODES`, with the refusal's span
where it has one), or one of
the tool's own codes (`span` `null`). `oce vocab` lists every code with one line of meaning. An
`id` document the engine refuses carries `at` (`"input"`) and `diagnostic_codes` from the keys
below. An `import` refusal by the importer carries one of `oce_author::IMPORT_ERROR_CODES` and
`at`, the refusal's subject (a node `@id`, or `document`; absent when the subject is an empty
`@id`). A `verify` refusal adds optional keys, each written only when it has a value (owner rulings of
2026-11-11; pinned by `tests/report_shape.rs`):
- `at`: the place, such as `scenarios[0].inputs.u`, `mutants[2]`, `subject` or `line 2 column 1`
  (never `subject`, which is the report's subject file);
- `path`: the canonical path it concerns;
- `candidates`: an ambiguous name's matches, in order;
- `cause`: from one of `oce-verify`'s closed tables;
- `diagnostic_codes`: the engine's codes for a subject that did not load.

Command keys:

| Command | Keys |
| --- | --- |
| `validate` | `stages`: the stages that ran, `["author"]` or `["author", "engine"]` (`[]` after a read or parse refusal); stage 2 runs only when stage 1 found no error and `--author-side` is absent |
| `emit` | `stages` as for `validate`; with `-o`, only once the write succeeded, `output` (the path as given) and `bytes` (absent from a failed write's report) |
| `fmt` | `form` (`text`, `wire`, or `null` before the form is known); with `--check`, `canonical` (whether the input is already canonical **in its own form**: text against `fmt`, wire against the canonical wire writer); with `--write`, only once the file is known canonical on disk (unchanged, or rewritten), `changed` and `output` (absent from a failed write's report) |
| `catalog` | without a class, `classes`; with one, `class` (`null` when the class is unknown or a `--param` is malformed) and `params` (the `--param` strings as given). A class is one closed object, the same in both ([below](#a-catalog-class)) |
| `vocab` | `keywords`, `types`, `tags`, `base_units`, `scale_factors` ([below](#scale-factors)), `parse_codes`, `finding_codes` (each with `stage`), `wire_codes`, `import_codes` (`oce_author::IMPORT_ERROR_CODES`, in order), `error_codes`, `vector_codes` and `verify_codes` (`oce-verify`'s load, bind and run codes, pinned to its lists), `spec_codes`, `source_codes`, `unit_codes` and `frame_refusal_codes` (`verify --commission`: the spec reader's codes, and the causes of `source-refused`, `unit-refused` and `frame-refused`, each pinned to `oce-verify`'s list), `diff_codes` (`oce_diff::DiffError::CODES`, in order, the two of the blob comparison `diff` never reads included), `exit_codes` (each `{ "code", "meaning" }`), `enumerations` (below) |
| `id` | `form` (`cxf`, `text`, `wire`, or `null` before the form is known); `content_id`, only when the input has one (so only with `--json`, exit `0`); `stages` as for `validate`, only once a validation stage of a text or wire file ran (absent after a read, parse or namespace refusal, and for a CXF document) |
| `import` | `output_form` (`text` or `wire`, always); once the importer accepted the document, `dropped` and `normalised` ([above](#oce-import)), so they are absent after an output-guard, read or importer refusal and present beside an `unspellable` or `write-failed` error; with `-o`, only once the write succeeded, `output` (the path as given) and `bytes`. Never `stages`: no validation stage runs |
| `verify` | `subject` (the subject's path as given; not for `--commission`); with `--out`, only once the receipt is written, `passed`, `status` (`pass`, `fail`, `no_data`), `receipt_id` and `output` (`DIR/receipt.json`, with `/`), and for `--commission` `curves` (per compared output its `index`, its canonical path as `output` and the `files` written for it, `DIR` as given then `/outputs/<k>/<name>`); after a text or wire subject's validation, `stages` as for `validate` |
| `diff` | `head` (the head's path as given, `-` for stdin); `form` (`cxf`, `text`, `wire`, or `null` until both sides are known to be one form); `diff`, only once a diff is made (exit `0` and `1`): the `oce-diff/1` object `oce_diff::Diff::to_json` writes, its keys sorted as the report's are. Never `stages`: nothing is validated. An error about one side carries `at`, `base` or `head`; `mixed-forms` and `stdin-twice` are about the pair and carry none |

The tool's own error codes: `missing-from`, `unknown-form`, `not-found`, `not-a-file`,
`read-failed`, `input-too-large`, `invalid-namespace`, `unspellable`, `write-failed`,
`write-needs-file`, `write-needs-text`, `malformed-param`, `unknown-param`, `duplicate-param`,
`malformed-value`, `ports-unresolvable`, `emit-refused`, `unknown-subject-form`,
`mutant-outside-directory`, `namespace-needs-authored`, `not-loadable`, `no-content-id`,
`output-form-mismatch`, `output-is-input`, `output-not-a-file`, `mixed-forms`, `stdin-twice`.
`unspellable` is `fmt`'s for a wire file the text form cannot spell and `import`'s for a
sequence the output form cannot spell. The wire-form refusal codes are `oce-author`'s: the tool reports
`oce_author::WireError::code()` and `location()` and names no refusal itself (issue #495), so the
sixteen codes of `oce_author::WIRE_CODES` are documented once, in the
[`oce-author` README](../oce-author/README.md#wire-refusal-codes). `oce vocab --json` lists them
with their meanings under `wire_codes`, in that order. There is no fallback code: `wire-refused`,
which named a refusal the tool did not know and which no run could produce, is gone from the
listing.

### A catalog class

Every class object has exactly these keys, in `classes` and as `class`. The engine's facts come
from the facade's catalog; the meanings come from the block vocabulary
([`oce-vocab`](../oce-vocab/README.md)).

| Key | Type | From |
| --- | --- | --- |
| `class_path`, `description`, `reserved` | string, string, boolean | the catalog (`description` is the class one-liner) |
| `parameters` | array | each `{ "name", "type", "default": { "kind", "value" }, "settable", "enumeration", "description", "unit", "quantity", "range" }` |
| `inputs`, `outputs` | array, or `null` where the ports do not resolve (a required parameter at the default table) | each `{ "name", "kind", "description", "unit", "quantity" }`, resolved at the `--param` table |
| `notes` | array | each `{ "kind", "subject", "text" }` |
| `links` | object or `null` | `{ "pinned_source", "latest_help" }` |

- A parameter's `default.kind` is one of `real`, `integer`, `boolean`, `string`, `member`,
  `derived` (value: the formula) or `required` (value `null`).
- **`description`** is what the parameter or port means, **`unit`** and **`quantity`** what the CDL
  source declares, in Modelica spelling (`s`, `K`, `1`; `Time`). `unit` and `quantity` are `""`
  where the source declares none, which is most ports. A port's text is its connector's: `u1`,
  `u2` and `u3` of a `MultiSum` all carry the vector connector `u`'s.
- **`range`** is a list of strings, one per catalog rule that constrains the parameter's value,
  `[]` when there is none: a bound (`Ti >= 1e-13`), a relation, listed for both parameters
  (`uLow <= uHigh`), a relation the engine only warns about (`uLow == uHigh warns`; `validate`
  reports every engine warning as an error) and an element bound (`1 <= extract_<i> <= nin`). It
  is rendered from the catalog's rules on every run, never read from the vocabulary's data, so
  it cannot drift from what a load enforces. Ports have no `range`: the catalog has no rule
  about a port's value.
- **`notes`** are the curated notes: `kind` is `convention`, `pitfall`, `port-order`, `loop` or
  `units`; `subject` names a parameter or connector, or is `""` for the class; `text` is the
  note. A note's `evidence` (where the claim comes from) is for reviewers and is **never
  printed**, in text or JSON.
- **`links`**: `pinned_source` is the class's Modelica source at the Buildings commit the
  vocabulary was generated from; `latest_help` is its page in the latest Buildings release, which
  **may drift from the pin**. `null` for a class upstream does not have
  (`CDL.Logical.TrueHoldWithReset`).
- A **reserved** lowering identity has no vocabulary: its strings are `""`, its `notes` `[]` and
  its `links` `null`.

The report schema is still `oce-report/1`: these keys were added (issue #500) and none was
removed or changed, so a reader of the earlier shape still reads this one.

### Scale factors

`vocab`'s `scale_factors` is the vocabulary's table of unit symbols that are an exact integer
multiple of a base unit (`oce_vocab::SCALE_FACTORS`), in table order, each
`{ "symbol", "base", "factor" }` with `factor` a JSON integer: `min` 60, `h` 3600 and `d` 86400,
all of `s`. It is time only: `degC`, `%` and prefixed SI units are not in it. Every `base` is one
of `base_units` and no `symbol` is.

The text form reads the table (issue #501): one of these symbols alone as a unit suffix scales
the number with one rounding (`delayTime = 1 h` is `3600.0` s), on a parameter declared in the
base unit. An Integer is scaled by its exact product, a Real from the binary64 it parses to
(through its shortest round-trip decimal, not the digits as typed), so `fmt` never moves a value;
the [`oce-author` README](../oce-author/README.md) states the consequences. The output says so
itself. The text listing's heading reads "alone as a unit suffix on a parameter declared in the
base unit, it scales the number, rounding once"; the meaning of
`unit-not-base` under `parse_codes` names `min`, `h` and `d` as accepted alone, and
`unit-suffix-mismatch` under `finding_codes` is the stage-1 code for a suffix the parameter
cannot take, in the text and in the JSON. The JSON carries it there because the table's rows
have no place for prose; `tests/vocabulary.rs` holds the wording to the table and to what the
text form does.

### Enumerations

Both commands read the facade's enumeration and Integer constant-package table
(`oce_api::enumerations()`, GitHub #488). `vocab`'s `enumerations` lists every row in table order as
`{ "class_path", "kind", "members": [{ "name", "value" }] }`, `kind` `enumeration` (each value the
member's 1-based ordinal) or `constant-package` (each value its Integer; a package is never a
parameter type). A catalog parameter of an enumeration type has `type` `Enum` and `enumeration` the
type's authoring class path (`CDL.Types.SimpleController`; `null` for every other type), and its
`--param` value is a member name, bare (`PI`) or qualified by the type (`SimpleController.PI`); an
ordinal or another type's member is `malformed-value`.

## Bounds and hostile input

Every read is bounded by its form's limit (`oce_author::MAX_TEXT_BYTES`, `MAX_WIRE_BYTES`, 4 MiB;
for a CXF document given to `id`, `import` or `diff`, `oce_api::MAX_CXF_BYTES`, 8 MiB):
a file's size is checked from its metadata before a byte is read (a 100 MB file costs one `stat`),
and every read, stdin included, stops one byte past the limit. Stdin is bounded in size, not in
time: a pipe that never closes keeps `oce` waiting. Non-UTF-8, binary junk, an empty file, a
directory and a missing path each get one typed error code. Nothing panics.

`catalog` reads no file, and **its output has no cap of its own**. What bounds it is the
catalog's width rules: a width parameter is at most 1,048,576, so one class resolves to about
2^21 ports at most, and every port carries its connector's description. The widest case,
`oce catalog CDL.Routing.RealExtractSignal --param nin=1048576 --param nout=1048576 --json`,
wrote 416 MB and peaked at 3.1 GB of memory; `CDL.Reals.MultiSum` at `nin=1048576` is about
189 MB. The whole report is built in memory before a byte is written, and the text listing
builds it too, so asking for text does not make a wide class cheaper. Ask for the width you
mean; a cap on the output is a separate change.

An `import` report quotes the document: a refusal's `at`, and each `dropped` entry's `subject`
and `value`. Its size is bounded by the input's, which is bounded at 8 MiB.

### Writing a file

One rule holds every verb that writes a file: `emit -o`, `import -o`, `fmt --write` (whose file
is its input) and `verify --out` (the vector receipt, and every file of `--commission`) (issue
#758, ruled by the owner on 2026-11-22). **Breaking in place:** until then a target that was a
symbolic link was followed and the file it named replaced, and a named pipe or device at the
target became a regular file.

- **The target must be absent or a regular file.** A symbolic link, dangling or not, a
  directory, a named pipe, a socket or a device is `output-not-a-file`, exit `2`, decided before
  anything is read, so nothing is read or written. The detail names the path as given, what it
  is and the way out. The target's own entry is never written through: a link planted there is
  refused, or, planted after the check, replaced (below). **A caller whose output is a link gives
  the path of the file it names** (`oce emit rule.oce -o "$(realpath out.cxf.json)"`, `oce fmt
  --write "$(realpath link.oce)"`); `DIR/receipt.json` is a name `oce` chooses, so whatever is
  there is the caller's to remove or replace. `-o -` is stdout, so `/dev/stdout` (a link) is
  never needed; `-o /dev/null` (a device) is refused, where until then the temporary file could
  not be created beside it and the write failed. An empty `--out` is refused by both forms of
  `verify` as a missing directory (`write-failed`, before anything is read; the vector form
  wrote `./receipt.json` until issue #758's review).
- **Each write goes through its directory's handle** (issue #771; Unix). The directory a file
  goes into is opened once, and every step below it is relative to that handle. For a path the
  caller named (`-o lnk/x`, `--out lnk`, `fmt --write dir/x.oce`) that is its parent, and the
  links on the way are followed: the caller named them. For `verify --commission --out` it is
  `DIR`, opened by the checks before the run, then `DIR/outputs` and each `DIR/outputs/<k>`, made
  with `mkdirat` and opened with `openat(O_DIRECTORY | O_NOFOLLOW)` from their parent's handle; a
  directory just made must be empty when opened, and must be owned by the user who owns the
  temporary file created inside it, so a link, a file or a non-empty directory swapped in under
  the name is refused (`write-failed`) and so is a directory another user swapped in. A process
  running as the same user is not stopped by that check, and needs no swap: it can rewrite the
  outputs anyway. `mkdirat` returns no handle, so what is opened cannot be proved to be what was
  made; the two checks are what is held. A directory swapped for a link, or exchanged, after it
  was opened cannot redirect anything; a `DIR` renamed during the run takes its files with it,
  as a rename after the run would.
- **The write** goes to a uniquely named temporary file in that directory
  (`.NAME.<pid>.<counter>.oce-tmp`), created with `openat(O_CREAT | O_EXCL | O_NOFOLLOW)` so it
  never follows a link or truncates a file already there, synced, given the permissions of the
  file it replaces (Unix mode bits) and renamed with `renameat` over the target's own directory
  entry; on a failure the target is untouched and the temporary file is removed with `unlinkat`
  from the same handle, so none is left behind even when the directory's path was swapped
  meanwhile. A link planted between the check and the write is replaced, its target untouched.
  On Unix the temporary file is created no more permissive than the target, and a new file (no
  target yet) is created with mode `0600` whatever the umask allows. The replace is atomic, but
  the directory itself is not fsynced. **The file is replaced, not rewritten in place: a hard
  link to the old inode keeps the old bytes.** `fmt --write` rewrites only when the text changes.
- **`fmt --write` reads the file it rewrites through the same handle**: its entry is checked
  without following a link (a link, a directory or another non-file swapped in after the first
  check is the same `output-not-a-file`), opened with `O_NOFOLLOW | O_NONBLOCK`, and must be the
  entry checked; the new file takes the mode of the file read, from its handle, and is renamed
  over the entry only if the entry still has the device and inode that were read (`write-failed`
  "the file changed after it was read; nothing is written" otherwise). That check comes just
  before the rename, and a file swapped in between the two is replaced, never written through.
  Under contention that window is hit often: the review of GitHub #777 exchanged the file flat
  out with another regular file in the same directory and destroyed that file's content in 402
  of 3,000 runs, each exiting 0 "rewritten" (756 before issue #771); with a link swapped in, the
  link's entry was replaced in 195 runs and the file outside never written. Issue #780 closes it
  with an exchange-based compare-and-swap.
- **Breaking in place** (issue #771): a directory a write goes into, or a document's directory,
  that can be searched but not listed (mode `--x` or `-wx`) is refused (`write-failed` for a
  write, `read-failed` for a read) with "the directory `D` cannot be listed (permission denied)",
  `D` as the caller's path gives it, where a path-based call used to pass through it: the handles
  are opened read-only, since rustix offers no `O_SEARCH` on macOS (nor `O_PATH` outside Linux),
  on every platform alike (the orchestrator's call in the review of GitHub #777). For the same
  reason a `--commission --out` run under a umask that leaves the directories it makes unreadable
  (`umask 0477`) is refused: "`DIR/outputs`: it was made but could not be opened (permission
  denied), and is removed". A descriptor limit met there reads "too many open files", never
  "uncategorized error". And a `--commission --out DIR` just under the platform's path limit
  now has its curves written (each name is relative to its directory's handle), where until then
  the longer names under it were refused (`write-failed`, ENAMETOOLONG).
- **The output is never an input.** `emit -o` and `import -o` refuse an output that is their own
  input file before anything is read, and both forms of `verify --out` a `DIR/receipt.json` that
  is a file the run reads, after the reads and before the run (`output-is-input`; [above](#oce-import)
  says what one file is, and that on Unix it includes the file stdin was opened from), so no run
  replaces what it read.
- `emit -o` also refuses a name that reads as text or wire (`output-form-mismatch`): the CXF it
  writes goes to `.jsonld`, `.cxf.json` or a name with no known extension, never to `.oce` or
  `.json`, which `oce validate`, `fmt` and `id` would read as another form.

On Windows a symbolic link or a junction at the target is refused by the same check (std reports
both as links), and a hard link is not recognised as an input; neither is tested there. Windows
has no directory handles here (std has no `openat`): every step of a write, the temporary file,
the rename and the clean-up, reaches the directory by path, so a directory swapped during a write
is followed there, a failed write's temporary file can be left in a directory swapped meanwhile,
and `fmt --write` neither ties its read to the entry nor checks the entry before the rename.

### Reading a file a document names

A vector set's mutants and a commissioning spec's CXF, mapping and trend files are named by
relative paths and must stay inside the document's directory.

**The document itself** (issue #771; Unix): its directory, as the document's path gives it
(`dir/spec.json` gives `dir`), is opened **once**, following the links on that path (the caller
named it: `bundle -> real`, `oce verify --commission bundle/spec.json`, reads `real`), then
canonicalized, and the handle must have the device and inode of the canonical directory. The
document is read through that handle: its entry, following a link there (the caller named it: a
spec that is a link reads the files beside the link, not beside its target; the orchestrator's
call on issue #771), must be a regular file (`not-a-file`; a named pipe is never opened), is
opened with `O_NONBLOCK`, must be the entry checked, and its size is judged from that handle.
Every file it names is resolved against that same handle, never against the directory's path
again, so a folder exchanged during the run cannot pair the document with another folder's files.

**A file it names**: its path is joined onto the document's directory and canonicalized,
following links, so a link that stays inside is honoured and one that leaves is refused
(`mutant-outside-directory`, `file-outside-directory`). On Unix the canonical path is then opened
**component by component from the document's directory handle without following a link** (issue
#758, ruled by the owner on 2026-11-22): each directory below it with `openat` from its parent's
handle and `O_DIRECTORY | O_NOFOLLOW`; the last entry's own metadata, read from its directory's
handle without following a link, must be a regular file (`not-a-file`), and it is opened with
`O_NOFOLLOW | O_NONBLOCK` and must be the entry checked, and the handle the caller reads is then
cleared of `O_NONBLOCK`. A link met on the way, or a directory that is no longer one (a named
pipe swapped in for a directory is refused by `O_DIRECTORY` before it is opened), was swapped in
after the canonicalization and is `read-failed`. The opens use the workspace's `rustix`, a
third-party dependency beside `clap` and `serde_json` (ruled by the owner on 2026-11-22); the
crate stays `forbid(unsafe_code)`.

**The one call left that reads the directory by path** is the canonicalization: it only decides
which segments the walk opens, and the walk opens them from the pinned handle with no link
followed. So a folder swapped in at the directory's path after the pin can change **which file
inside the pinned directory** is read (its links steer the canonicalization), and can never reach
a file outside it (`src/commands/contained_tests.rs` tries every schedule of that swap).

Each handle pins what it names, so a rename in the directory cannot redirect a later step. Issue
#758's harness swapped an intermediate directory for a link to a directory outside, atomically
and as fast as it could, against 1,000,000 reads, and tried every schedule of swaps between two of
the reader's calls (an adversary that acts between any two of them):

| Reader | Adversary schedules that read outside | Race, macOS arm64 | Race, Linux aarch64 |
| --- | --- | --- | --- |
| GitHub #757's path-based checks | 1 of 8 (one swap before the entry's check) | 135 in 1,000,000 | 168,695 in 1,000,000 |
| the same, re-checked against a second canonicalization after the open (std only) | 1 of 32 (three swaps) | 0 in 1,000,000 | 410,340 in 1,000,000 |
| this `openat` walk | 0 of 16 | 0 in 1,000,000 | 0 in 1,000,000 |

Through the built binary on macOS, 3,000 `oce verify --commission` runs against the same swap
read outside 141 times with GitHub #757's checks and never with these opens.

### Directory handles: the races they close

Issue #771's review found three places that reached a directory or a file by path after their
checks; each was reproduced as a harness against the built binary (a debug build, run from a
scratch directory) before and after the change, on macOS (APFS) and on Linux (the
`rust:1.97.1-alpine` image on aarch64, tmpfs, no network):

| Race (the reviewer's reproduction as a harness) | Runs | macOS arm64, before | after | Linux aarch64, before | after |
| --- | --- | --- | --- | --- | --- |
| (i) `verify --commission spec.json --out out --bounds`: `out/outputs` exchanged for a link to a victim directory once the first curve's temporary file exists | 200 | 200 sent the curves outside `DIR`, exit 0 | 0 (all 200 swapped) | 198 (2 runs finished before the swap) | 0 (196 swapped) |
| (ii) `verify --commission cur/spec.json`: `cur` exchanged flat out with another folder | 2,000 | 676 paired the spec with the other folder's CXF | 0 | 455 | 0 |
| (ii) `verify cur/vectors.json --subject rule.cxf.jsonld`: the same, for a vector set and its mutant | 2,000 | 97 | 0 | 427 | 0 |
| (iii) `fmt --write t.oce`: `t.oce` exchanged flat out with a link to a file outside its directory | 3,000 | 117 wrote the outside file's canonical text into the directory | 0 | 362 | 0 |

A pairing is counted only when it is proved: each folder holds the other folder's file names as
over-size decoys, so a document read with the other folder's file answers `input-too-large`. The
runs that are not counted answer a verdict or a refusal for the swap they met (`read-failed`,
`not-found`, the outside code), never a pairing.

The deterministic proofs are the adversaries in `src/commands/contained_tests.rs`,
`atomic_write_tests.rs`, `commission_out_tests.rs` and `fmt_tests.rs`: each tries every schedule
of the swap between the code's own file-system calls, and a copy of the path-based code that came
before runs under the same adversary as the control, which some schedule must beat.
`tests/directory_handle_races.rs` and `tests/read_containment_race.rs` race the binary. All of
them need an atomic exchange (Linux, macOS). On a file system that refuses it they **fail**,
unless `OCE_TESTS_ALLOW_NO_EXCHANGE=1` is set, when they skip with a note and the rules are untested
there: a missing exchange never passes for a race that ran. CI's Linux runners (ext4 under the
workspace) and macOS (APFS) support it.

**Windows** has less, and none of it is tested there: a document is read by path and its
directory canonicalized by path; each named file's canonical path's own entry must be a regular
file, it is opened by path, and the handle must be a regular file. Nothing ties the handle to the
entry checked, since std has no stable file index there (a file's identity is its canonical path),
so a link swapped in for an intermediate directory or the last entry after the check is followed
(std has no `openat`), and a folder exchanged during a run can pair a document with another
folder's files. A hard link inside the directory to a file outside it is read on every platform:
it has no other path.

## Tests and goldens

`tests/runs.rs` runs the built binary (`CARGO_BIN_EXE_oce`) for every subcommand and outcome
class and pins each run's exit status, stdout and stderr as one transcript in
`tests/fixtures/runs/*.txt` (`--help` of every command included, so flag drift is visible);
`tests/report_shape.rs` holds every report to the contract above; `tests/vocabulary.rs` holds
the block vocabulary over every class (the closed class shape, the vocabulary's own text and the
catalog's ranges, no `evidence` and no markup anywhere, the same bytes on a second run) and the
scale-factor table; `tests/catalog_text.rs` writes the text listing it expects from each
class's JSON report and requires the binary's text to be exactly that, for the 133 classes that
resolve at a table of plain values (every port's and parameter's `unit:` line included); `tests/files.rs` covers `fmt`
over the `oce-author` text fixtures (referenced in place), `--write`, `-o` and a warning-free
engine load of every emitted bundle; `tests/atomic_writes.rs` the write hardening (a planted link
or file at the temporary name, permissions, a link or directory target refused, a failed write's
report); `tests/output_targets.rs` every verb that writes against every kind of target (absent,
a regular file, a link inside, outside, dangling and to an input, a hard link to an input, an
empty and a full directory, a named pipe, a linked parent directory) with every entry unchanged
after a refusal, and the vector `--out`'s `output-is-input` against the set, the subject and a
mutant; `tests/read_containment_race.rs` the containment rule raced through the binary;
`tests/directory_handle_races.rs` issue #771's three races (the curve files, a document's folder,
`fmt --write`'s file) and a named pipe swapped in, each through the binary;
`tests/trailing_slashes.rs` a document, `fmt --write`'s file and an output named with a trailing
`/`; `tests/commission_out_limits.rs` `--commission --out` under a umask and a descriptor limit;
`tests/hostile_inputs.rs` the bounds. For `verify`:
- `tests/verify_runs.rs` pins one transcript per verdict and refusal class, with each exit-only
  flag flipping the status over the same receipt bytes;
- `tests/verify_files.rs` covers `--out`, `--timings` (no time is asserted), the subject forms and
  the size bounds from metadata;
- `tests/verify_mutant_paths.rs` covers mutant path resolution: links, hard links, case and
  normalization twins, trailing dots and spaces, drive prefixes and `..`, each with a test or the
  platform's stated reason;
- `tests/replay_gate.rs` is the in-repo replay gate.

For `verify --commission` (every run in a scratch directory holding `oce-verify`'s commissioning
run fixture, read in place, `tests/support/commission.rs`):
- `tests/commission_runs.rs` pins one transcript per verdict (`pass`, `fail`, `no_data`, each
  stdout byte-equal to `oce-verify`'s golden receipt) and per refusal class (usage, the spec, the
  reference, a file, each library refusal with its cause, `--out`), the rendering, and holds every
  code and cause they print to `oce vocab`;
- `tests/commission_files.rs` covers `--out`: every `errors.csv` against the library's golden
  columns and, for the oracle case, against `derive_run.py`'s independent derivation (`nan` at an
  uncovered instant), `inf` for a non-finite engine sample, the bound files against the library's
  curves and a golden (`tests/fixtures/commission/`), the report's `curves`, two runs giving the
  same files, the flags, nothing written on a refusal, a part-way failure, links at
  `receipt.json`, hostile names, and the streams;
- `tests/commission_out_guards.rs` covers what `--out` refuses to write over (a stale
  `receipt.json`; a link there to a file outside `DIR`, to an unread file, dangling; a dangling
  link at `outputs`; a `DIR` that is a file), a curve that cannot be written (a `DIR` path swept
  across the platform's path limit, so writes fail at every stage, each run's `curves` holding
  exactly the files that exist; the long directories are removed when the sweep ends, an
  assertion failing included, since a path past the limit left under `target/` keeps `git
  worktree remove` from removing a worktree), and `--deny-warnings` on a load warning;
- `tests/commission_reads.rs` covers the reads: each bound from metadata at and one past its
  edge, the files together, links out of and inside the spec's directory, a directory and a named
  pipe;
- `tests/hostile_inputs.rs` runs seeded byte edits of the spec, the mapping and a trend file
  through the binary.

For `id`:
- `tests/id_runs.rs` pins one transcript per input form and refusal class, `--json` and `--help`
  included;
- `tests/id_files.rs` holds every printed id to `Engine::content_id`: the 47 documents of the
  tracked pin file (`crates/oce-api/tests/fixtures/content_id/pin.tsv`, read in place), both
  in-repo library rules, and each `oce-author` text fixture against its committed and its
  emitted CXF. It also covers stdin against a file, the name rule, the namespace, both size
  bounds, hostile CXF, and the agreement of `oce emit` and `oce id` over every tracked authored
  fixture, with their counts pinned.

For `import`:
- `tests/import_runs.rs` pins one transcript per outcome class: the text and the wire form of
  two in-repo fixtures (a G36 corpus fixture and a library rule), the report under `-o` with
  both lists, stdin, and one refusal per construct outside the authoring subset (a nested
  composite, a conditional component, an expression value, a block-port attribute,
  `hasInstance`, `hasConstant`, an array connector, an array sequence parameter), each over a
  tracked fixture read in place;
- `tests/import_corpus.rs` is the round trip over every CXF fixture tracked under `crates/`
  ([above](#the-round-trip)); its table, `tests/fixtures/import_corpus.tsv`, regenerates with
  the transcripts. It asserts one split by hand, the G36 catalog's, and builds the two shapes
  of issue #683 that no tracked fixture writes: an Integer element of a Real array, which
  keeps its id, and an Integer above 2^53 on a Real parameter, which has none;
- `tests/import_refusals.rs` earns each of the twenty-seven importer codes a CXF document can
  earn, from a file and from stdin and in both output forms, names the four no run earns, and
  holds hostile bytes and the escaped rendering;
- `tests/output_guards.rs` holds the two `-o` guards, for `import` and for `emit`: the input
  under another spelling of its path, through a symbolic link and (Unix) a hard link, with
  nothing written or left behind; every contradicting name; and each file `import` writes read
  back by name through `validate`, `fmt --check`, `emit` and `id`; for `emit`, every accepted
  and refused `-o` name (nothing written when refused, each CXF name read back by `id`), and a
  file opened as stdin (Unix) under every name of it;
- `tests/rendering.rs` is the regression test of the escaped rendering over `validate`, `emit`
  and `diff` of a wire file whose class carries format and separator characters;
- `tests/import_files.rs` covers `-o`, the two output forms against each other and against
  `oce_author::import_cxf`, the read-back refusals, the size bound, a Real's bits through both
  forms, and the Integer-literal case of the round trip.

For `diff`:
- `tests/diff_runs.rs` pins one transcript per outcome class: an equal pair (text, and CXF against
  a reordering of itself), the JSON envelope and the Markdown table of each of the note's three
  worked examples, the envelope of each as CXF (emitted into a scratch directory, so no CXF is
  checked in), the stderr rendering, a `reordered`-only pair, `--no-renames`, an invalid draft,
  stdin on one side, and every refusal: a mixed pair, stdin twice, an unparsable base and head,
  a CXF side the engine refuses, a document the engine exported (`outside-root`), stdin without
  `--from`, a name with no form, a missing head, a refusal of each side rendered at that side's
  name, and two usage errors;
- `tests/diff_files.rs` is the oracle: the diff the report carries is `oce-diff`'s golden byte
  for byte once the envelope is removed, the Markdown is its table golden and the stderr its
  text golden, in the text form for every worked example and `oce-diff`'s `every_kind` pair, and
  in the CXF form for every worked example (`every_kind` is not emitted); the wire form's diff is
  the library's. It also holds `.cxf.json` to CXF, exit `0` to `equal`, `--no-renames` to its
  note, an invalid draft to the library's diff, two runs and stdin to the same bytes, each side's
  bound (at the bound read, one byte over refused about that side), the base read, parsed,
  loaded and compared before the head (both sides refused alike name the base; one side refused,
  by a read, a parse, a load or `outside-root`, names that side), and every code a run reports to
  `oce vocab`;
- `tests/hostile_inputs.rs` refuses junk, bad UTF-8, an empty file, a directory and a missing
  path on either side, in each form and through stdin, about that side; `tests/rendering.rs`
  escapes a wire class's format and separator characters in the diff's text on stderr, which
  `render_text` leaves as they are; `tests/streams.rs` renders the report, not the diff, when
  stdout cannot be written.

The transcripts regenerate only with `OCE_BLESS_CLI=1 cargo nextest run -p oce-cli` (see
`.agents/project-facts.md`). The `verify` transcripts embed receipts, which carry the engine
version, so a workspace version bump re-blesses them; the `verify_commission_*` transcripts read
`oce-verify`'s commissioning run fixture, so anything that moves one of its golden receipts moves
them too.
