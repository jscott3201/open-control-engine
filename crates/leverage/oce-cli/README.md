# oce-cli

The `oce` command-line tool (M2; design note [`docs/design/authoring-layer.md`](../../../docs/design/authoring-layer.md),
"CLI", decisions 9 and 20; issue #489): `emit`, `validate`, `fmt`, `catalog` and `vocab` over
[`oce-author`](../oce-author/README.md) and the facade. A leverage crate and the first binary above
the facade (the engine crates ship none); it depends on `oce-api` and `oce-author` only, opens no
listener, and every engine it loads is a throwaway of the validator's own. Package `oce-cli`,
binary `oce`, `publish = false`. Arguments are parsed with `clap` (derive, no colour).

```text
oce emit <FILE> [--namespace <NS>] [-o <OUT>]    validate both stages, then flat CXF
oce validate <FILE> [--author-side]              findings as a JSON report
oce fmt <FILE> [--check | --write]               canonical text (wire input becomes text)
oce catalog [<CLASS>] [--json] [--param N=V ...] classes, parameters, resolved ports
oce vocab [--json]                               keywords, tags, units, enumerations, codes
```

`FILE` is a `.oce` (text form) or `.json` (wire form) file, or `-` for stdin; `--from text|wire`
overrides the extension and is **required** for stdin. Every command takes `--render
auto|always|never` (default `auto`: a human-readable rendering of the report on stderr only when
**stderr** is a terminal, whatever stdout is; stdout is the same bytes whatever `--render` says),
`--help`, and `--version` (the workspace version). `--json` exists only on `catalog` and `vocab`,
the two commands whose default stdout is text; every other command's report is already JSON. A
failing `catalog` (an unknown class, a refused table) prints the JSON report on stdout even
without `--json`.

```sh
oce validate econ_enable.oce                 # report on stdout, exit 0 / 1 / 2
oce emit econ_enable.oce -o econ_enable.json # writes the CXF atomically, report on stdout
oce fmt --check econ_enable.oce              # exit 1 when not canonical
oce fmt econ_enable.json > econ_enable.oce   # wire to text
oce catalog CDL.Reals.MultiSum --param nin=3 # ports u1, u2, u3
```

## Exit codes

| Exit | Meaning | When |
| --- | --- | --- |
| `0` | yes | `validate` and `emit`: no error finding; `fmt --check`: canonical; `catalog`: the ports resolve; `fmt`, `fmt --write`, `vocab`, `--help`, `--version` |
| `1` | no | an error finding (`validate`, `emit`, which then writes nothing); `fmt --check`: not canonical; `catalog`: an unknown class, or the class rules refuse the parameter table (including a required parameter with no default) |
| `2` | error | the question could not be answered: a usage error (clap), stdin without `--from`, an unknown extension, a missing path or a directory, an unreadable or over-size input, a parse refusal, a malformed `--namespace` or `--param`, `--write` on stdin or a wire file, a failed write, or a stdout that cannot be written |

## Output

Stdout carries **exactly one document** per run:

- the command's product when it succeeds and has one — the CXF (`emit` without `-o`), the
  canonical text (`fmt`), a text listing (`catalog`, `vocab` without `--json`);
- otherwise the JSON report below (every exit `1` and `2`, `emit -o`, `fmt --check`, `fmt --write`,
  `catalog --json`, `vocab --json`);
- nothing for a clap usage error (its message is on stderr) and clap's text for `--help` and
  `--version`.

Everything is deterministic: no clock, no locale, no terminal width, LF line endings, one final
line feed, and the program is always named `oce` whatever path started it. No path appears that
was not given as an argument: `input` and `output` are echoed exactly as given (`-` for stdin), and
I/O failures are reported by kind, never with an operating-system message that could carry one.

## The JSON report (`schema: "oce-report/1"`)

One JSON object, keys sorted at every depth, two-space indentation. The common keys are always
present:

| Key | Type | Meaning |
| --- | --- | --- |
| `tool` | `"oce"` | |
| `schema` | `"oce-report/1"` | the report contract's version; a breaking change bumps it |
| `command` | string | `emit`, `validate`, `fmt`, `catalog` or `vocab` |
| `input` | string or `null` | the input path as given, `-` for stdin; `null` for `catalog` and `vocab` |
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
with the refusal's span), a wire-form code (with the refusal's span where it has one), or one of
the tool's own codes (`span` `null`). `oce vocab` lists every code with one line of meaning.

