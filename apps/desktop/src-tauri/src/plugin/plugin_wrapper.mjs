/**
 * PostGate Plugin Wrapper
 * 
 * This script bootstraps plugins and provides the bridge between
 * the plugin and the PostGate host process.
 */

import { createRequire } from 'module';
import { fileURLToPath } from 'url';
import { dirname, resolve } from 'path';
import { createInterface } from 'readline';

const require = createRequire(import.meta.url);

// Get plugin path from arguments
const pluginPath = process.argv[2];
if (!pluginPath) {
  console.error('Plugin path not provided');
  process.exit(1);
}

// Message handling
function sendMessage(message) {
  console.log(JSON.stringify(message));
}

// Create logger
function createLogger(pluginId) {
  return {
    debug: (message, ...args) => sendMessage({ type: 'log', level: 'debug', message, args }),
    info: (message, ...args) => sendMessage({ type: 'log', level: 'info', message, args }),
    warn: (message, ...args) => sendMessage({ type: 'log', level: 'warn', message, args }),
    error: (message, ...args) => sendMessage({ type: 'log', level: 'error', message, args }),
  };
}

// Create storage interface
let storageId = 0;
const pendingStorage = new Map();

function createStorage(pluginId) {
  return {
    async get(key) {
      const id = storageId++;
      return new Promise((resolve, reject) => {
        pendingStorage.set(id, { resolve, reject });
        sendMessage({ type: 'storage', id, op: { op: 'get', key } });
      });
    },
    async set(key, value) {
      const id = storageId++;
      return new Promise((resolve, reject) => {
        pendingStorage.set(id, { resolve, reject });
        sendMessage({ type: 'storage', id, op: { op: 'set', key, value } });
      });
    },
    async delete(key) {
      const id = storageId++;
      return new Promise((resolve, reject) => {
        pendingStorage.set(id, { resolve, reject });
        sendMessage({ type: 'storage', id, op: { op: 'delete', key } });
      });
    },
    async has(key) {
      const id = storageId++;
      return new Promise((resolve, reject) => {
        pendingStorage.set(id, { resolve, reject });
        sendMessage({ type: 'storage', id, op: { op: 'has', key } });
      });
    },
    async keys() {
      const id = storageId++;
      return new Promise((resolve, reject) => {
        pendingStorage.set(id, { resolve, reject });
        sendMessage({ type: 'storage', id, op: { op: 'keys' } });
      });
    },
    async clear() {
      const id = storageId++;
      return new Promise((resolve, reject) => {
        pendingStorage.set(id, { resolve, reject });
        sendMessage({ type: 'storage', id, op: { op: 'clear' } });
      });
    },
  };
}

// Create UI interface
function createUI(pluginId) {
  return {
    registerPanel(panel) {
      sendMessage({ 
        type: 'registerPanel', 
        panel: { ...panel, plugin_id: pluginId }
      });
    },
    unregisterPanel(panelId) {
      sendMessage({ type: 'unregisterPanel', panel_id: panelId });
    },
    toast(message, toastType = 'info') {
      sendMessage({ type: 'toast', message, toastType });
    },
  };
}

// Load the plugin
let plugin = null;
let pluginContext = null;

async function loadPlugin() {
  try {
    // Load the plugin module
    const pluginModule = await import(resolve(pluginPath));
    plugin = pluginModule.default || pluginModule;
    
    if (!plugin || !plugin.name) {
      throw new Error('Invalid plugin: must export a plugin object with name property');
    }
    
    return plugin;
  } catch (error) {
    sendMessage({ type: 'error', message: `Failed to load plugin: ${error.message}` });
    process.exit(1);
  }
}

