# Shipped container image for Typeanvil (CORE-247).
#
# Wraps the release binary unchanged (no RUSTFLAGS, no rebuild semantics
# change — see docs/specifications/release-ci.spec.md determinism rules). The
# binary is injected by the release workflow from the CORE-135 build artifact,
# one per architecture (amd64 = musl static, arm64 = gnu glibc).
#
# Base: distroless base-debian12 (nonroot). It carries glibc (needed by the
# arm64 gnu binary), has no shell, and runs as a non-root user — satisfying
# the "non-root, no shell" requirement for both arches. The amd64 musl
# static binary runs on it unchanged.
#
# Layout:
#   /usr/bin/typeanvil            the release binary
#   /usr/share/fonts/typeanvil/   bundled Liberation Sans (4 faces) + OFL license
#   /usr/share/fonts/user/        user font mount point (documented contract)
#
# The release workflow builds with buildx --platform linux/amd64,linux/arm64
# and passes the per-arch binary via the build context (dist/typeanvil-linux-<arch>).

FROM gcr.io/distroless/base-debian12:nonroot

# Per-arch binary from the build context (injected by the release workflow).
ARG TARGETARCH
COPY dist/typeanvil-linux-${TARGETARCH} /usr/bin/typeanvil

# Bundled open-source font set (Liberation Sans, metric-compatible with Arial)
# + its OFL license. This is the Linux fallback when no user fonts are mounted.
COPY docker/fonts/typeanvil/ /usr/share/fonts/typeanvil/

# User font mount point (documented contract). fontdb scans /usr/share/fonts
# on Linux, so a mount here is discovered automatically.
VOLUME ["/usr/share/fonts/user"]

WORKDIR /work
ENTRYPOINT ["/usr/bin/typeanvil"]
