---
title: Data templates — agent supplies JSON, a template renders it
slug: /specifications/data-templates
type: spec
status: approved
owner: maintainers
created: 2026-10-25
updated: 2026-10-25
sidebar_position: 77
tags: [ai-native, cli, mcp, templates, json-schema]
spec_id: data-templates
issue_id: CORE-576
applies_to:
  - engine/src/template.rs
  - engine/src/main.rs
  - engine/src/capabilities.rs
  - engine/tests/it/core576_data_templates.rs
  - templates/
dependencies:
  - cli-surface
  - diagnostics
  - mcp-server
  - mcp-tool-contract
  - untrusted-input-safety
---

# Data templates: agent supplies JSON, a template renders it

## Overview

Agents produce structured data well and fine layout badly. Data template
mode splits the two jobs. A template owns the layout: HTML and CSS with
placeholders and loops. The agent supplies a JSON data file. A JSON Schema
next to the template describes the data. The engine validates the data,
expands the template, and renders the result as an ordinary HTML document.

```text
typeanvil render invoice.tmpl.html --data invoice.json -o invoice.pdf
```

## Decisions

DECIDED 2026-10-25 (agent), recorded on CORE-576:

1. **Template language: MiniJinja 3** (Jinja2 syntax). It is pure Rust,
   widely known by models, and deterministic. TypeAnvil does not invent a
   template language. The crate builds with default features off, plus
   `builtins` (filters such as `upper`, `length`, `join`) and `fuel`.
2. **Sandbox.** The engine registers no loader, so `include`, `import` and
   `extends` fail. Templates have no filesystem, network, clock or random
   source. The `fuel` budget bounds runaway loops.
3. **Escaping.** HTML auto-escape is always on, whatever the template file
   name is. A template opts out per value with the `safe` filter only.
4. **Validation.** The engine reuses its own JSON Schema subset validator
   (the one that checks the MCP contract) and reports every violation, each
   with a JSON pointer. No new schema dependency.

## Goals

1. Render a template plus JSON data with one CLI flag.
2. Validate data before render. Report schema errors with stable codes and
   a JSON pointer to the bad field, and write no PDF.
3. Byte-identical PDFs for identical template and data.
4. Let an agent read a template's schema before it writes data.
5. Ship an invoice and a report template as examples.

## Non-Goals

- No template inheritance or partials (`extends`, `include`, `import`).
  One template is one file.
- No full JSON Schema 2020-12. The supported keyword set is listed below.
  `$ref` and remote schemas are not resolved.
- No MCP surface in this slice. It landed in CORE-588 as contract 1.1: the
  `templates` tool and the `data`/`schema` inputs of `render` and
  `validate` (`mcp-tool-contract.spec.md` § Data templates). The CLI
  equivalent is `typeanvil template-schema`.
- No Markdown templates. `--data` applies to HTML input only.

## Behavior

### Template expansion

1. `render` shall accept `--data <file.json>`. With the flag, the input file
   is a MiniJinja template. Without it, behavior is unchanged.
2. The data file shall be parsed as JSON. Invalid JSON shall stop the render
   with code `TEMPLATE_DATA_INVALID_JSON`.
3. The JSON value shall be the template context. A top-level object exposes
   its keys as variables. Any other top-level value is exposed as `data`.
   The whole value is always available as `data` too.
4. Auto-escape shall be HTML for every template.
5. Undefined variables shall render as empty (MiniJinja lenient default).
   The schema is the place to require fields.
6. A template syntax or evaluation error shall stop the render with code
   `TEMPLATE_ERROR`, the message, and the template line when known.
7. `include`, `import` and `extends` shall fail with `TEMPLATE_ERROR`.
8. The engine shall set a fuel limit of 10,000,000 instructions. Exceeding
   it shall stop the render with `TEMPLATE_ERROR`.
9. The expanded HTML shall render through the same pipeline as an HTML
   input at the template's path, so relative URLs resolve as before and the
   fetch policy and limits (`untrusted-input-safety.spec.md`) apply.

### Schema

10. The schema shall be `--schema <file>` when given. Otherwise it is the
    sibling file named by replacing `.tmpl.html` (or the last extension)
    with `.schema.json`, when that file exists. No schema means no
    validation.
