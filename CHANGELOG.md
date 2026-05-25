# Changelog

## Unreleased

- Add provider plugin entries for Abacus AI, Alibaba, Alibaba Token Plan, AWS Bedrock, Azure OpenAI, Command Code, Deepgram, Droid, ElevenLabs, Grok, GroqCloud, LLM Proxy, Manus, Moonshot, OpenCode, StepFun, T3 Chat, Vertex AI, and Xiaomi MiMo.
- Back `usagestat cost` and Codex/Claude cost rows with pinned ccusage runners, including total-token columns and 30-day cost summaries.
- Add a local beta workflow: `scripts/install-dev.sh` installs `usagestat-dev` and synced dev plugins under `~/.local`.
- Harden ccusage runner discovery and cleanup by resolving NVM default aliases and killing Unix child process groups on timeout.
- Move provider icon and link metadata into plugin manifests/list output, with absolute monochrome SVG paths and separate color SVG paths where available.
- Add T3 Chat web probing with configured cookie/cURL-header support for Vercel-protected sessions.

## 1.0.2 - 2026-05-18

- Add `usagestat test https` to smoke-test the same HTTPS path used by provider plugins.
- Install provider plugins in distro packages and release tarballs.
- Discover installed provider plugins from the binary install prefix.
- Move dev-only providers (`mock`, `host-smoke`) out of the production plugin bundle.
- Add a copyable custom provider plugin template.

## 1.0.1 - 2026-05-18

- Fix HTTPS provider probes by installing the rustls ring crypto provider before plugin HTTP requests.

## 1.0.0 - 2026-05-17

- Initial UsageStat release.
- Publish Linux release binaries and tarballs.
- Add package metadata for COPR, AUR, Homebrew, and Launchpad PPA.
