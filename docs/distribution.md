# Release distribution

## Automatic publishing

Stable `vMAJOR.MINOR.PATCH` tags now run the complete release pipeline:

1. Run the workspace tests and build both Linux architectures.
2. Publish the GitHub release and its checksums.
3. Publish AUR, Homebrew, Fedora COPR, and the Ubuntu PPA in independent jobs.

Prereleases such as `v1.0.4-beta.1` remain on GitHub. Package publishing validates
that the requested tag is the latest published stable release, verifies both
archive checksums and architectures, and generates recipe versions/checksums
from those downloads. A missing credential fails the affected job explicitly;
it does not silently skip a repository or prevent the other repositories from
publishing.

### One-time GitHub Actions configuration

Set these **repository secrets** on `Hashim-K/usagestat`:

| Secret | Value |
| --- | --- |
| `AUR_SSH_PRIVATE_KEY` | Unencrypted SSH private key registered to the AUR maintainer of `usagestat-bin`. |
| `HOMEBREW_SSH_PRIVATE_KEY` | Unencrypted SSH private key whose public key is a **write-enabled deploy key** on `Hashim-K/homebrew-tap`. |
| `COPR_CONFIG` | Complete COPR API configuration, including the `[copr-cli]` section, login, token, and username. |
| `PPA_GPG_PRIVATE_KEY` | ASCII-armored private signing key registered to the Launchpad PPA uploader. |
| `PPA_GPG_PASSPHRASE` | Signing-key passphrase, if the exported key is protected; otherwise omit. |

Set these **repository variables**:

| Variable | Value |
| --- | --- |
| `AUR_SSH_KNOWN_HOSTS` | Verified `aur.archlinux.org` SSH known-hosts entry. Strict host verification stays enabled. |
| `PPA_GPG_FINGERPRINT` | Full fingerprint of the imported Launchpad signing key. |

Create new AUR SSH and Launchpad OpenPGP keys alongside existing account keys;
do not replace them. Launchpad requires the new public key to be published to
its keyserver and confirmed on the uploader account. A CI signing subkey can
be exported without exposing the primary private key.

COPR has one API token per account: `copr-cli new-api-token` invalidates the
existing token. For an independent CI credential, use a separate Fedora
account granted builder access to `hashimkarim/usagestat`. Its configuration
still belongs in `COPR_CONFIG`; the project owner does not change. Reusing an
existing token does not rotate it, but should be an explicit choice.

Use dedicated publishing credentials where possible. The Homebrew key only
needs access to the tap, not a personal GitHub account token. Secrets are kept
out of package artifacts and Git remotes; temporary key files are removed on
exit. Dry runs do not upload packages or require publishing credentials.

### Retry an existing release

In **Actions → Publish package repositories → Run workflow**, select `main`,
enter the existing stable tag, choose `all` or one repository, and set **dry_run**
to false to publish. For example:

```bash
gh workflow run publish-packages.yml --ref main \
  -f tag=v1.0.3 -f platform=all -F dry_run=false
```

Leave `dry_run=true` for a rehearsal. This supports publishing a release made
before the automation was added, without moving its tag or rebuilding its
GitHub assets. AUR and Homebrew commit only changed recipes. COPR waits for an
existing build, or submits a new build if no successful attempt exists. The PPA
waits for existing uploads and for the resulting binaries to be published;
already used PPA versions cannot be uploaded again after a failed build. Retry
that build in Launchpad, or change the Debian package revision deliberately.

Repository jobs are serialized per platform and have a 75-minute timeout.
Remote build/publishing waits have a 45-minute timeout. A timeout reports a
failure; rerunning the job checks the existing remote state first. Generated
recipes and Debian source packages are retained as workflow artifacts.

The AUR package targets x86-64. Homebrew supports Linux x86-64 and ARM64. COPR
builds the project's enabled Fedora targets. The PPA keeps the existing `noble`
series and `-1ppa1` version scheme, and supports its enabled amd64/arm64 builders.
Version 1.0.3 uses `-1ppa2` to replace a package rejected for epoch-zero file
timestamps. The Debian install step sets bundled timestamps from the package
changelog; the reproducible upstream source archive remains unchanged. Uploads
and publication checks share the revision in `publication-state.py`.
As in the existing PPA, its Debian source package wraps the validated release
binaries and plugins. Both the CLI and daemon are installed. It does not run
Cargo on Launchpad.

