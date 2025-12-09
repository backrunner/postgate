import type { languages } from 'monaco-editor';

export const WHISTLE_LANGUAGE_ID = 'whistle';

// Whistle language token provider
export const whistleLanguage: languages.IMonarchLanguage = {
  defaultToken: '',
  tokenPostfix: '.whistle',
  
  // Pattern keywords
  patterns: [
    'http', 'https', 'ws', 'wss', 'tunnel',
  ],
  
  // Action protocols
  actions: [
    'host', 'file', 'redirect', 'statusCode', 'status',
    'reqHeaders', 'resHeaders', 'reqHeader', 'resHeader',
    'reqBody', 'resBody', 'htmlBody', 'jsBody', 'cssBody',
    'htmlAppend', 'htmlPrepend', 'jsAppend', 'jsPrepend', 'cssAppend', 'cssPrepend',
    'reqDelay', 'resDelay', 'delay', 'reqSpeed', 'resSpeed', 'speed',
    'urlParams', 'pathReplace', 'method', 'ua', 'userAgent', 'referer', 'auth',
    'reqCookies', 'resCookies', 'forwardedFor', 'xff',
    'reqReplace', 'resReplace', 'reqCors', 'resCors', 'cors',
    'resType', 'contentType', 'resCharset', 'charset', 'attachment',
    'proxy', 'http-proxy', 'https-proxy', 'socks', 'socks5', 'socks4',
    'debug', 'weinre', 'log', 'ignore', 'filter', 'enable', 'disable', 'plugin',
    '301', '302', '307', '308',
  ],
  
  // Filter operators
  filters: [
    'm', 'method', 'p', 'protocol', 'port', 'ct', 'contentType',
    'excludeFilter', 'includeFilter',
  ],
  
  tokenizer: {
    root: [
      // Comments
      [/#.*$/, 'comment'],
      [/\/\/.*$/, 'comment'],
      
      // Regex pattern (starts with ^)
      [/\^[^\s]+/, 'regexp'],
      
      // Regex pattern (enclosed in /)
      [/\/[^\/]+\/[gimsuy]*/, 'regexp'],
      
      // Filter operators (m:GET, p:https, port:443)
      [/(m|method|p|protocol|port|ct|contentType|excludeFilter|includeFilter):/, {
        token: 'keyword.filter',
        next: '@filterValue',
      }],
      
      // Action protocols (host://, file://, etc.)
      [/([a-zA-Z0-9\-]+):\/\//, {
        cases: {
          '$1@actions': { token: 'keyword.action', next: '@actionValue' },
          '$1@patterns': { token: 'keyword.protocol', next: '@root' },
          '@default': { token: 'string', next: '@actionValue' },
        },
      }],
      
      // HTTP status codes as protocol (301://, 302://)
      [/(301|302|307|308):\/\//, { token: 'keyword.action', next: '@actionValue' }],
      
      // URLs (http://, https://)
      [/(https?|wss?):\/\/[^\s]+/, 'string.url'],
      
      // Wildcard patterns
      [/\*+/, 'operator.wildcard'],
      [/\?/, 'operator.wildcard'],
      
      // IP addresses
      [/\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}(:\d+)?\b/, 'number.ip'],
      
      // Port numbers (standalone :port)
      [/:\d+/, 'number.port'],
      
      // Domain patterns
      [/[a-zA-Z0-9]([a-zA-Z0-9\-]*[a-zA-Z0-9])?(\.[a-zA-Z0-9]([a-zA-Z0-9\-]*[a-zA-Z0-9])?)+/, 'string.domain'],
      
      // Path patterns
      [/\/[a-zA-Z0-9_\-\.\/\*\?]+/, 'string.path'],
      
      // JSON objects in action values
      [/\{/, { token: 'delimiter.bracket', next: '@json' }],
      
      // Whitespace
      [/\s+/, 'white'],
    ],
    
    filterValue: [
      [/[A-Z,]+/, 'string.filter-value'],
      [/[a-z,\/]+/, 'string.filter-value'],
      [/\d+/, 'number'],
      [/\s/, { token: 'white', next: '@root' }],
      [/$/, { token: '', next: '@root' }],
    ],
    
    actionValue: [
      // JSON object
      [/\{/, { token: 'delimiter.bracket', next: '@json' }],
      // File path
      [/\/[^\s]+/, 'string.path'],
      // Number (for delays, speeds, status codes)
      [/\d+/, 'number'],
      // General value until whitespace
      [/[^\s\{]+/, 'string.value'],
      // End of value
      [/\s/, { token: 'white', next: '@root' }],
      [/$/, { token: '', next: '@root' }],
    ],
    
    json: [
      [/[{}]/, 'delimiter.bracket'],
      [/[\[\]]/, 'delimiter.array'],
      [/"([^"\\]|\\.)*"/, 'string.json'],
      [/'([^'\\]|\\.)*'/, 'string.json'],
      [/[a-zA-Z_]\w*/, 'variable.json'],
      [/:/, 'delimiter.colon'],
      [/,/, 'delimiter.comma'],
      [/\d+(\.\d+)?/, 'number.json'],
      [/true|false|null/, 'keyword.json'],
      [/\}/, { token: 'delimiter.bracket', next: '@pop' }],
      [/\s+/, 'white'],
    ],
  },
};

