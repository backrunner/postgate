---
title: Settings, profiles, and updates
description: Configure listeners, transfer profiles, sync settings, and receive signed GitHub Release updates.
navTitle: Settings and updates
order: 32
---

# Settings, profiles, and updates

## Proxy configuration

Settings controls the localhost proxy port, HTTP/2, experimental QUIC/HTTP/3, and the DevTools service port. If the installed build does not include QUIC support, the QUIC toggle is disabled and explains why.

Changing a listener port requires the affected service to restart. Avoid ports already used by a development server or another proxy.

## Software updates

PostGate checks the signed update manifest published with the latest GitHub Release. Settings provides:

- manual **Check for Updates**
- automatic checks at startup
- optional background download
- download, install, and restart progress

The release workflow currently builds Apple silicon and Intel macOS packages, signs their updater artifacts, validates both Darwin entries in `latest.json`, and publishes the draft only after both builds succeed. PostGate verifies updater signatures before installation.

The website download area checks GitHub separately and links directly to the selected macOS installer. Windows is marked **Coming soon** and has no download action. If no macOS release exists or GitHub is unavailable, the macOS action opens the repository's Releases page instead.

## Profile transfer

A profile can include rules, values, Replay collections, certificate material, app preferences, and sync configuration. The import flow previews the profile before restoring its data. You can also import compatible Whistle rules into a new rule group without restoring a complete profile.

Profiles that include the CA private key, WebDAV password, headers, or request bodies are sensitive. Store and transfer them accordingly.

## Settings sync

Sync uses the same profile snapshot format as manual transfer:

- iCloud writes a local Cloud Drive file on supported macOS builds.
- WebDAV uploads the JSON snapshot to the configured endpoint and remote path.

Save the sync provider settings before using Push or Pull. Push replaces the remote snapshot with local state. Pull imports the remote snapshot into PostGate, so export a backup before the first pull if the current setup already contains data.

## Appearance

Choose light, dark, or system mode. The documentation site uses the same three-way theme behavior but keeps its preference separate from the desktop app.
