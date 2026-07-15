---
title: HTTPS and the root certificate
description: Trust the PostGate CA to inspect HTTPS traffic on a development machine.
navTitle: HTTPS certificate
order: 3
---

# HTTPS and the root certificate

PostGate decrypts HTTPS locally so it can capture traffic and apply rules. It creates a local root certificate authority (CA) and generates host certificates on demand.

## Install the certificate

1. Open **Settings → HTTPS & Security**.
2. Choose **Install to System**.
3. Approve the trust prompt from your operating system.
4. Restart browsers that were already open.

If the automatic installer is unavailable, choose **Export**, then import the `.pem` or `.crt` file into the system or browser trust store as a trusted root certificate authority.

## Verify HTTPS capture

With the proxy running and the browser configured for `127.0.0.1:8899`, open an HTTPS page. The request should appear without a certificate warning. Select it to inspect TLS metadata, request and response headers, bodies, and timing.

## Security boundary

The exported root certificate is public and does not need to remain secret. A PostGate profile can also contain the matching private key, so protect exported profiles like credentials.

- Install the CA only on a development machine you control.
- Do not share profiles that include certificate material.
- Remove the CA from the system trust store when you no longer use PostGate.
- Keep the proxy bound to localhost unless you have explicitly designed a trusted network setup.

Certificate errors usually mean that the browser uses a separate trust store, the CA was not marked as trusted, or the browser needs to be restarted. See [Troubleshooting](/docs/troubleshooting) for a focused checklist.
