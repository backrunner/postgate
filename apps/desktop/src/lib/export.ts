import { CapturedRequest } from '@/stores/capture';

// HAR (HTTP Archive) format types
interface HarEntry {
  startedDateTime: string;
  time: number;
  request: {
    method: string;
    url: string;
    httpVersion: string;
    cookies: Array<{ name: string; value: string }>;
    headers: Array<{ name: string; value: string }>;
    queryString: Array<{ name: string; value: string }>;
    postData?: {
      mimeType: string;
      text: string;
    };
    headersSize: number;
    bodySize: number;
  };
  response: {
    status: number;
    statusText: string;
    httpVersion: string;
    cookies: Array<{ name: string; value: string }>;
    headers: Array<{ name: string; value: string }>;
    content: {
      size: number;
      mimeType: string;
      text?: string;
    };
    redirectURL: string;
    headersSize: number;
    bodySize: number;
  };
  cache: Record<string, never>;
  timings: {
    blocked: number;
    dns: number;
    connect: number;
    send: number;
    wait: number;
    receive: number;
    ssl: number;
  };
}

interface HarLog {
  version: string;
  creator: {
    name: string;
    version: string;
  };
  entries: HarEntry[];
}

interface Har {
  log: HarLog;
}

/**
 * Convert captured requests to HAR format
 */
export function requestsToHar(
  requests: CapturedRequest[],
  requestBodies?: Map<string, Uint8Array>,
  responseBodies?: Map<string, Uint8Array>
): Har {
  const entries: HarEntry[] = requests.map(request => {
    const url = new URL(request.url.startsWith('http') ? request.url : `http://${request.host}${request.url}`);
    
    // Parse query string
    const queryString = Array.from(url.searchParams.entries()).map(([name, value]) => ({
      name,
      value,
    }));

    // Convert headers to HAR format
    const requestHeaders = Object.entries(request.requestHeaders).map(([name, value]) => ({
      name,
      value,
    }));

    const responseHeaders = request.responseHeaders
      ? Object.entries(request.responseHeaders).map(([name, value]) => ({
          name,
          value,
        }))
      : [];

    // Parse cookies from headers
    const requestCookies = parseCookies(request.requestHeaders['cookie'] || '');
    const responseCookies = parseSetCookies(request.responseHeaders?.['set-cookie'] || '');

    // Get body content
    const reqBody = requestBodies?.get(request.id);
    const resBody = responseBodies?.get(request.id);

    const entry: HarEntry = {
      startedDateTime: new Date(request.timestamp).toISOString(),
      time: request.durationMs ?? 0,
      request: {
        method: request.method,
        url: request.url,
        httpVersion: request.protocol.toUpperCase().replace('HTTP', 'HTTP/'),
        cookies: requestCookies,
        headers: requestHeaders,
        queryString,
        headersSize: calculateHeadersSize(request.requestHeaders),
        bodySize: request.requestSize,
      },
      response: {
        status: request.responseStatus ?? 0,
        statusText: getStatusText(request.responseStatus ?? 0),
        httpVersion: request.protocol.toUpperCase().replace('HTTP', 'HTTP/'),
        cookies: responseCookies,
        headers: responseHeaders,
        content: {
          size: request.responseSize ?? 0,
          mimeType: request.contentType || 'application/octet-stream',
        },
        redirectURL: request.responseHeaders?.['location'] || '',
        headersSize: calculateHeadersSize(request.responseHeaders || {}),
        bodySize: request.responseSize ?? 0,
      },
      cache: {},
      timings: {
        blocked: -1,
        dns: -1,
        connect: -1,
        send: -1,
        wait: request.durationMs ?? -1,
        receive: -1,
        ssl: -1,
      },
    };

    // Add request body if available
    if (reqBody && reqBody.length > 0) {
      try {
        entry.request.postData = {
          mimeType: request.requestHeaders['content-type'] || 'application/octet-stream',
          text: new TextDecoder().decode(reqBody),
        };
      } catch {
        // Binary content, skip text
      }
    }

    // Add response body if available
    if (resBody && resBody.length > 0) {
      try {
        entry.response.content.text = new TextDecoder().decode(resBody);
      } catch {
        // Binary content, skip text
      }
    }

    return entry;
  });

  return {
    log: {
      version: '1.2',
      creator: {
        name: 'PostGate',
        version: '0.1.0',
      },
      entries,
    },
  };
}

/**
 * Export requests as HAR file
 */
