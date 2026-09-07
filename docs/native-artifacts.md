# Native release artifacts

Implementation: [#14](https://github.com/hashimkarim/usagestat/issues/14).
Native archive qualification is pending; these assets are not newly published
downloads. Existing Linux asset names and package consumers are preserved.

| Target | Archive | Contents |
| --- | --- | --- |
| Linux x64 | `usagestat-linux-x86_64.tar.gz` | CLI, daemon, plugins/icons, combined license notices |
| Linux ARM64 | `usagestat-linux-aarch64.tar.gz` | Same |
| macOS ARM64 | `usagestat-macos-aarch64.tar.gz` | Same |
| macOS Intel | `usagestat-macos-x86_64.tar.gz` | Same |
| Windows x64 | `usagestat-windows-x86_64.zip` | `usagestat.exe`, `usagestatd.exe`, `usagestat-service.exe`, plugins/icons, combined license notices |

Archives have a flat executable/resource layout. Linux retains its standalone
CLI assets as well. Every asset and manifest has a SHA-256 sidecar. Archive
members have fixed timestamps, ownership, modes and ordering; identical binary
and resource inputs produce identical archive bytes. This is reproducible
packaging, not a claim that unrelated compiler runs produce identical binaries.

Each `<asset>.manifest.json` describes schema/version/source commit, dirty-source
state, OS/CPU/libc, candidate minimum system and qualification state, compiler,
locked native dependencies, executable formats/versions/imports, archive digest,
every payload file's digest/size/mode and a shared resource digest.
`usagestat-artifacts.json` combines the matching target manifests. npm consumes
these exact artifacts and file hashes; its Windows dependency includes the
internal service launcher. Native packages have no Node runtime requirement.

Packaging validates ELF, Mach-O and PE architectures. Linux required GLIBC
versions must remain within 2.39. Both Mach-O slices must target macOS 11 or lower
and only import system libraries. Windows release builds link the MSVC runtime
statically, reject dynamic VC runtime imports and require a GUI-subsystem service
launcher. These checks do not prove minimum-OS execution, signing or notarization.
The current macOS and Windows artifacts are explicitly unsigned candidates.

`LICENSE` includes the project's license and notices from the locked dependency
union for all initial native targets, including bundled QuickJS/SQLite notices.
The source license remains unchanged. Bundled resources retain their exact Git
bytes on every checkout; generated manifests verify the same resource digest on
every architecture before release inputs can be aggregated.

The existing Release workflow now accepts manual dispatch on the implementation
branch. Manual runs build, extract, smoke-test, validate legacy Linux package
inputs and upload CI artifacts; they never create tags or publish releases.
Tag pushes retain the existing stable/prerelease publication behavior. Stable
downloads include only targets whose minimum-system qualification is recorded;
Linux remains eligible while macOS/Windows await #20. Prereleases can include all
staged candidates. Registry publication and Apple signing are separate tasks.

To rehearse locally on a supported native machine (Python 3.12 and Rust 1.89):

```sh
cargo build --release --locked --workspace --target YOUR_NATIVE_RUST_TARGET
python tools/publish/scripts/native_artifacts.py --target YOUR_NATIVE_RUST_TARGET --binary-dir target/YOUR_NATIVE_RUST_TARGET/release --output target/native-release
python tools/publish/scripts/native_artifacts.py --verify target/native-release/YOUR_ASSET.manifest.json
```

On Windows set `RUSTFLAGS=-C target-feature=+crt-static` in the build environment.
On macOS set `MACOSX_DEPLOYMENT_TARGET=11.0`. The supplied target must match the
machine actually executing the binaries. Use a fresh output directory if inputs
change; staging refuses to overwrite different existing bytes. Dirty-source
local rehearsals are marked and cannot be aggregated for publication.

Archive verification checks every member before extraction, rejects traversal,
links and case aliases, lists the extracted providers/icons from an unrelated
directory, then exercises those exact binaries in the isolated native runtime
suite. It probes only synthetic providers. All real bundled providers are listed
and hashed, without reading their credentials or making usage requests.
