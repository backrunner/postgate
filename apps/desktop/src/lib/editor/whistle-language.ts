import type { languages } from 'monaco-editor';

export const WHISTLE_LANGUAGE_ID = 'whistle';

// Whistle rule tokenizer
// Line format: SOURCE TARGET [modifiers]
// SOURCE = first token (green), TARGET = second token (orange), modifiers = purple
export const whistleLanguage: languages.IMonarchLanguage = {
  tokenPostfix: '.whistle',
  
  tokenizer: {
    root: [
      // Comment lines
      [/^[ \t]*#.*$/, 'comment'],
      
      // Empty lines
      [/^[ \t]*$/, ''],
      
      // Leading whitespace
      [/^[ \t]+/, ''],
      
      // First token on a line (detected by start of line anchor ^)
      [/^[^\s]+/, 'source', '@afterFirst'],
      
      // This shouldn't normally match in root, but just in case
      [/[^\s]+/, 'source', '@afterFirst'],
    ],
    
    afterFirst: [
      // CRITICAL: Detect start of new line and reset to root
      [/^[ \t]*#.*$/, { token: 'comment', next: '@root' }],
      [/^[ \t]*$/, { token: '', next: '@root' }],
      [/^[ \t]+/, { token: '', next: '@afterFirstToken' }],
      [/^[^\s]+/, { token: 'source', next: '@afterFirst' }],
      
      // Within the same line
      [/[ \t]+/, ''],
      [/#.*$/, 'comment'],
      
      // Filter modifiers (purple)
      [/(?:m|method|i|ip|clientIp|serverIp|p|protocol|h|host|hostname|port|path|pathPrefix|u|url|urlPrefix|s|statusCode|ct|contentType|reqContentType|resContentType|b|body|reqBody|resBody|header|reqHeader|resHeader|excludeFilter|includeFilter):[^\s]+/, 'modifier'],
      
      // Action with :// but NOT http/https (blue)
      [/(?!https?:\/\/)[a-zA-Z][\w-]*:\/\/[^\s]+/, 'action'],
      
      // Any other token is target (orange)
      [/[^\s]+/, 'target'],
    ],
    
    afterFirstToken: [
      // CRITICAL: Detect start of new line and reset to root
      [/^[ \t]*#.*$/, { token: 'comment', next: '@root' }],
      [/^[ \t]*$/, { token: '', next: '@root' }],
      [/^[ \t]+/, { token: '', next: '@afterFirstToken' }],
      [/^[^\s]+/, { token: 'source', next: '@afterFirst' }],
      
      // The actual first token after whitespace
      [/[^\s]+/, 'source', '@afterFirst'],
    ],
  },
};

// Language configuration
export const whistleLanguageConfig: languages.LanguageConfiguration = {
  comments: { lineComment: '#' },
  brackets: [['{', '}'], ['[', ']'], ['(', ')']],
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

// Completions
export interface WhistleCompletionItem {
  label: string;
  kind: number;
  insertText: string;
  insertTextRules: number;
  detail: string;
  documentation: string;
}

export const whistleCompletions: WhistleCompletionItem[] = [
  { label: 'host://', kind: 1, insertText: 'host://${1:127.0.0.1:8080}', insertTextRules: 4, detail: 'Redirect to host', documentation: 'Redirects to a different host' },
  { label: 'file://', kind: 1, insertText: 'file://${1:/path/to/file}', insertTextRules: 4, detail: 'Serve file', documentation: 'Returns content from a local file' },
  { label: 'statusCode://', kind: 1, insertText: 'statusCode://${1:200}', insertTextRules: 4, detail: 'Return status', documentation: 'Returns a specific HTTP status code' },
  { label: 'delay://', kind: 1, insertText: 'delay://${1:1000}', insertTextRules: 4, detail: 'Delay (ms)', documentation: 'Adds a delay' },
  { label: 'm:', kind: 14, insertText: 'm:${1:GET}', insertTextRules: 4, detail: 'Filter by method', documentation: 'Only match specified methods' },
];

// Dark theme colors
export const whistleThemeRules = [
  { token: 'comment', foreground: '6A9955', fontStyle: 'italic' },
  { token: 'source', foreground: '4EC9B0' },
  { token: 'source.wildcard', foreground: '4EC9B0', fontStyle: 'bold' },
  { token: 'source.regex', foreground: 'D16969', fontStyle: 'bold' },
  { token: 'target', foreground: 'CE9178' },
  { token: 'action', foreground: '569CD6', fontStyle: 'bold' },
  { token: 'modifier', foreground: 'C586C0' },
];

// Light theme colors
export const whistleThemeRulesLight = [
  { token: 'comment', foreground: '008000', fontStyle: 'italic' },
  { token: 'source', foreground: '0E7490' },
  { token: 'source.wildcard', foreground: '0E7490', fontStyle: 'bold' },
  { token: 'source.regex', foreground: 'C41A16', fontStyle: 'bold' },
  { token: 'target', foreground: 'A31515' },
  { token: 'action', foreground: '0000FF', fontStyle: 'bold' },
  { token: 'modifier', foreground: 'AF00DB' },
];
