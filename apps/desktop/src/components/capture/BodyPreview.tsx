import { useMemo, useState } from 'react';
import { Copy, Download, Image, FileText, Code, Eye, EyeOff } from 'lucide-react';
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

export function BodyPreview({ body, contentType, loading, className }: BodyPreviewProps) {
  const [viewMode, setViewMode] = useState<ViewMode>('pretty');
  const [wordWrap, setWordWrap] = useState(true);

  const contentInfo = useMemo(() => {
    if (!body || body.length === 0) {
      return { type: 'empty', text: null, parsed: null };
    }

    const isImage = contentType?.startsWith('image/');
    const isJson = contentType?.includes('json');
    const isHtml = contentType?.includes('html');
    const isXml = contentType?.includes('xml');
    const isCss = contentType?.includes('css');
    const isJs = contentType?.includes('javascript') || contentType?.includes('ecmascript');
    const isText = contentType?.startsWith('text/') || isJson || isHtml || isXml || isCss || isJs;

    // Try to decode as text
    let text: string | null = null;
    let parsed: unknown = null;

    if (isText || !contentType) {
      try {
        text = new TextDecoder().decode(body);
        
        // Try to parse JSON
        if (isJson || (!contentType && text.trim().startsWith('{'))) {
          try {
            parsed = JSON.parse(text);
          } catch {
            // Not valid JSON
          }
        }
      } catch {
        // Binary content
      }
    }

    if (isImage) {
      return { type: 'image', text: null, parsed: null, mimeType: contentType };
    }

    if (parsed) {
      return { type: 'json', text, parsed };
    }

    if (isHtml) {
      return { type: 'html', text, parsed: null };
    }

    if (isXml) {
      return { type: 'xml', text, parsed: null };
    }

    if (isCss) {
      return { type: 'css', text, parsed: null };
    }

    if (isJs) {
      return { type: 'javascript', text, parsed: null };
    }

    if (text) {
      return { type: 'text', text, parsed: null };
    }

    return { type: 'binary', text: null, parsed: null };
  }, [body, contentType]);

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
      {contentInfo.type === 'image' ? (
        <ImagePreview body={body} mimeType={contentInfo.mimeType || undefined} />
      ) : contentInfo.type === 'binary' ? (
        <BinaryPreview body={body} />
      ) : (
        <Tabs value={viewMode} onValueChange={(v) => setViewMode(v as ViewMode)} className="flex-1 flex flex-col">
          <TabsList className="mx-3 mt-2 w-fit h-7">
            <TabsTrigger value="pretty" className="text-xs h-6 px-2">
              <Code className="h-3 w-3 mr-1" />
              Pretty
            </TabsTrigger>
            <TabsTrigger value="raw" className="text-xs h-6 px-2">
              <FileText className="h-3 w-3 mr-1" />
              Raw
            </TabsTrigger>
            {contentInfo.type === 'html' && (
              <TabsTrigger value="preview" className="text-xs h-6 px-2">
                <Eye className="h-3 w-3 mr-1" />
                Preview
              </TabsTrigger>
            )}
          </TabsList>

          <TabsContent value="pretty" className="flex-1 overflow-auto mt-2 mx-3 mb-3">
            {contentInfo.parsed ? (
              <JsonHighlight data={contentInfo.parsed} />
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

// JSON syntax highlighting component
function JsonHighlight({ data }: { data: unknown }) {
  const highlighted = useMemo(() => highlightJson(data, 0), [data]);
  
  return (
    <pre className="text-xs font-mono p-3 bg-muted/30 rounded overflow-auto">
      <code dangerouslySetInnerHTML={{ __html: highlighted }} />
    </pre>
  );
}

function highlightJson(value: unknown, indent: number): string {
  const spaces = '  '.repeat(indent);
  
  if (value === null) {
    return `<span class="text-orange-500">null</span>`;
  }
  
  if (typeof value === 'boolean') {
    return `<span class="text-orange-500">${value}</span>`;
  }
  
  if (typeof value === 'number') {
    return `<span class="text-emerald-500">${value}</span>`;
  }
  
  if (typeof value === 'string') {
    const escaped = escapeHtml(value);
    // Check if it looks like a URL
    if (value.match(/^https?:\/\//)) {
      return `<span class="text-sky-500">"${escaped}"</span>`;
    }
    return `<span class="text-amber-500">"${escaped}"</span>`;
  }
  
  if (Array.isArray(value)) {
    if (value.length === 0) {
      return '[]';
    }
    const items = value.map(item => `${spaces}  ${highlightJson(item, indent + 1)}`);
    return `[\n${items.join(',\n')}\n${spaces}]`;
  }
  
  if (typeof value === 'object') {
    const entries = Object.entries(value as Record<string, unknown>);
    if (entries.length === 0) {
      return '{}';
    }
    const items = entries.map(([key, val]) => {
      return `${spaces}  <span class="text-purple-500">"${escapeHtml(key)}"</span>: ${highlightJson(val, indent + 1)}`;
    });
    return `{\n${items.join(',\n')}\n${spaces}}`;
  }
  
  return String(value);
}

// HTML syntax highlighting
function HtmlHighlight({ code, wordWrap }: { code: string; wordWrap: boolean }) {
  const highlighted = useMemo(() => {
    return code
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      // Tags
      .replace(/(&lt;\/?)([\w-]+)/g, '$1<span class="text-rose-500">$2</span>')
      // Attributes
      .replace(/\s([\w-]+)=/g, ' <span class="text-amber-500">$1</span>=')
      // Strings
      .replace(/"([^"]*)"/g, '<span class="text-emerald-500">"$1"</span>')
      // Comments
      .replace(/(&lt;!--[\s\S]*?--&gt;)/g, '<span class="text-zinc-500">$1</span>');
  }, [code]);

  return (
    <pre className={cn(
      'text-xs font-mono p-3 bg-muted/30 rounded overflow-auto',
      wordWrap && 'whitespace-pre-wrap break-all'
    )}>
      <code dangerouslySetInnerHTML={{ __html: highlighted }} />
    </pre>
  );
}

// XML syntax highlighting (same as HTML)
function XmlHighlight({ code, wordWrap }: { code: string; wordWrap: boolean }) {
  return <HtmlHighlight code={code} wordWrap={wordWrap} />;
}

// CSS syntax highlighting
function CssHighlight({ code, wordWrap }: { code: string; wordWrap: boolean }) {
  const highlighted = useMemo(() => {
    return escapeHtml(code)
      // Selectors
      .replace(/([\w\-\.#\[\]="':,\s]+)\s*\{/g, '<span class="text-amber-500">$1</span>{')
      // Properties
      .replace(/([\w-]+)\s*:/g, '<span class="text-sky-500">$1</span>:')
      // Values with units
      .replace(/:\s*([^;{}]+)/g, ': <span class="text-emerald-500">$1</span>')
      // Comments
      .replace(/(\/\*[\s\S]*?\*\/)/g, '<span class="text-zinc-500">$1</span>');
  }, [code]);

  return (
    <pre className={cn(
      'text-xs font-mono p-3 bg-muted/30 rounded overflow-auto',
      wordWrap && 'whitespace-pre-wrap break-all'
    )}>
      <code dangerouslySetInnerHTML={{ __html: highlighted }} />
    </pre>
  );
}

// JavaScript syntax highlighting
function JsHighlight({ code, wordWrap }: { code: string; wordWrap: boolean }) {
  const highlighted = useMemo(() => {
    const keywords = ['const', 'let', 'var', 'function', 'return', 'if', 'else', 'for', 'while', 'class', 'extends', 'import', 'export', 'from', 'async', 'await', 'try', 'catch', 'throw', 'new', 'this', 'true', 'false', 'null', 'undefined'];
    
    let result = escapeHtml(code);
    
    // Keywords
    keywords.forEach(kw => {
      result = result.replace(new RegExp(`\\b(${kw})\\b`, 'g'), '<span class="text-purple-500">$1</span>');
    });
    
    // Strings
    result = result.replace(/(["'`])(?:(?!\1)[^\\]|\\.)*\1/g, '<span class="text-emerald-500">$&</span>');
    
    // Numbers
    result = result.replace(/\b(\d+\.?\d*)\b/g, '<span class="text-amber-500">$1</span>');
    
    // Comments
    result = result.replace(/(\/\/[^\n]*)/g, '<span class="text-zinc-500">$1</span>');
    result = result.replace(/(\/\*[\s\S]*?\*\/)/g, '<span class="text-zinc-500">$1</span>');

    return result;
  }, [code]);

  return (
    <pre className={cn(
      'text-xs font-mono p-3 bg-muted/30 rounded overflow-auto',
      wordWrap && 'whitespace-pre-wrap break-all'
    )}>
      <code dangerouslySetInnerHTML={{ __html: highlighted }} />
    </pre>
  );
}

// Plain text display
function PlainText({ text, wordWrap }: { text: string; wordWrap: boolean }) {
  const truncated = text.length > 100000;
  const displayText = truncated ? text.slice(0, 100000) : text;

  return (
    <pre className={cn(
      'text-xs font-mono p-3 bg-muted/30 rounded overflow-auto',
      wordWrap && 'whitespace-pre-wrap break-all'
    )}>
      {displayText}
      {truncated && (
        <span className="text-muted-foreground block mt-2">
          ... (truncated, showing first 100KB)
        </span>
      )}
    </pre>
  );
}

// Image preview
function ImagePreview({ body, mimeType }: { body: Uint8Array; mimeType?: string }) {
  const url = useMemo(() => {
    const blob = new Blob([new Uint8Array(body)], { type: mimeType || 'image/png' });
    return URL.createObjectURL(blob);
  }, [body, mimeType]);

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

// Helper functions
function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}

function getExtension(contentType: string | null): string {
  if (!contentType) return 'bin';
  if (contentType.includes('json')) return 'json';
  if (contentType.includes('html')) return 'html';
  if (contentType.includes('xml')) return 'xml';
  if (contentType.includes('css')) return 'css';
  if (contentType.includes('javascript')) return 'js';
  if (contentType.includes('png')) return 'png';
  if (contentType.includes('jpeg') || contentType.includes('jpg')) return 'jpg';
  if (contentType.includes('gif')) return 'gif';
  if (contentType.includes('svg')) return 'svg';
  if (contentType.includes('text')) return 'txt';
  return 'bin';
}
