# Demo — TypeAnvil vs PrinceXML Comparison

The wedge's calling card: a browsable side-by-side gallery proving TypeAnvil
renders real documents like Prince. See
`docs/specifications/visual-comparison-demo.spec.md` for the full contract
(manifest schema, scoreboard schema, triage buckets, determinism rules).

## Prince install (macOS)

Prince is a commercial product. **The demo uses Prince's free
non-commercial license** — comparison-only, never for commercial purposes.
By running the demo you accept the license terms shown at install time
(`LICENSE.txt` in the cask directory).

Installed 2026-08-17:

```sh
brew install --cask prince            # downloads the 16.2 cask
cd /opt/homebrew/Caskroom/prince/16.2/prince-16.2-macos
./install.sh /opt/homebrew            # pass the prefix as $1 (the interactive
                                      # prompt defaults to /usr/local, which
                                      # fails on Apple Silicon without sudo)
```

- Binary: `/opt/homebrew/bin/prince`
- Version pinned: **Prince 16.2** (`prince --version` → "Prince 16.2 … Non-commercial License")
- License file: `/opt/homebrew/Caskroom/prince/16.2/prince-16.2-macos/LICENSE.txt`

### Why `$1`?

The installer's interactive prompt (`Press Enter to accept the default
directory`) defaults to `/usr/local`, which is not writable on a Homebrew
Apple Silicon setup. Passing `/opt/homebrew` as the first argument installs
there without sudo. Re-running the installer upgrades in place.

## Wrapper

`scripts/render-prince.sh` translates the `typeanvil render` CLI contract into
Prince flags:

| Engine flag            | Prince flag             |
|------------------------|-------------------------|
| `--page-width` `--page-height` | `--page-size="W H"` (space-separated; `5inx3in` is rejected) |
| `--margin-top/right/bottom/left` | `--page-margin="T R B L"` (same order) |
| `--base-url`           | `--baseurl=`            |
| `-o`                   | `-o`                    |

Verified: `5in × 3in, 0.5in margins` → Prince MediaBox `0 0 360 216`,
identical geometry to `typeanvil render`.

## Pipeline

```sh
scripts/build-demo.sh    # renders corpus through both engines → demo/out/
```

Output: `demo/out/index.html` (gallery) + `demo/out/scoreboard.json`
(schema in the spec). Deterministic: re-running produces byte-identical
output except the scoreboard's `generated` timestamp.
