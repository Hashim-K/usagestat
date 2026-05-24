# Changelog

## Unreleased

- Add provider plugin entries for Abacus AI, Alibaba, AWS Bedrock, Command Code, Deepgram, Droid, ElevenLabs, Grok, GroqCloud, LLM Proxy, Manus, Moonshot, OpenCode, StepFun, Vertex AI, and Xiaomi MiMo.

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
