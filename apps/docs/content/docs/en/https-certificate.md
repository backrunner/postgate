---
title: HTTPS and the root certificate
description: Trust the PostGate CA to inspect HTTPS traffic safely on a development machine.
navTitle: HTTPS certificate
order: 3
---

# HTTPS and the root certificate

PostGate decrypts HTTPS locally so it can capture and apply rules. It creates a private root certificate authority and generates short-lived host certificates as traffic arrives.

## Install the certificate

1. Open **Settings → HTTPS & Security**.
2. Choose **Install to System**.
3. Approve the operating-system trust prompt.
4. Restart browsers that were already open.

If the automatic installer is unavailable, choose **Export**, then import the `.pem` or `.crt` file into the system or browser trust store as a trusted root certificate authority.

## Verify HTTPS capture

With the proxy running and the browser configured for `127.0.0.1:8899`, open an HTTPS page. The request should appear without a certificate warning. Select it to inspect TLS metadata, request and response headers, bodies, and timing.

## Security boundary

The exported certificate is public, but the PostGate profile can contain the matching private key. Treat exported profiles as sensitive credentials.

- Install the CA only on a development machine you control.
- Do not share profiles that include certificate material.
- Remove the CA from the system trust store when you no longer use PostGate.
- Keep the proxy bound to localhost unless you have explicitly designed a trusted network setup.

Certificate errors usually mean the browser uses its own trust store, the CA was not marked as trusted, or the browser must be restarted. See [Troubleshooting](/docs/troubleshooting) for a focused checklist.
