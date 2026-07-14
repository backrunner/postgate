function jsonResponse(status, value, headers = {}) {
  const json = JSON.stringify(value);
  return {
    status,
    headers: {
      "content-type": "application/json; charset=utf-8",
      ...headers,
    },
    body: btoa(unescape(encodeURIComponent(json))),
    body_base64: true,
  };
}

module.exports = {
  name: "mock-api",
  version: "0.1.0",

  async onLoad(context) {
    const loadCount = (await context.storage.get("loadCount")) || 0;
    await context.storage.set("loadCount", loadCount + 1);
    context.ui.registerPanel({
      id: "mock-api-status",
      title: "Mock API",
      content: {
        type: "html",
        html: `<!doctype html>
          <meta charset="utf-8">
          <style>
            body { margin: 0; padding: 20px; font: 13px system-ui; color: #18181b; background: #fafafa; }
            code { font-family: ui-monospace, monospace; }
          </style>
          <h2>Mock API plugin is running</h2>
          <p>Requests matching <code>plugin://mock-api</code> can be served locally.</p>`,
      },
    });
    context.ui.toast("Mock API plugin loaded", "success");
  },

  async onUnload(context) {
    await context.storage.set("lastUnloadAt", Date.now());
  },

  async handleRequest(request, context) {
    if (!request.path.startsWith("/__postgate/mock")) {
      return null;
    }

    return jsonResponse(200, {
      ok: true,
      source: "postgate-plugin-mock-api",
      method: request.method,
      path: request.path,
      mode: context.ruleConfig.mode || "default",
    });
  },

  async handleResponse(request, response) {
    return {
      ...response,
      headers: {
        ...response.headers,
        "x-postgate-plugin": "mock-api",
      },
    };
  },
};