Command keys:

| Command | Keys |
| --- | --- |
| `validate` | `stages`: the stages that ran, `["author"]` or `["author", "engine"]` (`[]` after a read or parse refusal); stage 2 runs only when stage 1 found no error and `--author-side` is absent |
| `emit` | `stages` as for `validate`; with `-o`, only once the write succeeded, `output` (the path as given) and `bytes` (absent from a failed write's report) |
| `fmt` | `form` (`text`, `wire`, or `null` before the form is known); with `--check`, `canonical` (whether the input is already canonical **in its own form**: text against `fmt`, wire against the canonical wire writer); with `--write`, only once the file is known canonical on disk (unchanged, or rewritten), `changed` and `output` (absent from a failed write's report) |
| `catalog` | without a class, `classes`; with one, `class` (`null` when the class is unknown or a `--param` is malformed) and `params` (the `--param` strings as given). A class is `{ "class_path", "description", "reserved", "parameters", "inputs", "outputs" }`; a parameter `{ "name", "type", "default": { "kind", "value" }, "settable", "enumeration" }`, `kind` one of `real`, `integer`, `boolean`, `string`, `member`, `derived` (value: the formula) or `required` (value `null`); a port `{ "name", "kind" }`; `inputs`/`outputs` are `null` where the ports do not resolve (a required parameter at the default table) |
| `vocab` | `keywords`, `types`, `tags`, `base_units`, `parse_codes`, `finding_codes` (each with `stage`), `wire_codes`, `error_codes`, `exit_codes` (each `{ "code", "meaning" }`), `enumerations` (below) |

The tool's own error codes: `missing-from`, `unknown-form`, `not-found`, `not-a-file`,
`read-failed`, `input-too-large`, `invalid-namespace`, `unspellable`, `write-failed`,
`write-needs-file`, `write-needs-text`, `malformed-param`, `unknown-param`, `duplicate-param`,
`malformed-value`, `ports-unresolvable`, `emit-refused`. The wire-form refusal codes (one per
`oce_author::WireError`: `not-utf8`, `byte-order-mark`, `nesting-too-deep`, `list-too-long`,
`string-too-long`, `duplicate-key`, `non-finite-number`, `integer-out-of-range`,
`control-character`, `unknown-key`, `missing-key`, `wrong-type`, `json-syntax`,
`malformed-connection`, `wire-refused`) are listed with their meanings by `oce vocab --json` under
`wire_codes`.

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

Every read is bounded by its form's limit (`oce_author::MAX_TEXT_BYTES`, `MAX_WIRE_BYTES`, 4 MiB):
a file's size is checked from its metadata before a byte is read (a 100 MB file costs one `stat`),
and every read, stdin included, stops one byte past the limit. Stdin is bounded in size, not in
time: a pipe that never closes keeps `oce` waiting. Non-UTF-8, binary junk, an empty file, a
directory and a missing path each get one typed error code. Nothing panics.

`fmt --write` and `emit -o` write a uniquely named temporary file beside the target
(`.NAME.<pid>.<counter>.oce-tmp`), opened with `create_new` so it never follows a link or
truncates a file already there, sync it, give it the target's permissions (Unix mode bits) and
rename it over the target; on any failure the temporary file is removed and the target is
untouched. A target that is a symbolic link is followed: the file it names is replaced and the
link stays a link (a dangling link is `write-failed`). **The file is replaced, not rewritten in
place: a hard link to the old inode keeps the old bytes.** `fmt --write` rewrites only when the
text changes.

## Tests and goldens

`tests/runs.rs` runs the built binary (`CARGO_BIN_EXE_oce`) for every subcommand and outcome
class and pins each run's exit status, stdout and stderr as one transcript in
`tests/fixtures/runs/*.txt` (`--help` of every command included, so flag drift is visible);
`tests/report_shape.rs` holds every report to the contract above; `tests/files.rs` covers `fmt`
over the `oce-author` text fixtures (referenced in place), `--write`, `-o` and a warning-free
engine load of every emitted bundle; `tests/atomic_writes.rs` the write hardening (a planted link
or file at the temporary name, permissions, a link target, a failed write's report);
`tests/hostile_inputs.rs` the bounds. The transcripts
regenerate only with `OCE_BLESS_CLI=1 cargo nextest run -p oce-cli` (see
`.agents/project-facts.md`).