// Whistle language configuration
export const whistleLanguageConfig: languages.LanguageConfiguration = {
  comments: {
    lineComment: '#',
  },
  brackets: [
    ['{', '}'],
    ['[', ']'],
    ['(', ')'],
  ],
  autoClosingPairs: [
    { open: '{', close: '}' },
    { open: '[', close: ']' },
    { open: '(', close: ')' },
    { open: '"', close: '"' },
    { open: "'", close: "'" },
  ],
  surroundingPairs: [
    { open: '{', close: '}' },
    { open: '[', close: ']' },
    { open: '(', close: ')' },
    { open: '"', close: '"' },
    { open: "'", close: "'" },
  ],
};

// Whistle completions (range is added at runtime)
export interface WhistleCompletionItem {
  label: string;
  kind: number;
  insertText: string;
  insertTextRules: number;
  detail: string;
  documentation: string;
}

export const whistleCompletions: WhistleCompletionItem[] = [
  // Actions
  { label: 'host://', kind: 1, insertText: 'host://${1:127.0.0.1:8080}', insertTextRules: 4, detail: 'Redirect to different host', documentation: 'Redirects the request to a different host. Example: example.com host://127.0.0.1:8080' },
  { label: 'file://', kind: 1, insertText: 'file://${1:/path/to/file}', insertTextRules: 4, detail: 'Serve local file', documentation: 'Returns content from a local file' },
  { label: 'redirect://', kind: 1, insertText: 'redirect://${1:https://example.com}', insertTextRules: 4, detail: 'HTTP 302 redirect', documentation: 'Returns a 302 redirect response' },
  { label: 'statusCode://', kind: 1, insertText: 'statusCode://${1:200}', insertTextRules: 4, detail: 'Return status code', documentation: 'Returns a specific HTTP status code' },
  { label: 'reqHeaders://', kind: 1, insertText: 'reqHeaders://{"${1:header}": "${2:value}"}', insertTextRules: 4, detail: 'Modify request headers', documentation: 'Adds or modifies request headers' },
  { label: 'resHeaders://', kind: 1, insertText: 'resHeaders://{"${1:header}": "${2:value}"}', insertTextRules: 4, detail: 'Modify response headers', documentation: 'Adds or modifies response headers' },
  { label: 'reqBody://', kind: 1, insertText: 'reqBody://${1:content}', insertTextRules: 4, detail: 'Replace request body', documentation: 'Replaces the request body' },
  { label: 'resBody://', kind: 1, insertText: 'resBody://${1:content}', insertTextRules: 4, detail: 'Replace response body', documentation: 'Replaces the response body' },
  { label: 'reqDelay://', kind: 1, insertText: 'reqDelay://${1:1000}', insertTextRules: 4, detail: 'Delay request (ms)', documentation: 'Adds a delay before forwarding the request' },
  { label: 'resDelay://', kind: 1, insertText: 'resDelay://${1:1000}', insertTextRules: 4, detail: 'Delay response (ms)', documentation: 'Adds a delay before returning the response' },
  { label: 'reqSpeed://', kind: 1, insertText: 'reqSpeed://${1:100}', insertTextRules: 4, detail: 'Throttle request (kbps)', documentation: 'Limits request upload speed in kbps' },
  { label: 'resSpeed://', kind: 1, insertText: 'resSpeed://${1:100}', insertTextRules: 4, detail: 'Throttle response (kbps)', documentation: 'Limits response download speed in kbps' },
  { label: 'resCors://', kind: 1, insertText: 'resCors://*', insertTextRules: 4, detail: 'Add CORS headers', documentation: 'Adds CORS headers to allow cross-origin requests' },
  { label: 'htmlAppend://', kind: 1, insertText: 'htmlAppend://${1:<script>console.log("injected")</script>}', insertTextRules: 4, detail: 'Append to HTML', documentation: 'Appends content to HTML responses before </body>' },
  { label: 'htmlPrepend://', kind: 1, insertText: 'htmlPrepend://${1:<meta charset="utf-8">}', insertTextRules: 4, detail: 'Prepend to HTML', documentation: 'Prepends content to HTML responses after <head>' },
  { label: 'jsAppend://', kind: 1, insertText: 'jsAppend://${1:console.log("appended")}', insertTextRules: 4, detail: 'Append JavaScript', documentation: 'Appends JavaScript to HTML responses' },
  { label: 'cssAppend://', kind: 1, insertText: 'cssAppend://${1:body { background: red; \\}}', insertTextRules: 4, detail: 'Append CSS', documentation: 'Appends CSS styles to HTML responses' },
  { label: 'proxy://', kind: 1, insertText: 'proxy://${1:127.0.0.1:8888}', insertTextRules: 4, detail: 'Use upstream proxy', documentation: 'Routes request through an upstream HTTP proxy' },
  { label: 'socks://', kind: 1, insertText: 'socks://${1:127.0.0.1:1080}', insertTextRules: 4, detail: 'Use SOCKS5 proxy', documentation: 'Routes request through a SOCKS5 proxy' },
  { label: 'urlParams://', kind: 1, insertText: 'urlParams://{"${1:key}": "${2:value}"}', insertTextRules: 4, detail: 'Modify URL params', documentation: 'Adds, modifies, or removes URL query parameters' },
  { label: 'method://', kind: 1, insertText: 'method://${1:POST}', insertTextRules: 4, detail: 'Change HTTP method', documentation: 'Changes the HTTP method of the request' },
  { label: 'ua://', kind: 1, insertText: 'ua://${1:Mozilla/5.0}', insertTextRules: 4, detail: 'Set User-Agent', documentation: 'Sets the User-Agent header' },
  { label: 'auth://', kind: 1, insertText: 'auth://${1:user}:${2:pass}', insertTextRules: 4, detail: 'Set Basic Auth', documentation: 'Adds Basic Authentication header' },
  { label: 'log://', kind: 1, insertText: 'log://${1:message}', insertTextRules: 4, detail: 'Log request', documentation: 'Logs the request with a custom message' },
  { label: 'ignore://', kind: 1, insertText: 'ignore://', insertTextRules: 4, detail: 'Ignore request', documentation: 'Skips rule processing for this request' },
  
  // Filters
  { label: 'm:', kind: 14, insertText: 'm:${1:GET,POST}', insertTextRules: 4, detail: 'Filter by method', documentation: 'Only match requests with specified HTTP methods' },
  { label: 'p:', kind: 14, insertText: 'p:${1:https}', insertTextRules: 4, detail: 'Filter by protocol', documentation: 'Only match requests with specified protocols' },
  { label: 'port:', kind: 14, insertText: 'port:${1:443}', insertTextRules: 4, detail: 'Filter by port', documentation: 'Only match requests to specified ports' },
  { label: 'excludeFilter://', kind: 14, insertText: 'excludeFilter://${1:pattern}', insertTextRules: 4, detail: 'Exclude pattern', documentation: 'Exclude URLs matching this pattern' },
  { label: 'includeFilter://', kind: 14, insertText: 'includeFilter://${1:pattern}', insertTextRules: 4, detail: 'Include pattern', documentation: 'Only include URLs matching this pattern' },
];