export function exportToHar(
  requests: CapturedRequest[],
  requestBodies?: Map<string, Uint8Array>,
  responseBodies?: Map<string, Uint8Array>
): void {
  const har = requestsToHar(requests, requestBodies, responseBodies);
  const json = JSON.stringify(har, null, 2);
  const blob = new Blob([json], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  
  const a = document.createElement('a');
  a.href = url;
  a.download = `postgate-export-${new Date().toISOString().split('T')[0]}.har`;
  a.click();
  
  URL.revokeObjectURL(url);
}

/**
 * Convert a request to cURL command
 */
export function requestToCurl(
  request: CapturedRequest,
  requestBody?: Uint8Array
): string {
  const parts: string[] = ['curl'];

  // Method (if not GET)
  if (request.method !== 'GET') {
    parts.push(`-X ${request.method}`);
  }

  // URL
  parts.push(`'${escapeShell(request.url)}'`);

  // Headers
  for (const [name, value] of Object.entries(request.requestHeaders)) {
    // Skip pseudo-headers and host (curl adds it)
    if (name.startsWith(':') || name.toLowerCase() === 'host') continue;
    // Skip content-length (curl calculates it)
    if (name.toLowerCase() === 'content-length') continue;
    
    parts.push(`-H '${escapeShell(name)}: ${escapeShell(value)}'`);
  }

  // Body
  if (requestBody && requestBody.length > 0) {
    try {
      const text = new TextDecoder().decode(requestBody);
      // Check if it's JSON
      const contentType = request.requestHeaders['content-type'] || '';
      if (contentType.includes('json')) {
        parts.push(`-d '${escapeShell(text)}'`);
      } else if (contentType.includes('x-www-form-urlencoded')) {
        parts.push(`--data-urlencode '${escapeShell(text)}'`);
      } else {
        parts.push(`-d '${escapeShell(text)}'`);
      }
    } catch {
      // Binary content
      parts.push('--data-binary @-');
    }
  }

  // Additional flags
  parts.push('-v'); // Verbose
  parts.push('--compressed'); // Accept compression

  return parts.join(' \\\n  ');
}

/**
 * Copy request as cURL to clipboard
 */
export async function copyAsCurl(
  request: CapturedRequest,
  requestBody?: Uint8Array
): Promise<void> {
  const curl = requestToCurl(request, requestBody);
  await navigator.clipboard.writeText(curl);
}

/**
 * Convert a request to fetch() code
 */
export function requestToFetch(
  request: CapturedRequest,
  requestBody?: Uint8Array
): string {
  const options: string[] = [];
  
  // Method
  options.push(`method: '${request.method}'`);

  // Headers
  const headers: Record<string, string> = {};
  for (const [name, value] of Object.entries(request.requestHeaders)) {
    if (name.startsWith(':') || name.toLowerCase() === 'host') continue;
    if (name.toLowerCase() === 'content-length') continue;
    headers[name] = value;
  }
  
  if (Object.keys(headers).length > 0) {
    options.push(`headers: ${JSON.stringify(headers, null, 2).split('\n').join('\n    ')}`);
  }

  // Body
  if (requestBody && requestBody.length > 0) {
    try {
      const text = new TextDecoder().decode(requestBody);
      const contentType = request.requestHeaders['content-type'] || '';
      
      if (contentType.includes('json')) {
        try {
          const json = JSON.parse(text);
          options.push(`body: JSON.stringify(${JSON.stringify(json, null, 2).split('\n').join('\n    ')})`);
        } catch {
          options.push(`body: ${JSON.stringify(text)}`);
        }
      } else {
        options.push(`body: ${JSON.stringify(text)}`);
      }
    } catch {
      options.push(`body: /* binary data */`);
    }
  }

  return `fetch('${request.url}', {
  ${options.join(',\n  ')}
})
  .then(response => response.json())
  .then(data => console.log(data))
  .catch(error => console.error('Error:', error));`;
}

// Helper functions

function escapeShell(str: string): string {
  return str.replace(/'/g, "'\\''");
}

function parseCookies(cookieHeader: string): Array<{ name: string; value: string }> {
  if (!cookieHeader) return [];
  
  return cookieHeader.split(';').map(cookie => {
    const [name, ...valueParts] = cookie.trim().split('=');
    return {
      name: name?.trim() || '',
      value: valueParts.join('=').trim(),
    };
  }).filter(c => c.name);
}

function parseSetCookies(setCookieHeader: string): Array<{ name: string; value: string }> {
  if (!setCookieHeader) return [];
  
  // Set-Cookie headers are typically one per header, but may be combined
  return setCookieHeader.split(/,(?=[^;]*=)/).map(cookie => {
    const [nameValue] = cookie.split(';');
    const [name, ...valueParts] = nameValue.trim().split('=');
    return {
      name: name?.trim() || '',
      value: valueParts.join('=').trim(),
    };
  }).filter(c => c.name);
}

function calculateHeadersSize(headers: Record<string, string>): number {
  let size = 0;
  for (const [name, value] of Object.entries(headers)) {
    size += name.length + value.length + 4; // ": " and "\r\n"
  }
  return size;
}

function getStatusText(status: number): string {
  const statusTexts: Record<number, string> = {
    100: 'Continue',
    101: 'Switching Protocols',
    200: 'OK',
    201: 'Created',
    202: 'Accepted',
    204: 'No Content',
    301: 'Moved Permanently',
    302: 'Found',
    303: 'See Other',
    304: 'Not Modified',
    307: 'Temporary Redirect',
    308: 'Permanent Redirect',
    400: 'Bad Request',
    401: 'Unauthorized',
    403: 'Forbidden',
    404: 'Not Found',
    405: 'Method Not Allowed',
    408: 'Request Timeout',
    409: 'Conflict',
    410: 'Gone',
    413: 'Payload Too Large',
    415: 'Unsupported Media Type',
    429: 'Too Many Requests',
    500: 'Internal Server Error',
    501: 'Not Implemented',
    502: 'Bad Gateway',
    503: 'Service Unavailable',
    504: 'Gateway Timeout',
  };
  
  return statusTexts[status] || 'Unknown';
}
