# Template library

Data templates (CORE-576, `docs/specifications/data-templates.spec.md`).
Each directory holds one template:

| File | What it is |
|---|---|
| `<name>.tmpl.html` | MiniJinja (Jinja2 syntax) HTML + CSS template. Values are HTML-escaped by default. |
| `<name>.schema.json` | JSON Schema for the data. `render` validates the data before it expands the template. |
| `<name>.example.json` | Valid example data. |

```sh
typeanvil template-schema templates/invoice/invoice.tmpl.html      # what data to send
typeanvil render templates/invoice/invoice.tmpl.html \
  --data templates/invoice/invoice.example.json \
  --page-width 8.5in --page-height 11in -o invoice.pdf
```

Invalid data writes no PDF and exits 5. Each error has a stable code and a
JSON pointer, for example
`typeanvil: error: [TEMPLATE_DATA_SCHEMA] /items/0/quantity: expected type "number", got string`.
Add `--diagnostics json` to get the errors as JSON on stdout.

Templates have no file or network access: `include`, `import` and
`extends` fail. The extra filter `fixed(n)` formats a number with `n`
decimals.
