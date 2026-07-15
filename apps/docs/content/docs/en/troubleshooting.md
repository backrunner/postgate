---
title: Troubleshooting
description: Diagnose missing traffic, certificate failures, rule mismatches, debug sessions, plugins, and updates.
navTitle: Troubleshooting
order: 40
---

# Troubleshooting

## No requests appear

- Confirm Capture says the proxy is running.
- Copy the exact address from the Capture toolbar into both HTTP and HTTPS proxy fields.
- Check browser and system bypass lists for the target hostname.
- Confirm another process is not using the configured port.
- Start with an HTTP page to separate proxy routing from certificate trust.

## HTTPS shows a certificate warning

- Install the root CA from **Settings → HTTPS & Security**.
- If using Export, import it as a trusted root, not a client certificate.
- Restart the browser after changing trust.
- Check whether the browser maintains a trust store separate from the operating system.
- Remove an obsolete PostGate CA before installing a newly generated one.

## A rule does not run

- Enable its rule group.
- Check the editor's parse status for errors or unsupported-protocol warnings.
- Narrow the pattern and inspect the requested URL in Capture.
- Check method, protocol, header, status, and include/exclude filters.
- Look for an earlier broad route, file, or mock action that changes the same request.

## A file or injection is missing

- Use an absolute file path and confirm PostGate can read it.
- Check that an injection targets a compatible `content-type`.
- Inspect compressed and rewritten response headers in Capture.
- Reload without the browser cache after changing a bundle rule.

## Debug has no connected page

- Enable a matching `debug://` rule before reloading the document.
- Confirm the main document is HTML and passes through PostGate.
- Check the DevTools port for conflicts.
- Check whether Content Security Policy or a browser extension is blocking the local WebSocket.

## A plugin will not load

- Use a package named `postgate-plugin-*` or `@postgate/plugin-*`.
- Ensure `main` or `module` points to JavaScript inside the package.
- Bundle dependencies and remove reliance on Node.js globals.
- Enable the plugin and attach it with a matching `plugin://name` rule.
- Keep request and response handlers below the five-second execution limit.

## Update checks fail

- Open [GitHub Releases](https://github.com/backrunner/postgate/releases) to verify network access and that a stable release exists.
- Confirm that the installed build contains the public key used to verify production updates.
- Do not rename or edit `latest.json` or its signed assets in a release.
- When testing a prerelease, remember that GitHub's stable-release endpoint normally excludes prereleases.

When reporting an issue, include the PostGate version, operating system, smallest rule that reproduces the problem, relevant Capture metadata, and the exact error message. Remove credentials, cookies, certificate keys, and private request or response bodies first.
