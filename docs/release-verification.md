# Release Verification Guide

This guide explains how to verify a ZERO release before installing or sharing
it. It checks artifact integrity, GitHub artifact attestations, SBOM/provenance
metadata, the committed Homebrew formula, and the clean-download evidence record.

It verifies release integrity only. It does not prove live trading safety,
hosted custody, future package-registry publication, or profitability.

## From A Fresh Clone

Use this path when you want the repository verifier to download the release
from GitHub and check everything in a temporary clean directory:

```bash
git clone https://github.com/zero-intel/zero.git
cd zero
just release-evidence v0.1.1
```

For machine-readable output:

```bash
scripts/release_evidence.py v0.1.1 --json
```

The release evidence command:

- reads the published GitHub Release metadata;
- downloads every attached release asset into a clean temporary directory;
- verifies `SHA256SUMS` with `shasum -a 256 -c SHA256SUMS`;
- runs `scripts/release_verify.py` against the downloaded directory;
- verifies executable GitHub artifact attestations;
- renders a Homebrew formula from the downloaded checksums;
- fails if the rendered formula differs from the committed `Formula/zero.rb`.

The current published evidence is recorded in
[v0.1.1 release evidence](releases/v0.1.1-evidence.md). That page is evidence
for `v0.1.1` only; future releases need their own clean-download evidence.

## From Downloaded Assets

Use this path when you already downloaded all GitHub Release assets into one
directory:

```bash
cd /path/to/downloaded/zero-release
shasum -a 256 -c SHA256SUMS
/path/to/zero/scripts/release_verify.py .
```

Expected launch assets include:

- `SHA256SUMS`
- `zero-linux`
- `zero-macos`
- `zero-paper-image.tar`
- `zero_engine-<version>-py3-none-any.whl`
- `zero_engine-<version>.tar.gz`
- `SBOM.spdx.json`
- `PROVENANCE.json`

`scripts/release_verify.py` checks that the checksum manifest covers exactly
the release assets, every checksum matches, expected launch assets are present,
assets are non-empty, and the metadata files parse with the expected safety
claims.

## Verify GitHub Artifact Attestations

Run attestation verification from the downloaded asset directory:

```bash
gh attestation verify zero-linux -R zero-intel/zero
gh attestation verify zero-macos -R zero-intel/zero
```

These commands prove that GitHub has signed provenance for the executable
artifacts attached to the release. They do not prove that the executable is safe
to use for live capital; they prove release provenance for the downloaded file.

## Read SBOM And Provenance

The release verifier requires both files:

```bash
python3 -m json.tool SBOM.spdx.json >/dev/null
python3 -m json.tool PROVENANCE.json >/dev/null
```

`SBOM.spdx.json` records package/component metadata. `PROVENANCE.json` records
source commit, tag, asset hashes, dirty-state policy, and release assertions
such as paper-first defaults and no package-registry publication.

## Check The Homebrew Formula

The public repo works as its own Homebrew tap:

```bash
brew tap zero-intel/zero https://github.com/zero-intel/zero
brew install zero
```

The committed formula at `Formula/zero.rb` must be generated from a verified
release directory:

```bash
scripts/homebrew_formula.py /path/to/downloaded/zero-release --tag v0.1.1 --output /tmp/zero.rb
diff -u Formula/zero.rb /tmp/zero.rb
scripts/homebrew_formula_check.py
```

The formula drift check proves that the tap points at the same GitHub Release
assets and checksums as the downloaded release. If the rendered formula differs
from `Formula/zero.rb`, the tap is stale or the release verification input is
wrong.

## Refuse On Any Failure

Do not install or redistribute a release when any of these fail:

- `shasum -a 256 -c SHA256SUMS`
- `scripts/release_verify.py <downloaded-release-dir>`
- `gh attestation verify zero-linux -R zero-intel/zero`
- `gh attestation verify zero-macos -R zero-intel/zero`
- `scripts/homebrew_formula_check.py`
- `just release-evidence <tag>`

Treat failure as an integrity incident until a maintainer publishes corrected
evidence or replaces the release.