// Handle incoming messages
async function handleMessage(message) {
  try {
    switch (message.type) {
      case 'init': {
        const logger = createLogger(plugin.name);
        const storage = createStorage(plugin.name);
        const ui = createUI(plugin.name);
        
        pluginContext = {
          storage,
          logger,
          ui,
          config: message.config || {},
        };
        
        if (plugin.onLoad) {
          await plugin.onLoad(pluginContext);
        }
        
        sendMessage({ type: 'loaded' });
        break;
      }
      
      case 'handleRequest': {
        if (!plugin.handleRequest) {
          sendMessage({ type: 'response', request_id: message.request.id, response: null });
          break;
        }
        
        const request = convertRequest(message.request);
        const context = {
          ruleConfig: message.context.rule_config || {},
          matchedPattern: message.context.matched_pattern,
          logger: pluginContext?.logger || createLogger(plugin.name),
        };
        
        try {
          const response = await plugin.handleRequest(request, context);
          sendMessage({ 
            type: 'response', 
            request_id: message.request.id, 
            response: response ? convertResponse(response) : null 
          });
        } catch (error) {
          pluginContext?.logger?.error(`Error handling request: ${error.message}`);
          sendMessage({ type: 'response', request_id: message.request.id, response: null });
        }
        break;
      }
      
      case 'handleResponse': {
        if (!plugin.handleResponse) {
          sendMessage({ 
            type: 'modifiedResponse', 
            request_id: message.request.id, 
            response: message.response 
          });
          break;
        }
        
        const request = convertRequest(message.request);
        const response = convertResponseFromHost(message.response);
        const context = {
          ruleConfig: message.context.rule_config || {},
          matchedPattern: message.context.matched_pattern,
          logger: pluginContext?.logger || createLogger(plugin.name),
        };
        
        try {
          const modified = await plugin.handleResponse(request, response, context);
          sendMessage({ 
            type: 'modifiedResponse', 
            request_id: message.request.id, 
            response: convertResponse(modified) 
          });
        } catch (error) {
          pluginContext?.logger?.error(`Error handling response: ${error.message}`);
          sendMessage({ 
            type: 'modifiedResponse', 
            request_id: message.request.id, 
            response: message.response 
          });
        }
        break;
      }
      
      case 'storageResult': {
        const pending = pendingStorage.get(message.id);
        if (pending) {
          pendingStorage.delete(message.id);
          if (message.result.success) {
            pending.resolve(message.result.value);
          } else {
            pending.reject(new Error(message.result.error || 'Storage operation failed'));
          }
        }
        break;
      }
      
      case 'unload': {
        if (plugin.onUnload) {
          await plugin.onUnload();
        }
        process.exit(0);
        break;
      }
    }
  } catch (error) {
    sendMessage({ type: 'error', message: `Message handling error: ${error.message}` });
  }
}

// Convert host request format to plugin format
function convertRequest(hostRequest) {
  let body = null;
  if (hostRequest.body) {
    if (hostRequest.body_base64) {
      body = Buffer.from(hostRequest.body, 'base64');
    } else {
      body = new TextEncoder().encode(hostRequest.body);
    }
  }
  
  return {
    id: hostRequest.id,
    method: hostRequest.method,
    url: hostRequest.url,
    host: hostRequest.host,
    path: hostRequest.path,
    query: hostRequest.query || {},
    headers: hostRequest.headers || {},
    body,
    timestamp: hostRequest.timestamp,
  };
}

// Convert host response format to plugin format
function convertResponseFromHost(hostResponse) {
  let body = null;
  if (hostResponse.body) {
    if (hostResponse.body_base64) {
      body = Buffer.from(hostResponse.body, 'base64');
    } else {
      body = new TextEncoder().encode(hostResponse.body);
    }
  }
  
  return {
    status: hostResponse.status,
    headers: hostResponse.headers || {},
    body,
  };
}

// Convert plugin response to host format
function convertResponse(pluginResponse) {
  let body = null;
  let bodyBase64 = false;
  
  if (pluginResponse.body) {
    if (pluginResponse.body instanceof Uint8Array || Buffer.isBuffer(pluginResponse.body)) {
      body = Buffer.from(pluginResponse.body).toString('base64');
      bodyBase64 = true;
    } else if (typeof pluginResponse.body === 'string') {
      body = pluginResponse.body;
    }
  }
  
  return {
    status: pluginResponse.status,
    headers: pluginResponse.headers || {},
    body,
    body_base64: bodyBase64,
  };
}

// Main entry point
async function main() {
  // Load the plugin first
  await loadPlugin();
  
  // Set up readline for stdin
  const rl = createInterface({
    input: process.stdin,
    output: process.stdout,
    terminal: false,
  });
  
  rl.on('line', async (line) => {
    if (!line.trim()) return;
    
    try {
      const message = JSON.parse(line);
      await handleMessage(message);
    } catch (error) {
      sendMessage({ type: 'error', message: `Failed to parse message: ${error.message}` });
    }
  });
  
  rl.on('close', () => {
    process.exit(0);
  });
  
  // Handle process signals
  process.on('SIGTERM', async () => {
    if (plugin?.onUnload) {
      await plugin.onUnload();
    }
    process.exit(0);
  });
  
  process.on('SIGINT', async () => {
    if (plugin?.onUnload) {
      await plugin.onUnload();
    }
    process.exit(0);
  });
}

main().catch((error) => {
  console.error('Plugin wrapper error:', error);
  process.exit(1);
});