## Local-first publishing

Publishing should be rehearsed locally before pushing a release tag or
submitting to package repositories.

Build the local release artifacts exactly like the GitHub Release workflow:

```bash
tools/publish/scripts/local-release-build.sh 1.0.0
```

Artifacts are written to:

```text
dist/releases/v1.0.0/
```

Build local distro packages:

```bash
tools/publish/scripts/local-deb-build.sh
tools/publish/scripts/local-rpm-build.sh
```

Both package builders install the resulting package inside the publisher
container and run:

```bash
usagestat --version
usagestat test https
```

Packages are written to:

```text
dist/packages/v1.0.0/
```

Build publisher containers:

```bash
tools/publish/scripts/docker-build.sh arch-aur
tools/publish/scripts/docker-build.sh fedora-copr
tools/publish/scripts/docker-build.sh ubuntu-ppa
```

Open a shell in a publisher container:

```bash
tools/publish/scripts/docker-run.sh arch-aur
tools/publish/scripts/docker-run.sh fedora-copr
tools/publish/scripts/docker-run.sh ubuntu-ppa
```

Optional credential mounts:

```bash
MOUNT_SSH=1 tools/publish/scripts/docker-run.sh arch-aur
MOUNT_COPR=1 tools/publish/scripts/docker-run.sh fedora-copr
MOUNT_GNUPG=1 tools/publish/scripts/docker-run.sh ubuntu-ppa
```

## GitHub Releases

Push a semver tag prefixed with `v`:

```bash
git tag v1.0.0
git push origin v1.0.0
```

The release workflow builds the `usagestat` binary with `cross` for:

- `x86_64-unknown-linux-gnu` -> `usagestat-linux-x86_64`
- `aarch64-unknown-linux-gnu` -> `usagestat-linux-aarch64`

Each release also includes `.sha256` checksum files and `.tar.gz` archives for package managers.

## AUR

Use `packaging/aur/usagestat-bin/PKGBUILD` for the binary package. Before publishing:

1. Set `pkgver` to the release version without the leading `v`.
2. Set `sha256sums_x86_64` to the checksum from `usagestat-linux-x86_64.tar.gz.sha256`.
3. Validate in the Arch container:

```bash
tools/publish/scripts/aur-check.sh
```

4. Run `makepkg --printsrcinfo > .SRCINFO`.
5. Commit `PKGBUILD` and `.SRCINFO` to the `usagestat-bin` AUR Git repository.

A separate source-build `usagestat` AUR package should wait until the CLI crate is published to crates.io or a source release tarball with vendored dependencies is produced.

## Homebrew

The existing tap is `github.com/Hashim-K/homebrew-tap`. For manual publishing, copy `packaging/homebrew/Formula/usagestat.rb` to `Formula/usagestat.rb`.

For each release, update:

- `version`
- Linux x86_64 tarball `sha256`
- Linux aarch64 tarball `sha256`

Install command:

```bash
brew install hashim-k/tap/usagestat
```

## COPR

Use `packaging/rpm/usagestat.spec` as the starting spec for COPR package `hashimkarim/usagestat`.

Submit a COPR build from the Fedora publisher container:

```bash
tools/publish/scripts/copr-build.sh hashimkarim/usagestat packaging/rpm/usagestat.spec
```

The current spec builds from the GitHub tag archive and enables network access
for Cargo dependency download. Stricter production builds should either vendor
Rust dependencies into the source package or use Fedora Rust packaging macros
generated by `rust2rpm` so builds do not depend on network access during
`%build`.

User install command:

```bash
dnf copr enable hashimkarim/usagestat
dnf install usagestat
```

## Ubuntu PPA

The CLI crate has `cargo-deb` metadata for generating a local `.deb`:

```bash
cargo install cargo-deb
cargo build --release --locked -p usagestat-cli -p usagestat-daemon
cargo deb -p usagestat-cli --no-build
```

Launchpad PPAs require signed Debian source packages, not just binary `.deb`
uploads. Use the Ubuntu publisher container so the Debian toolchain is
consistent:

```bash
tools/publish/scripts/ppa-shell.sh
```

Inside the shell, prepare a Debian source package with a signed `.dsc` and
upload it with `dput`.

User install command:

```bash
add-apt-repository ppa:hashimkarim/usagestat
apt install usagestat
```