11. A schema file that is not valid JSON shall stop the render with
    `TEMPLATE_SCHEMA_INVALID`.
12. Data shall be validated before expansion. Each violation is a
    diagnostic with code `TEMPLATE_DATA_SCHEMA`, a JSON `pointer` (RFC 6901;
    `/` for the root), and a message. The engine shall report every
    violation it finds, in document order, at most one per pointer.
13. Supported keywords: `type`, `enum`, `const`, `properties`, `required`,
    `additionalProperties`, `items`, `minItems`, `maxItems`, `minimum`,
    `maximum`, `minLength`, `maxLength`, `uniqueItems`, `oneOf`, `if`/`then`.
    Annotations (`title`, `description`, `format`, `default`, `examples`,
    `$schema`, `$id`) are ignored. `pattern` is ignored.

### Errors and exit codes

14. Any template error (codes above) shall write no PDF and exit 5.
15. stderr shall carry one line per error:
    `typeanvil: error: [<CODE>] <pointer or line>: <message>`.
16. With `--diagnostics json`, stdout shall carry
    `{"schema": 1, "template_errors": [{"code", "pointer", "line", "message"}, ...]}`
    where `pointer` is `null` for non-schema errors and `line` is `null`
    when unknown.

### Publishing schemas

17. `typeanvil template-schema <template>` shall print the resolved schema
    (rule 10) as JSON, or exit 1 with `no schema for <template>`.
18. `capabilities --json` shall list `template` among `input_formats`.
19. The `template` input-format entry shall name `template-schema` as the
    way to request a template's schema.

### Examples

20. The repository shall ship `templates/invoice/` and `templates/report/`,
    each with `<name>.tmpl.html`, `<name>.schema.json` and
    `<name>.example.json`. Each example shall validate against its schema
    and render.

## Interfaces

```rust
// engine/src/template.rs
pub struct TemplateError { pub code: &'static str, pub pointer: Option<String>,
                           pub line: Option<usize>, pub message: String }
pub struct TemplateErrors(pub Vec<TemplateError>);   // std::error::Error
pub fn schema_path_for(template: &Path) -> PathBuf;
pub fn load_schema(template: &Path, explicit: Option<&Path>) -> Result<Option<Value>, TemplateErrors>;
pub fn validate_data(schema: &Value, data: &Value) -> Vec<TemplateError>;
pub fn expand(source: &str, data: &Value) -> Result<String, TemplateErrors>;
pub fn errors_json(errors: &[TemplateError]) -> String;
```

## Acceptance Criteria

| # | Given / When / Then | Test |
|---|---|---|
| AC1 | Given the invoice example, when rendered twice, then the PDFs are byte-identical and non-empty. | `core576_data_templates::valid_data_renders_byte_identically` |
| AC2 | Given data that breaks the schema, when rendered, then exit 5, no PDF, and `TEMPLATE_DATA_SCHEMA` errors with pointers to each bad field. | `invalid_data_yields_schema_diagnostics_no_pdf` |
| AC3 | Given `<script>` in a string field, then the expanded HTML holds it escaped. | `values_are_html_escaped` |
| AC4 | Given a template with `{% include %}`, then `TEMPLATE_ERROR`, no PDF. | `include_is_sandboxed` |
| AC5 | Given malformed JSON data, then `TEMPLATE_DATA_INVALID_JSON`. | `invalid_json_data` |
| AC6 | `template-schema` prints the sibling schema; `capabilities --json` lists the `template` input format. | `template_schema_is_published` |
| AC7 | Both shipped examples validate and render. | `shipped_examples_render` |

## Edge Cases

- A schema violation inside `oneOf` reports one error at the `oneOf`
  location, because no single branch is the "right" one.
- An unbounded `{% for %}` over `range(1e9)` stops on fuel, not on time.

## References

- CORE-576 (this spec), epic CORE-570.
- `mcp-tool-contract.spec.md` — the schema validator this reuses.
- `untrusted-input-safety.spec.md` — fetch policy and limits.
- MiniJinja: <https://docs.rs/minijinja>.
