import { useMemo, useState, useCallback, useEffect } from 'react';
import { Copy, Download, Image, FileText, Code, Eye, EyeOff, ChevronRight, ChevronDown, AlertTriangle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { cn, formatBytes } from '@/lib/utils';

interface BodyPreviewProps {
  body: Uint8Array | null;
  contentType: string | null;
  loading?: boolean;
  className?: string;
}

type ViewMode = 'pretty' | 'raw' | 'preview' | 'hex';

/**
 * Size ceilings for body rendering. Anything above these falls back to a
 * "too large — download instead" placeholder so the UI thread doesn't lock
 * up decoding / syntax-highlighting / JSON-parsing megabytes of text.
 *
 * - `TEXT_DECODE_LIMIT`: above this we don't even call TextDecoder.
 * - `JSON_PARSE_LIMIT`: above this we don't JSON.parse (tree view is skipped
 *   and the user sees a truncated Raw view instead).
 * - `HIGHLIGHT_LIMIT`: above this we skip syntax highlighting and show a
 *   truncated plain-text view.
 * - `RENDER_LIMIT`: hard truncation point used by the plain-text view.
 */
const TEXT_DECODE_LIMIT = 10 * 1024 * 1024; // 10 MB
const JSON_PARSE_LIMIT = 2 * 1024 * 1024; // 2 MB
const HIGHLIGHT_LIMIT = 512 * 1024; // 512 KB
const RENDER_LIMIT = 100 * 1024; // 100 KB visible at a time

export function BodyPreview({ body, contentType, loading, className }: BodyPreviewProps) {
  const [viewMode, setViewMode] = useState<ViewMode>('pretty');
  const [wordWrap, setWordWrap] = useState(true);
  /** User override: render anyway even if the body is above the decode limit. */
  const [forceRender, setForceRender] = useState(false);

  useEffect(() => {
    setViewMode('pretty');
    setForceRender(false);
  }, [body, contentType]);

  const contentInfo = useMemo(() => {
    if (!body || body.length === 0) {
      return { type: 'empty' as const, text: null, parsed: null };
    }

    const normalizedContentType = contentType?.toLowerCase() || '';
    const isImage = normalizedContentType.startsWith('image/');
    const isJson = normalizedContentType.includes('json');
    const isHtml = normalizedContentType.includes('html');
    const isXml = normalizedContentType.includes('xml');
    const isCss = normalizedContentType.includes('css');
    const isJs = normalizedContentType.includes('javascript') || normalizedContentType.includes('ecmascript');
    const isText = normalizedContentType.startsWith('text/') || isJson || isHtml || isXml || isCss || isJs;

    // Too large to decode — bail out before touching the main thread.
    const tooLargeToDecode = body.length > TEXT_DECODE_LIMIT && !forceRender;
    if (tooLargeToDecode) {
      if (isImage) {
        return { type: 'image' as const, text: null, parsed: null, mimeType: contentType };
      }
      return { type: 'too-large' as const, text: null, parsed: null };
    }

    // Try to decode as text
    let text: string | null = null;
    let parsed: unknown = null;
    let jsonParsed = false;

    if (isText || !contentType) {
      try {
        text = new TextDecoder().decode(body);

        // Only JSON-parse if under the parse budget — big JSON blobs will
        // be shown as raw text and the Pretty tree view is skipped.
        const jsonBudget = body.length <= JSON_PARSE_LIMIT;
        const trimmed = text.trim();
        const looksLikeJson = trimmed.startsWith('{') || trimmed.startsWith('[');
        if (jsonBudget && (isJson || looksLikeJson)) {
          try {
            parsed = JSON.parse(text);
            jsonParsed = true;
          } catch {
            // Not valid JSON
          }
        }
      } catch {
        // Binary content
      }
    }

    if (isImage) {
      return { type: 'image' as const, text: null, parsed: null, mimeType: contentType };
    }

    if (jsonParsed) {
      return { type: 'json' as const, text, parsed, jsonParsed: true as const };
    }

    if (isJson) {
      return { type: 'json' as const, text, parsed: null, jsonParsed: false as const };
    }

    if (isHtml) {
      return { type: 'html' as const, text, parsed: null };
    }

    if (isXml) {
      return { type: 'xml' as const, text, parsed: null };
    }

    if (isCss) {
      return { type: 'css' as const, text, parsed: null };
    }

    if (isJs) {
      return { type: 'javascript' as const, text, parsed: null };
    }

    if (text) {
      return { type: 'text' as const, text, parsed: null };
    }

    return { type: 'binary' as const, text: null, parsed: null };
  }, [body, contentType, forceRender]);

  const activeViewMode = viewMode === 'preview' && contentInfo.type !== 'html'
    ? 'pretty'
    : viewMode;

  const copyToClipboard = () => {
    if (contentInfo.text) {
      navigator.clipboard.writeText(contentInfo.text);
    }
  };

  const downloadBody = () => {
    if (!body) return;
    
    const blob = new Blob([new Uint8Array(body)], { type: contentType || 'application/octet-stream' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `body.${getExtension(contentType)}`;
    a.click();
    URL.revokeObjectURL(url);
  };

  if (loading) {
    return (
      <div className={cn('flex items-center justify-center p-8 text-muted-foreground', className)}>
        <div className="animate-pulse">Loading...</div>
      </div>
    );
  }

  if (!body || body.length === 0) {
    return (
      <div className={cn('flex items-center justify-center p-8 text-muted-foreground italic', className)}>
        No body content
      </div>
    );
  }

  return (
    <div className={cn('flex flex-col', className)}>
      {/* Toolbar */}
      <div className="flex items-center justify-between px-3 py-1.5 border-b bg-muted/30">
        <div className="flex items-center gap-1">
          <span className="text-xs text-muted-foreground">
            {formatBytes(body.length)}
          </span>
          {contentType && (
            <>
              <span className="text-xs text-muted-foreground">•</span>
              <span className="text-xs text-muted-foreground truncate max-w-[200px]">
                {contentType}
              </span>
            </>
          )}
        </div>
        <div className="flex items-center gap-1">
          <Button
            variant="ghost"
            size="icon"
            className="h-6 w-6"
            onClick={() => setWordWrap(!wordWrap)}
            title={wordWrap ? 'Disable word wrap' : 'Enable word wrap'}
          >
            {wordWrap ? <EyeOff className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-6 w-6"
            onClick={copyToClipboard}
            disabled={!contentInfo.text}
            title="Copy to clipboard"
          >
            <Copy className="h-3.5 w-3.5" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-6 w-6"
            onClick={downloadBody}
            title="Download"
          >
            <Download className="h-3.5 w-3.5" />
          </Button>
        </div>
      </div>

      {/* Content */}
      {contentInfo.type === 'too-large' ? (
        <TooLargeNotice
          size={body.length}
          onDownload={downloadBody}
          onForceRender={() => setForceRender(true)}
        />
      ) : contentInfo.type === 'image' ? (
        <ImagePreview body={body} mimeType={contentInfo.mimeType || undefined} />
      ) : contentInfo.type === 'binary' ? (
        <BinaryPreview body={body} />
      ) : (
        <Tabs value={activeViewMode} onValueChange={(v) => setViewMode(v as ViewMode)} className="flex-1 flex flex-col">
          <TabsList className="mx-3 mt-2 w-fit">
            <TabsTrigger value="pretty" className="text-xs px-2">
              <Code className="h-3 w-3 mr-1" />
              Pretty
            </TabsTrigger>
            <TabsTrigger value="raw" className="text-xs px-2">
              <FileText className="h-3 w-3 mr-1" />
              Raw
            </TabsTrigger>
            {contentInfo.type === 'html' && (
              <TabsTrigger value="preview" className="text-xs px-2">
                <Eye className="h-3 w-3 mr-1" />
                Preview
              </TabsTrigger>
            )}
          </TabsList>

          <TabsContent value="pretty" className="flex-1 overflow-auto mt-2 mx-3 mb-3">
            {contentInfo.type === 'json' && contentInfo.jsonParsed ? (
              <JsonTreeView data={contentInfo.parsed} />
            ) : contentInfo.type === 'json' ? (
              <JsonTextHighlight code={contentInfo.text || ''} wordWrap={wordWrap} />
            ) : contentInfo.type === 'html' ? (
              <HtmlHighlight code={contentInfo.text || ''} wordWrap={wordWrap} />
            ) : contentInfo.type === 'xml' ? (
              <XmlHighlight code={contentInfo.text || ''} wordWrap={wordWrap} />
            ) : contentInfo.type === 'css' ? (
              <CssHighlight code={contentInfo.text || ''} wordWrap={wordWrap} />
            ) : contentInfo.type === 'javascript' ? (
              <JsHighlight code={contentInfo.text || ''} wordWrap={wordWrap} />
            ) : (
              <PlainText text={contentInfo.text || ''} wordWrap={wordWrap} />
            )}
          </TabsContent>

          <TabsContent value="raw" className="flex-1 overflow-auto mt-2 mx-3 mb-3">
            <PlainText text={contentInfo.text || ''} wordWrap={wordWrap} />
          </TabsContent>

          {contentInfo.type === 'html' && (
            <TabsContent value="preview" className="flex-1 overflow-auto mt-2 mx-3 mb-3">
              <HtmlPreview html={contentInfo.text || ''} />
            </TabsContent>
          )}
        </Tabs>
      )}
    </div>
  );
}

// JSON Tree View component with expandable/collapsible nodes
interface JsonTreeViewProps {
  data: unknown;
}

function JsonTreeView({ data }: JsonTreeViewProps) {
  return (
    <div className="text-xs font-mono p-3 bg-muted/30 rounded overflow-auto">
      <JsonNode value={data} name={null} depth={0} defaultExpanded />
    </div>
  );
}

interface JsonNodeProps {
  value: unknown;
  name: string | null;
  depth: number;
  defaultExpanded?: boolean;
}

function JsonNode({ value, name, depth, defaultExpanded = false }: JsonNodeProps) {
  // Auto-expand first 2 levels by default
  const [expanded, setExpanded] = useState(defaultExpanded || depth < 2);
  
  const toggleExpand = useCallback(() => {
    setExpanded(prev => !prev);
  }, []);

  const indent = depth * 16;

  // Render null
  if (value === null) {
    return (
      <div className="flex items-center" style={{ paddingLeft: indent }}>
        {name !== null && (
          <>
            <span className="text-purple-600 dark:text-purple-400">&quot;{name}&quot;</span>
            <span className="text-muted-foreground mx-1">:</span>
          </>
        )}
        <span className="text-orange-500">null</span>
      </div>
    );
  }

  // Render boolean
  if (typeof value === 'boolean') {
    return (
      <div className="flex items-center" style={{ paddingLeft: indent }}>
        {name !== null && (
          <>
            <span className="text-purple-600 dark:text-purple-400">&quot;{name}&quot;</span>
            <span className="text-muted-foreground mx-1">:</span>
          </>
        )}
        <span className="text-orange-500">{String(value)}</span>
      </div>
    );
  }

  // Render number
  if (typeof value === 'number') {
    return (
      <div className="flex items-center" style={{ paddingLeft: indent }}>
        {name !== null && (
          <>
            <span className="text-purple-600 dark:text-purple-400">&quot;{name}&quot;</span>
            <span className="text-muted-foreground mx-1">:</span>
          </>
        )}
        <span className="text-emerald-600 dark:text-emerald-400">{value}</span>
      </div>
    );
  }

  // Render string
  if (typeof value === 'string') {
    // Check if it looks like a URL
    const isUrl = value.match(/^https?:\/\//);
    return (
      <div className="flex items-start" style={{ paddingLeft: indent }}>
        {name !== null && (
          <>
            <span className="text-purple-600 dark:text-purple-400 shrink-0">&quot;{name}&quot;</span>
            <span className="text-muted-foreground mx-1 shrink-0">:</span>
          </>
        )}
        <span className={cn(
          "break-all",
          isUrl ? "text-sky-600 dark:text-sky-400" : "text-amber-600 dark:text-amber-400"
        )}>
          &quot;{escapeString(value)}&quot;
        </span>
      </div>
    );
  }

  // Render array
  if (Array.isArray(value)) {
    const isEmpty = value.length === 0;
    
    if (isEmpty) {
      return (
        <div className="flex items-center" style={{ paddingLeft: indent }}>
          {name !== null && (
            <>
              <span className="text-purple-600 dark:text-purple-400">&quot;{name}&quot;</span>
              <span className="text-muted-foreground mx-1">:</span>
            </>
          )}
          <span className="text-muted-foreground">[]</span>
        </div>
      );
    }

    return (
      <div>
        <div 
          className="flex items-center cursor-pointer hover:bg-muted/50 rounded -ml-4 pl-4"
          style={{ paddingLeft: indent }}
          onClick={toggleExpand}
        >
          <span className="w-4 h-4 flex items-center justify-center mr-1 text-muted-foreground">
            {expanded ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
          </span>
          {name !== null && (
            <>
              <span className="text-purple-600 dark:text-purple-400">&quot;{name}&quot;</span>
              <span className="text-muted-foreground mx-1">:</span>
            </>
          )}
          <span className="text-muted-foreground">[</span>
          {!expanded && (
            <span className="text-muted-foreground ml-1">
              {value.length} {value.length === 1 ? 'item' : 'items'}
            </span>
          )}
          {!expanded && <span className="text-muted-foreground">]</span>}
        </div>
        {expanded && (
          <>
            {value.map((item, index) => (
              <div key={index} className="relative">
                <JsonNode value={item} name={null} depth={depth + 1} />
                {index < value.length - 1 && (
                  <span className="text-muted-foreground" style={{ paddingLeft: (depth + 1) * 16 }}>,</span>
                )}
              </div>
            ))}
            <div style={{ paddingLeft: indent }}>
              <span className="text-muted-foreground">]</span>
            </div>
          </>
        )}
      </div>
    );
  }

  // Render object
  if (typeof value === 'object') {
    const entries = Object.entries(value as Record<string, unknown>);
    const isEmpty = entries.length === 0;
    
    if (isEmpty) {
      return (
        <div className="flex items-center" style={{ paddingLeft: indent }}>
          {name !== null && (
            <>
              <span className="text-purple-600 dark:text-purple-400">&quot;{name}&quot;</span>
              <span className="text-muted-foreground mx-1">:</span>
            </>
          )}
          <span className="text-muted-foreground">{'{}'}</span>
        </div>
      );
    }

    return (
      <div>
        <div 
          className="flex items-center cursor-pointer hover:bg-muted/50 rounded -ml-4 pl-4"
          style={{ paddingLeft: indent }}
          onClick={toggleExpand}
        >
          <span className="w-4 h-4 flex items-center justify-center mr-1 text-muted-foreground">
            {expanded ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
          </span>
          {name !== null && (
            <>
              <span className="text-purple-600 dark:text-purple-400">&quot;{name}&quot;</span>
              <span className="text-muted-foreground mx-1">:</span>
            </>
          )}
          <span className="text-muted-foreground">{'{'}</span>
          {!expanded && (
            <span className="text-muted-foreground ml-1">
              {entries.length} {entries.length === 1 ? 'key' : 'keys'}
            </span>
          )}
          {!expanded && <span className="text-muted-foreground">{'}'}</span>}
        </div>
        {expanded && (
          <>
            {entries.map(([key, val], index) => (
              <div key={key}>
                <JsonNode value={val} name={key} depth={depth + 1} />
                {index < entries.length - 1 && (
                  <span className="text-muted-foreground">,</span>
                )}
              </div>
            ))}
            <div style={{ paddingLeft: indent }}>
              <span className="text-muted-foreground">{'}'}</span>
            </div>
          </>
        )}
      </div>
    );
  }

  // Fallback
  return (
    <div style={{ paddingLeft: indent }}>
      {name !== null && (
        <>
          <span className="text-purple-600 dark:text-purple-400">&quot;{name}&quot;</span>
          <span className="text-muted-foreground mx-1">:</span>
        </>
      )}
      <span>{String(value)}</span>
    </div>
  );
}

// Helper to escape string for display
function escapeString(str: string): string {
  return str
    .replace(/\\/g, '\\\\')
    .replace(/"/g, '\\"')
    .replace(/\n/g, '\\n')
    .replace(/\r/g, '\\r')
    .replace(/\t/g, '\\t');
}

interface SyntaxToken {
  text: string;
  className?: string;
}

function tokenizeSyntax(
  source: string,
  pattern: RegExp,
  classify: (token: string, index: number, source: string) => string | undefined,
): SyntaxToken[] {
  const tokens: SyntaxToken[] = [];
  let cursor = 0;
  pattern.lastIndex = 0;
  for (const match of source.matchAll(pattern)) {
    const index = match.index ?? 0;
    if (index > cursor) tokens.push({ text: source.slice(cursor, index) });
    tokens.push({ text: match[0], className: classify(match[0], index, source) });
    cursor = index + match[0].length;
  }
  if (cursor < source.length) tokens.push({ text: source.slice(cursor) });
  return tokens;
}

function HighlightedCode({
  code,
  wordWrap,
  tokens,
  truncated,
}: {
  code: string;
  wordWrap: boolean;
  tokens: SyntaxToken[];
  truncated: boolean;
}) {
  return (
    <pre className={cn(
      'text-xs font-mono p-3 bg-muted/30 rounded overflow-auto',
      wordWrap && 'whitespace-pre-wrap break-all'
    )}>
      <code>
        {tokens.map((token, index) => token.className ? (
          <span key={index} className={token.className}>{token.text}</span>
        ) : token.text)}
      </code>
      {truncated && <TruncationMarker originalSize={code.length} />}
    </pre>
  );
}

// HTML/XML tokenization emits React text nodes instead of rewriting generated
// markup with regexes. This keeps captured source inert and avoids corrupting
// the highlighter's own span attributes.
function HtmlHighlight({ code, wordWrap }: { code: string; wordWrap: boolean }) {
  const { slice, truncated } = truncateForHighlight(code);
  const tokens = useMemo(() => tokenizeSyntax(
    slice,
    /<!--[\s\S]*?-->|<!DOCTYPE[^>]*>|<\/?[\w:-]+|\/?>|[\w:-]+(?=\s*=)|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'/gi,
    (token) => {
      if (token.startsWith('<!--')) return 'text-zinc-500';
      if (token.toLowerCase().startsWith('<!doctype')) return 'text-zinc-500';
      if (/^<\/?/.test(token)) return 'text-rose-500';
      if (/^["']/.test(token)) return 'text-emerald-500';
      if (/^[\w:-]+$/.test(token)) return 'text-amber-500';
      return 'text-muted-foreground';
    },
  ), [slice]);

  return <HighlightedCode code={code} wordWrap={wordWrap} tokens={tokens} truncated={truncated} />;
}

function XmlHighlight({ code, wordWrap }: { code: string; wordWrap: boolean }) {
  return <HtmlHighlight code={code} wordWrap={wordWrap} />;
}

function CssHighlight({ code, wordWrap }: { code: string; wordWrap: boolean }) {
  const { slice, truncated } = truncateForHighlight(code);
  const tokens = useMemo(() => tokenizeSyntax(
    slice,
    /\/\*[\s\S]*?\*\/|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|#[0-9a-f]{3,8}\b|@[\w-]+|\b-?\d+(?:\.\d+)?(?:%|[a-z]+)?\b|--?[\w-]+(?=\s*:)|[\w-]+(?=\s*:)/gi,
    (token) => {
      if (token.startsWith('/*')) return 'text-zinc-500';
      if (/^["']/.test(token)) return 'text-emerald-500';
      if (token.startsWith('@')) return 'text-purple-500';
      if (token.startsWith('#')) return 'text-rose-500';
      if (/^-?\d/.test(token)) return 'text-amber-500';
      return 'text-sky-500';
    },
  ), [slice]);

  return <HighlightedCode code={code} wordWrap={wordWrap} tokens={tokens} truncated={truncated} />;
}

function JsHighlight({ code, wordWrap }: { code: string; wordWrap: boolean }) {
  const { slice, truncated } = truncateForHighlight(code);
  const tokens = useMemo(() => tokenizeSyntax(
    slice,
    /\/\*[\s\S]*?\*\/|\/\/[^\n]*|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|`(?:\\.|[^`\\])*`|\b(?:const|let|var|function|return|if|else|for|while|class|extends|import|export|from|async|await|try|catch|throw|new|this|true|false|null|undefined)\b|\b\d+(?:\.\d+)?\b/g,
    (token) => {
      if (token.startsWith('//') || token.startsWith('/*')) return 'text-zinc-500';
      if (/^["'`]/.test(token)) return 'text-emerald-500';
      if (/^\d/.test(token)) return 'text-amber-500';
      return 'text-purple-500';
    },
  ), [slice]);

  return <HighlightedCode code={code} wordWrap={wordWrap} tokens={tokens} truncated={truncated} />;
}

function JsonTextHighlight({ code, wordWrap }: { code: string; wordWrap: boolean }) {
  const { slice, truncated } = truncateForHighlight(code);
  const tokens = useMemo(() => tokenizeSyntax(
    slice,
    /"(?:\\.|[^"\\])*"(?=\s*:)|"(?:\\.|[^"\\])*"|-?\b\d+(?:\.\d+)?(?:e[+-]?\d+)?\b|\b(?:true|false|null)\b/gi,
    (token, index, source) => {
      if (token.startsWith('"')) {
        return /^\s*:/.test(source.slice(index + token.length))
          ? 'text-purple-600 dark:text-purple-400'
          : 'text-amber-600 dark:text-amber-400';
      }
      if (token === 'true' || token === 'false' || token === 'null') return 'text-orange-500';
      return 'text-emerald-600 dark:text-emerald-400';
    },
  ), [slice]);

  return <HighlightedCode code={code} wordWrap={wordWrap} tokens={tokens} truncated={truncated} />;
}

// Plain text display
function PlainText({ text, wordWrap }: { text: string; wordWrap: boolean }) {
  const truncated = text.length > RENDER_LIMIT;
  const displayText = truncated ? text.slice(0, RENDER_LIMIT) : text;

  return (
    <pre className={cn(
      'text-xs font-mono p-3 bg-muted/30 rounded overflow-auto',
      wordWrap && 'whitespace-pre-wrap break-all'
    )}>
      {displayText}
      {truncated && <TruncationMarker originalSize={text.length} />}
    </pre>
  );
}

// Image preview
function ImagePreview({ body, mimeType }: { body: Uint8Array; mimeType?: string }) {
  const url = useMemo(() => {
    const blob = new Blob([new Uint8Array(body)], { type: mimeType || 'image/png' });
    return URL.createObjectURL(blob);
  }, [body, mimeType]);

  // Revoke the blob URL whenever it changes or the component unmounts.
  // Without this, rapidly switching between captured responses (which
  // re-mounts ImagePreview with a new `url` from the memo above) leaks one
  // Blob per switch — fine for a few images, measurable for a debugging
  // session with hundreds of image responses.
  useEffect(() => {
    return () => {
      URL.revokeObjectURL(url);
    };
  }, [url]);

  return (
    <div className="flex-1 flex items-center justify-center p-4 bg-[url('data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iMjAiIGhlaWdodD0iMjAiIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyI+PGRlZnM+PHBhdHRlcm4gaWQ9ImNoZWNrZXIiIHdpZHRoPSIyMCIgaGVpZ2h0PSIyMCIgcGF0dGVyblVuaXRzPSJ1c2VyU3BhY2VPblVzZSI+PHJlY3QgZmlsbD0iI2YwZjBmMCIgd2lkdGg9IjEwIiBoZWlnaHQ9IjEwIi8+PHJlY3QgZmlsbD0iI2UwZTBlMCIgeD0iMTAiIHdpZHRoPSIxMCIgaGVpZ2h0PSIxMCIvPjxyZWN0IGZpbGw9IiNlMGUwZTAiIHk9IjEwIiB3aWR0aD0iMTAiIGhlaWdodD0iMTAiLz48cmVjdCBmaWxsPSIjZjBmMGYwIiB4PSIxMCIgeT0iMTAiIHdpZHRoPSIxMCIgaGVpZ2h0PSIxMCIvPjwvcGF0dGVybj48L2RlZnM+PHJlY3QgZmlsbD0idXJsKCNjaGVja2VyKSIgd2lkdGg9IjEwMCUiIGhlaWdodD0iMTAwJSIvPjwvc3ZnPg==')]">
      <img
        src={url}
        alt="Response preview"
        className="max-w-full max-h-[400px] object-contain shadow-lg rounded"
      />
    </div>
  );
}

// HTML preview (iframe sandbox)
function HtmlPreview({ html }: { html: string }) {
  const srcDoc = useMemo(() => {
    // Add base styles for better preview
    return `
      <style>
        body { font-family: system-ui, sans-serif; padding: 16px; margin: 0; }
        * { box-sizing: border-box; }
      </style>
      ${html}
    `;
  }, [html]);

  return (
    <iframe
      srcDoc={srcDoc}
      sandbox="allow-same-origin"
      className="w-full h-full min-h-[300px] bg-white rounded border"
      title="HTML Preview"
    />
  );
}

// Binary/hex preview
function BinaryPreview({ 
  body, 
}: { 
  body: Uint8Array; 
}) {
  const hexDump = useMemo(() => {
    const lines: string[] = [];
    const bytesPerLine = 16;
    
    for (let i = 0; i < Math.min(body.length, 1024); i += bytesPerLine) {
      const offset = i.toString(16).padStart(8, '0');
      const bytes: string[] = [];
      const chars: string[] = [];
      
      for (let j = 0; j < bytesPerLine; j++) {
        if (i + j < body.length) {
          bytes.push(body[i + j].toString(16).padStart(2, '0'));
          const char = body[i + j];
          chars.push(char >= 32 && char < 127 ? String.fromCharCode(char) : '.');
        } else {
          bytes.push('  ');
          chars.push(' ');
        }
      }
      
      lines.push(`${offset}  ${bytes.slice(0, 8).join(' ')}  ${bytes.slice(8).join(' ')}  |${chars.join('')}|`);
    }
    
    if (body.length > 1024) {
      lines.push(`... (${formatBytes(body.length - 1024)} more)`);
    }
    
    return lines.join('\n');
  }, [body]);

  return (
    <div className="flex-1 flex flex-col">
      <div className="px-3 py-2 flex items-center gap-2">
        <Image className="h-4 w-4 text-muted-foreground" />
        <span className="text-sm text-muted-foreground">
          Binary data ({formatBytes(body.length)})
        </span>
      </div>
      <pre className="flex-1 text-xs font-mono p-3 mx-3 mb-3 bg-muted/30 rounded overflow-auto">
        {hexDump}
      </pre>
    </div>
  );
}

/**
 * Trim text before syntax highlighting to avoid running regex replacers
 * across megabytes of input on the main thread.
 */
function truncateForHighlight(text: string): { slice: string; truncated: boolean } {
  if (text.length <= HIGHLIGHT_LIMIT) {
    return { slice: text, truncated: false };
  }
  return { slice: text.slice(0, HIGHLIGHT_LIMIT), truncated: true };
}

/** Tail marker shown at the bottom of a truncated preview. */
function TruncationMarker({ originalSize }: { originalSize: number }) {
  return (
    <span className="text-muted-foreground block mt-2 pt-2 border-t border-border/50">
      … truncated — full size {formatBytes(originalSize)}. Use the Download
      button above to save the complete body.
    </span>
  );
}

/**
 * Placeholder shown when the response body is too large to decode safely on
 * the main thread. Offers a one-click download and an escape hatch to force
 * render (at the user's own risk).
 */
function TooLargeNotice({
  size,
  onDownload,
  onForceRender,
}: {
  size: number;
  onDownload: () => void;
  onForceRender: () => void;
}) {
  return (
    <div className="flex-1 flex flex-col items-center justify-center p-8 text-center">
      <div className="inline-flex h-12 w-12 items-center justify-center rounded-xl bg-amber-500/10 mb-4">
        <AlertTriangle className="h-6 w-6 text-amber-500" />
      </div>
      <h3 className="font-medium text-sm mb-1">Response Too Large to Preview</h3>
      <p className="text-xs text-muted-foreground max-w-sm mb-4">
        This response is {formatBytes(size)}. Rendering it inline would freeze
        the UI, so the preview is disabled.
      </p>
      <div className="flex items-center gap-2">
        <Button size="sm" onClick={onDownload} className="gap-1.5">
          <Download className="h-3.5 w-3.5" />
          Download
        </Button>
        <Button size="sm" variant="outline" onClick={onForceRender}>
          Preview anyway
        </Button>
      </div>
    </div>
  );
}

function getExtension(contentType: string | null): string {
  if (!contentType) return 'bin';
  const normalized = contentType.toLowerCase();
  if (normalized.includes('json')) return 'json';
  if (normalized.includes('html')) return 'html';
  if (normalized.includes('xml')) return 'xml';
  if (normalized.includes('css')) return 'css';
  if (normalized.includes('javascript')) return 'js';
  if (normalized.includes('png')) return 'png';
  if (normalized.includes('jpeg') || normalized.includes('jpg')) return 'jpg';
  if (normalized.includes('gif')) return 'gif';
  if (normalized.includes('svg')) return 'svg';
  if (normalized.includes('text')) return 'txt';
  return 'bin';
}
