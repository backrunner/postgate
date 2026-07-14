# GitHub Releases And Automatic Updates

PostGate publishes signed Tauri updater artifacts from `.github/workflows/release.yml`. Stable releases are available to the desktop updater at:

```text
https://github.com/backrunner/postgate/releases/latest/download/latest.json
```

## Required Repository Secrets

- `TAURI_SIGNING_PRIVATE_KEY`: the minisign private key content or path used by the Tauri updater signer.
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: optional private key password when the key is encrypted.

The matching public key must remain in `apps/desktop/src-tauri/tauri.conf.json`. Replacing either side without updating the other makes every client reject new updates.

Production macOS signing and notarization additionally use these optional secrets:

- `APPLE_CERTIFICATE`: base64-encoded Developer ID Application `.p12` certificate.
- `APPLE_CERTIFICATE_PASSWORD`: password for the `.p12` file.
- `APPLE_SIGNING_IDENTITY`: Developer ID Application identity.
- `APPLE_ID`: Apple account used for notarization.
- `APPLE_PASSWORD`: app-specific password for that account.
- `APPLE_TEAM_ID`: Apple Developer team ID.

## Release Procedure

1. Ensure CI is green on `main`.
2. Create and push a semantic-version tag such as `v0.2.0`, or run the Release workflow manually with `0.2.0`.
3. The workflow creates or reuses a draft release, builds macOS ARM64/x64, Linux x64, and Windows x64 bundles, signs updater artifacts, and merges their entries into `latest.json`.
4. The final job verifies all four updater platforms and only then publishes the release. A failed build or incomplete manifest remains a draft and is never offered to clients.

The workflow is safe to rerun while a release remains a draft. It refuses to overwrite an already published tag.

## Signing Key Setup

Generate the updater key once on a trusted machine:

```bash
pnpm --filter @postgate/desktop tauri signer generate -w ~/.tauri/postgate.key
```

Store the private key and password as GitHub Actions secrets. Put the generated public key in the updater `pubkey` field, then verify a release before distributing that build. Keep the private key backed up securely; losing it prevents installed clients from accepting future updates.
