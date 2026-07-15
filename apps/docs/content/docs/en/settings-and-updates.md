---
title: Settings, profiles, and updates
description: Configure listeners, transfer profiles, sync settings, and install signed updates from GitHub Releases.
navTitle: Settings and updates
order: 32
---

# Settings, profiles, and updates

## Proxy configuration

In **Settings**, you can configure the localhost proxy port, HTTP/2, experimental QUIC/HTTP/3, and the DevTools service port. If the installed build does not include QUIC support, the QUIC toggle is disabled and shows the reason.

Changing a listener port requires the affected service to restart. Avoid ports already used by a development server or another proxy.

## Software updates

PostGate checks the signed update manifest published with the latest GitHub Release. Settings provides:

- Stable and Beta update channels
- manual **Check for Updates**
- automatic checks at startup
- optional background download
- download, install, and restart progress

Stable receives production releases only. Beta receives preview builds and then follows the final stable build when it is newer. Beta builds default to the Beta channel, and profile exports preserve the selected channel.

The release workflow builds macOS packages for Apple silicon and Intel, signs the update artifacts, validates both macOS entries in `latest.json`, and publishes the draft only after both builds succeed. Stable and Beta use separate manifests, so Stable clients never receive prereleases. PostGate verifies each update signature before installation.

The website download area checks GitHub separately and links directly to the selected macOS installer. Windows is marked **Coming soon** and has no download action. If no macOS release exists or GitHub is unavailable, the macOS action opens the repository's Releases page instead.

## Profile transfer

A profile can include rules, values, Replay collections, certificate material, app preferences, and sync configuration. The import flow previews the profile before restoring its data. You can also import compatible Whistle rules into a new rule group without restoring a complete profile.

Profiles that include the CA private key, WebDAV password, headers, or request bodies are sensitive. Protect them like credentials when storing or transferring them.

## Settings sync

Sync uses the same profile snapshot format as manual transfer:

- iCloud writes the profile snapshot to iCloud Drive on supported macOS builds.
- WebDAV uploads the JSON snapshot to the configured endpoint and remote path.

Save the sync provider settings before using **Push** or **Pull**. Push replaces the remote snapshot with the current local state. Pull imports the remote snapshot into PostGate; if the current setup already contains data, export a backup before the first pull.

## Appearance

Choose light, dark, or system mode. The documentation site uses the same three-way theme behavior but keeps its preference separate from the desktop app.
