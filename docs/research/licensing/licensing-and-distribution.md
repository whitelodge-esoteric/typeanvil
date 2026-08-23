---
title: Licensing & Commercial Distribution
type: research
status: draft
owner: Elijah Boston
created: 2026-08-21
updated: 2026-08-21
sidebar_position: 1
tags: [licensing, distribution, commercial]
---

# Licensing & Commercial Distribution

## Decision

Typeanvil is a **commercial, closed-source product**. No open-source
release is planned. The WPT conformance harness stays private (it is the
moat). Publishing harness *results* later remains an option for marketing.

## Goals

1. Paying customers can self-host the binary with minimal friction.
2. Unlicensed use is visible, not blocked — trials and evaluation are
   effortless.
3. No mandatory phone-home; air-gapped installs must work fully.
4. The dependency tree must stay compatible with closed-source
   distribution.

## Non-Goals

- DRM-grade tamper resistance. A determined user with a stripped binary
  can patch any client-side check. Legal terms + ease of honest compliance
  are the real protection (Prince's model).
- Blocking unlicensed runs outright.

## Recommended scheme

1. **Signed license files.** Ed25519 keypair; we hold the private key,
   the binary embeds the public key. License file carries: customer name,
   expiry date, edition/tier, seat or host count. Binary verifies the
   signature offline at startup. Works air-gapped, survives proxies.
2. **Watermark fallback.** Missing/expired license → engine still renders,
   every page gets a small watermark ("Unlicensed — TypeAnvil"). This is
   how Prince handles it. Trials need no sales conversation, and unlicensed
   production use advertises us.
3. **Phone-home activation, enterprise tier only.** Optional, opt-in per
   contract. Never required broadly — it kills self-hosted adoption.
4. **Distribution gate as separate friction layer (optional).** Private
   npm/GitHub Packages repo with per-customer download tokens reduces
   casual re-hosting of binaries. This is *not* the license mechanism;
   tokens protect the download only, not the bits afterward.

## Code seam (do early, cheap now)

One choke point in the render pipeline that asks "licensed?" before
rendering. Hardcode `true` today; grow into the real verifier later.
Retrofitting checks into layout hot paths later is painful.

```rust
// single call site in main.rs / render entry
let license = licensing::load_and_verify(); // stubbed true for now
match license {
    Ok(l) => render_full(...),
    Err(_) => render_with_watermark(...),
}
```

Design note: the watermark must be applied at PDF-emission level (pdf.rs)
so it cannot be skipped by CSS tricks; it draws after page content on
every page.

## Dependency-license constraint

Closed-source Rust binary rules:

| License | Closed-source use |
|---|---|
| MIT / Apache-2.0 / BSD | Fine; keep attribution notices |
| MPL-2.0 | Fine when used unmodified; file-level copyleft only bites if we modify those crate files (then owe those files' source) |
| GPL / LGPL | Problematic; forces disclosure. Must be kept out |

Action: add `cargo-deny` to CI with a license allowlist so a GPL
transitive dependency cannot enter via a future `cargo add`. Also keep
attribution/NOTICE generation automated (`cargo-about` or similar) since
we ship proprietary binaries containing MIT/Apache/BSD code.

Current known stack check: html5ever + stylo are Apache/MIT dual — fine.
krilla and font crates to be verified during implementation.

## Housekeeping before v1 packaging

- Remove `license = "MPL-2.0"` from `engine/Cargo.toml` (private repo;
  commercial EULA governs at distribution time).
- Write the commercial EULA / license agreement (decision points:
  seat vs. server vs. revenue-based pricing, trial terms, watermark
  removal clause).

## Cloud vs self-hosted gating

Cloud hosted version is gated at the API/service boundary — standard auth
and billing. Self-hosted uses the signed-license scheme above. Both share
the same entitlement records internally so upgrades between tiers are a
license-file swap.

## Open questions

1. Pricing tiers and what each unlocks (edition field in the license).
2. Do we ever publish conformance results publicly? (Harness stays
   private either way.)
3. EULA drafting timing — before first paying pilot customer.
