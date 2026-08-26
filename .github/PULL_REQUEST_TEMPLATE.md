# Pull Request

By opening this pull request you agree to the terms in [CLA.md](../blob/main/CLA.md):
you keep ownership of your contribution and grant the project owner a license
to distribute it under any license, including commercial ones.

Each commit must carry a sign-off line:

```
Signed-off-by: Your Name <your.email@example.com>
```

(`git commit -s` adds it automatically.)

## Summary

<!-- What does this PR change? -->

## Related issues

<!-- e.g. CORE-123 -->

## Checklist

- [ ] This PR adheres to the [development guidelines](../blob/main/docs/conventions/development-guidelines.md) (spec-driven development, docs sync, model attribution, verification)
- [ ] Commits are signed off (`git commit -s`)
- [ ] Behavior changes update their spec in `docs/specifications/` in this PR
- [ ] Model used for code generation noted (in this PR and on the Linear issue, if delegated)
- [ ] `cargo test` green; WPT harness A/B run for engine changes (zero regressions)
- [ ] Benchmarks + demo suite re-run for engine changes; results committed with the PR
- [ ] `python3 scripts/validate_docs.py` passes
- [ ] Tests added or updated for behavior changes
