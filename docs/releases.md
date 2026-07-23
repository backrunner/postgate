# GitHub Releases And Automatic Updates

PostGate publishes signed Tauri updater artifacts from `.github/workflows/release.yml`. The desktop app supports two update channels:

```text
Stable: https://github.com/backrunner/postgate/releases/latest/download/latest.json
Beta:   https://github.com/backrunner/postgate/releases/download/beta/latest.json
```

Stable only receives production releases. Beta receives prereleases and then follows the final stable build when its version is newer. Both channels use the same updater signing key and signature verification path.

## Required Repository Secrets

- `TAURI_SIGNING_PRIVATE_KEY`: the minisign private key content or path used by the Tauri updater signer.
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: optional private key password when the key is encrypted.

The matching public key must remain in `apps/desktop/src-tauri/tauri.conf.json`. Replacing either side without updating the other makes every client reject new updates.

Production macOS signing and notarization additionally require these secrets:

- `APPLE_CERTIFICATE`: base64-encoded Developer ID Application `.p12` certificate.
- `APPLE_CERTIFICATE_PASSWORD`: password for the `.p12` file.
- `APPLE_SIGNING_IDENTITY`: Developer ID Application identity.
- `APPLE_TEAM_ID`: Apple Developer team ID.
- `APPLE_PROVISIONING_PROFILE`: base64-encoded Developer ID provisioning profile for `com.alkinum.postgate`, with the `iCloud.com.alkinum.postgate` CloudKit container enabled.

Notarization accepts either an App Store Connect API key (preferred):

- `APPLE_API_KEY_ID`: App Store Connect key ID.
- `APPLE_API_ISSUER`: App Store Connect issuer ID.
- `APPLE_API_KEY_BASE64`: base64-encoded contents of the `AuthKey_<KEY_ID>.p8` private key.

Or Apple ID credentials:

- `APPLE_ID`: Apple account used for notarization.
- `APPLE_PASSWORD`: app-specific password for that account.

Signed macOS releases require the provisioning profile so the CloudKit entitlements survive code signing. Local builds use the Development environment from `apps/desktop/src-tauri/Entitlements.plist` with `tauri.cloudkit.conf.json`; releases use `tauri.cloudkit.production.conf.json` and `Entitlements.production.plist`. The workflow validates the profile team, exact App ID, expiration, debug policy, container, CloudKit service, and Production environment before embedding it as `Contents/embedded.provisionprofile`. It then verifies the embedded profile and final signed entitlements before publishing. Unsigned builds omit the profile and cannot access CloudKit.

## CloudKit Setup

1. Register the explicit App ID `com.alkinum.postgate` in Certificates, Identifiers & Profiles.
2. Register the iCloud container `iCloud.com.alkinum.postgate`, enable CloudKit, and assign it to the App ID.
3. Create development and Developer ID provisioning profiles that include that container.
4. For a local signed bundle, place the development profile at `apps/desktop/src-tauri/embedded.provisionprofile` and build with `--config src-tauri/tauri.cloudkit.conf.json`.
5. Validate and import `apps/desktop/src-tauri/CloudKit.schema.ckdb` into the Development environment with `cktool`, then deploy its schema changes to Production before publishing.
6. Export both environments with `cktool` and verify that `PostGateProfile` contains an Asset field named `payload`.

## Release Procedure

1. Ensure CI is green on `main`.
2. Create and push a semantic-version tag, or run the Release workflow manually with an explicit channel.
3. Use `v0.2.0` with the `stable` channel for a production release, or `v0.3.0-beta.1` with the `beta` channel for a preview release. Tag-triggered runs derive the channel from the version.
4. The workflow creates or reuses a draft release, builds macOS ARM64 and x64 bundles, signs updater artifacts, and merges both entries into `latest.json`.
5. The final job verifies both macOS updater platforms before publishing. Stable releases become GitHub's latest release. Beta releases are marked as prereleases.
6. After publication, the workflow refreshes the rolling `beta` release manifest when the new version is newer than the current Beta feed.

The workflow rejects prerelease versions on the Stable channel and requires `-beta` versions on the Beta channel. It is safe to rerun while a release remains a draft and refuses to overwrite an already published version tag.

## Signing Key Setup

Generate the updater key once on a trusted machine:

```bash
pnpm --filter @postgate/desktop tauri signer generate -w ~/.tauri/postgate.key
```

Store the private key and password as GitHub Actions secrets. Put the generated public key in the updater `pubkey` field, then verify a release before distributing that build. Keep the private key backed up securely; losing it prevents installed clients from accepting future updates.
