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
oce vocab [--json]                               keywords, tags, base units, every code
```

`FILE` is a `.oce` (text form) or `.json` (wire form) file, or `-` for stdin; `--from text|wire`
overrides the extension and is **required** for stdin. Every command takes `--render
auto|always|never` (default `auto`: a human-readable rendering of the report on stderr only when
stderr is a terminal), `--help`, and `--version` (the workspace version).

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
| `emit` | `stages` as for `validate`; with `-o` on success, `output` (the path as given) and `bytes` |
| `fmt` | `form` (`text`, `wire`, or `null` before the form is known); with `--check`, `canonical` (whether the input is already canonical **in its own form**: text against `fmt`, wire against the canonical wire writer); with `--write`, `changed` and `output` |
| `catalog` | without a class, `classes`; with one, `class` (`null` when the class is unknown or a `--param` is malformed) and `params` (the `--param` strings as given). A class is `{ "class_path", "description", "reserved", "parameters", "inputs", "outputs" }`; a parameter `{ "name", "type", "default": { "kind", "value" }, "settable", "enumeration" }`, `kind` one of `real`, `integer`, `boolean`, `string`, `member`, `derived` (value: the formula) or `required` (value `null`); a port `{ "name", "kind" }`; `inputs`/`outputs` are `null` where the ports do not resolve (a required parameter at the default table) |
| `vocab` | `keywords`, `types`, `tags`, `base_units`, `parse_codes`, `finding_codes` (each with `stage`), `wire_codes`, `error_codes`, `exit_codes` (each `{ "code", "meaning" }`), `enumerations` |

The tool's own error codes: `missing-from`, `unknown-form`, `not-found`, `not-a-file`,
`read-failed`, `input-too-large`, `invalid-namespace`, `unspellable`, `write-failed`,
`write-needs-file`, `write-needs-text`, `malformed-param`, `unknown-param`, `duplicate-param`,
`malformed-value`, `ports-unresolvable`, `emit-refused`.

### Enumerations (hook)

The facade's enumeration and constant-package table (issue #468, GitHub #488) is not on
`development` yet. Until it is: `vocab`'s `enumerations` is `null`, a catalog parameter's
`enumeration` is `null` (its `type` is `Enum`), and an enumeration `--param` value is the member's
1-based ordinal. When the table lands, `enumerations` lists each row's class path, kind and members,
`enumeration` names the parameter's type, and `--param` takes the member name. Each is one site in
`src/commands/{vocab,catalog}.rs`, and each moves the goldens.

## Bounds and hostile input

Every read is bounded by its form's limit (`oce_author::MAX_TEXT_BYTES`, `MAX_WIRE_BYTES`, 4 MiB):
a file's size is checked from its metadata before a byte is read (a 100 MB file costs one `stat`),
and every read, stdin included, stops one byte past the limit. Non-UTF-8, binary junk, an empty
file, a directory and a missing path each get one typed error code. `fmt --write` and `emit -o`
write a temporary file beside the target, sync it and rename it over the target, so a failure
leaves the target untouched; `fmt --write` rewrites only when the text changes. Nothing panics.

## Tests and goldens

`tests/runs.rs` runs the built binary (`CARGO_BIN_EXE_oce`) for every subcommand and outcome
class and pins each run's exit status, stdout and stderr as one transcript in
`tests/fixtures/runs/*.txt` (`--help` of every command included, so flag drift is visible);
`tests/report_shape.rs` holds every report to the contract above; `tests/files.rs` covers `fmt`
over the `oce-author` text fixtures (referenced in place), `--write`, `-o` and a warning-free
engine load of every emitted bundle; `tests/hostile_inputs.rs` the bounds. The transcripts
regenerate only with `OCE_BLESS_CLI=1 cargo nextest run -p oce-cli` (see
`.agents/project-facts.md`).