// Theme colors for whistle syntax
export const whistleThemeRules = [
  { token: 'comment', foreground: '6A9955' },
  { token: 'regexp', foreground: 'D16969' },
  { token: 'keyword.filter', foreground: 'C586C0' },
  { token: 'keyword.action', foreground: '569CD6' },
  { token: 'keyword.protocol', foreground: '4EC9B0' },
  { token: 'keyword.json', foreground: '569CD6' },
  { token: 'string', foreground: 'CE9178' },
  { token: 'string.url', foreground: '4FC1FF' },
  { token: 'string.domain', foreground: 'DCDCAA' },
  { token: 'string.path', foreground: 'CE9178' },
  { token: 'string.value', foreground: 'CE9178' },
  { token: 'string.filter-value', foreground: 'B5CEA8' },
  { token: 'string.json', foreground: 'CE9178' },
  { token: 'number', foreground: 'B5CEA8' },
  { token: 'number.ip', foreground: 'B5CEA8' },
  { token: 'number.port', foreground: 'B5CEA8' },
  { token: 'number.json', foreground: 'B5CEA8' },
  { token: 'operator.wildcard', foreground: 'D4D4D4', fontStyle: 'bold' },
  { token: 'variable.json', foreground: '9CDCFE' },
  { token: 'delimiter', foreground: 'D4D4D4' },
];
