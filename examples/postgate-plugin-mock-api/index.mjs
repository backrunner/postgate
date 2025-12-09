/**
 * PostGate Mock API Plugin
 * 
 * A sample plugin that demonstrates how to create mock API responses.
 * This plugin uses the built-in PostGate runtime APIs (no external dependencies needed).
 * 
 * Usage in rules:
 *   api.example.com/users plugin://mock-api?status=200&delay=100
 *   api.example.com/users plugin://mock-api?file=users.json
 * 
 * Configuration options:
 *   - status: HTTP status code to return (default: 200)
 *   - delay: Response delay in ms (default: 0)
 *   - file: Mock data file name (relative to plugin directory)
 *   - contentType: Response content type (default: application/json)
 */

// Mock data storage
const mockData = new Map();

// Default mock responses for common endpoints
const defaultMocks = {
  '/api/users': {
    status: 200,
    data: {
      users: [
        { id: 1, name: 'Alice Johnson', email: 'alice@example.com' },
        { id: 2, name: 'Bob Smith', email: 'bob@example.com' },
        { id: 3, name: 'Charlie Brown', email: 'charlie@example.com' },
      ],
      total: 3,
    },
  },
  '/api/users/:id': {
    status: 200,
    data: { id: 1, name: 'Alice Johnson', email: 'alice@example.com', createdAt: '2024-01-15' },
  },
  '/api/posts': {
    status: 200,
    data: {
      posts: [
        { id: 1, title: 'Hello World', content: 'This is a test post' },
        { id: 2, title: 'Second Post', content: 'Another test post' },
      ],
      total: 2,
    },
  },
  '/api/health': {
    status: 200,
    data: { status: 'healthy', timestamp: new Date().toISOString() },
  },
};

/**
 * PostGate Plugin Definition
 * 
 * The plugin object is automatically detected by the runtime.
 * Available context APIs:
 *   - ctx.logger: { debug, info, warn, error }
 *   - ctx.storage: { get, set, delete, has, keys, clear }
 *   - ctx.ui: { registerPanel, unregisterPanel, toast }
 *   - ctx.config: Configuration object from the matched rule
 */
export default {
  name: 'mock-api',
  version: '1.0.0',
  description: 'Mock API responses for testing',

  async onLoad(ctx) {
    ctx.logger.info('Mock API plugin loaded');
    
    // Register a UI panel
    ctx.ui.registerPanel({
      id: 'mock-api-panel',
      plugin_id: 'mock-api',
      title: 'Mock API',
      icon: 'Database',
      content: {
        type: 'html',
        html: `
          <div style="padding: 16px; font-family: system-ui, sans-serif;">
            <h2 style="margin: 0 0 16px 0; font-size: 18px;">Mock API Plugin</h2>
            <p style="color: #666; margin-bottom: 16px;">
              This plugin provides mock API responses for testing.
            </p>
            <h3 style="font-size: 14px; margin-bottom: 8px;">Available Endpoints:</h3>
            <ul style="padding-left: 20px; color: #666;">
              <li>/api/users - List of users</li>
              <li>/api/users/:id - Single user</li>
              <li>/api/posts - List of posts</li>
              <li>/api/health - Health check</li>
            </ul>
            <h3 style="font-size: 14px; margin: 16px 0 8px 0;">Usage:</h3>
            <code style="background: #f0f0f0; padding: 8px; display: block; border-radius: 4px; font-size: 12px;">
              api.example.com/api/* plugin://mock-api
            </code>
          </div>
        `,
      },
    });

    // Initialize mock data from storage if available
    try {
      const storedMocks = await ctx.storage.get('customMocks');
      if (storedMocks) {
        for (const [key, value] of Object.entries(storedMocks)) {
          mockData.set(key, value);
        }
        ctx.logger.info(`Loaded ${mockData.size} custom mocks from storage`);
      }
    } catch (e) {
      ctx.logger.warn('Failed to load custom mocks from storage: ' + e.message);
    }

    ctx.ui.toast('Mock API plugin ready', 'success');
  },

  async onUnload(ctx) {
    ctx.logger.info('Mock API plugin unloading...');
    ctx.ui.unregisterPanel('mock-api-panel');
  },

  async handleRequest(request, ctx) {
    ctx.logger.debug(`Handling request: ${request.method} ${request.path}`);

    // Get configuration from rule
    const config = ctx.ruleConfig || {};
    const delay = parseInt(config.delay) || 0;
    const customStatus = parseInt(config.status);
    const contentType = config.contentType || 'application/json';

    // Add delay if configured
    if (delay > 0) {
      await new Promise(resolve => setTimeout(resolve, delay));
    }

    // Try to find a matching mock
    let mockResponse = findMock(request.path, request.method);

    if (!mockResponse) {
      // Return a default 404 response
      mockResponse = {
        status: customStatus || 404,
        data: {
          error: 'Not Found',
          message: `No mock defined for ${request.method} ${request.path}`,
          path: request.path,
        },
      };
    }

    // Apply custom status if provided
    if (customStatus) {
      mockResponse.status = customStatus;
    }

    // Create response
    const responseBody = JSON.stringify(mockResponse.data, null, 2);
    const bodyBytes = new TextEncoder().encode(responseBody);
    
    // Return response in the format expected by PostGate
    // body should be base64 encoded
    return {
      status: mockResponse.status,
      headers: {
        'content-type': contentType,
        'content-length': String(bodyBytes.length),
        'x-mock-api': 'true',
        'x-mock-delay': String(delay),
      },
      body: btoa(String.fromCharCode(...bodyBytes)),
      body_base64: true,
    };
  },

  async handleResponse(request, response, ctx) {
    // Pass through - we don't modify responses in this plugin
    return response;
  },
};

/**
 * Find a matching mock for the given path and method
 */
function findMock(path, method) {
  // Check custom mocks first
  const customKey = `${method}:${path}`;
  if (mockData.has(customKey)) {
    return mockData.get(customKey);
  }

  // Check path-only custom mocks
  if (mockData.has(path)) {
    return mockData.get(path);
  }

  // Check default mocks
  for (const [pattern, mock] of Object.entries(defaultMocks)) {
    if (matchPath(pattern, path)) {
      return mock;
    }
  }

  return null;
}

/**
 * Simple path matching with :param support
 */
function matchPath(pattern, path) {
  const patternParts = pattern.split('/');
  const pathParts = path.split('/');

  if (patternParts.length !== pathParts.length) {
    return false;
  }

  for (let i = 0; i < patternParts.length; i++) {
    const patternPart = patternParts[i];
    const pathPart = pathParts[i];

    // Skip param placeholders
    if (patternPart.startsWith(':')) {
      continue;
    }

    if (patternPart !== pathPart) {
      return false;
    }
  }

  return true;
}

// Helper: Base64 encode (if btoa is not available)
if (typeof btoa === 'undefined') {
  globalThis.btoa = function(str) {
    const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=';
    let output = '';
    for (let i = 0; i < str.length; i += 3) {
      const a = str.charCodeAt(i);
      const b = str.charCodeAt(i + 1);
      const c = str.charCodeAt(i + 2);
      output += chars[a >> 2];
      output += chars[((a & 3) << 4) | (b >> 4)];
      output += chars[isNaN(b) ? 64 : ((b & 15) << 2) | (c >> 6)];
      output += chars[isNaN(c) ? 64 : (c & 63)];
    }
    return output;
  };
}
