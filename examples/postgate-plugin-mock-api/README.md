# Mock API Plugin

Install this directory from PostGate's Plugins page, enable `postgate-plugin-mock-api`, and add a rule:

```text
example.test plugin://mock-api?mode=fixture
```

Requests under `/__postgate/mock` return a local JSON response. Other matching requests continue upstream and receive an `x-postgate-plugin: mock-api` response header.
