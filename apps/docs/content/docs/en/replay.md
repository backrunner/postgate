---
title: Replay requests
description: Save captured traffic into collections, edit requests, and execute them repeatedly.
navTitle: Replay
order: 30
---

# Replay requests

Replay turns a captured request into a repeatable local test. Requests are stored in collections and retain execution history.

## Import from Capture

Select a Capture row and import it into Replay. PostGate copies the method, URL, query parameters, headers, and body into an editable request.

Review credentials and session cookies before saving or sharing a request. Replay sends the values currently shown in the editor.

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

Replay uses the same HTTP environment as the desktop app but represents a direct saved request workflow. When validating a proxy rule, confirm the executed request is routed through the active PostGate proxy path and compare it with the corresponding Capture row.
