---
title: Replay requests
description: Save captured requests in collections, edit them, and run them repeatedly.
navTitle: Replay
order: 30
---

# Replay requests

Replay turns a captured request into a repeatable test. Saved requests are organized into collections and retain their execution history.

## Import from Capture

Select a Capture row and import it into Replay. PostGate copies the method, URL, query parameters, headers, and body into an editable request.

Review credentials and session cookies before saving or sharing a request. Replay sends exactly the values currently shown in the editor.

## Build a request

You can also create a request from scratch:

1. Choose or create a collection.
2. Enter the method and complete URL.
3. Add query parameters and headers.
4. Select a body type and enter text, JSON, form fields, or multipart data.
5. Execute the request.

The response view shows status, headers, body, and timing. Execution history makes it possible to compare repeated results after changing a rule or local service.

## Organize collections

Collections can be created, renamed, and removed. Saved requests can be duplicated or moved between collections. Profile export and sync include Replay collections and requests when those options are enabled.

## Replay and rules

Replay sends requests directly from the desktop app. It does not automatically route them through the PostGate proxy, so proxy rules are not applied to Replay requests. To validate a proxy rule, use a browser or another client configured to use PostGate, then inspect the corresponding row in **Capture**.
