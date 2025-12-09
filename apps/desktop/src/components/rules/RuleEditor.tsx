import { useEffect, useRef, useCallback } from 'react';
import Editor, { OnMount, OnChange, Monaco } from '@monaco-editor/react';
import type { editor, Position } from 'monaco-editor';
import { useThemeStore } from '@/stores/theme';
import { useRulesStore } from '@/stores/rules';
import {
  WHISTLE_LANGUAGE_ID,
  whistleLanguage,
  whistleLanguageConfig,
  whistleCompletions,
  whistleThemeRules,
} from '@/lib/editor/whistle-language';

interface RuleEditorProps {
  className?: string;
}

export function RuleEditor({ className }: RuleEditorProps) {
  const { theme } = useThemeStore();
  const { editorContent, setEditorContent, parseContent, parseResult } = useRulesStore();
  const editorRef = useRef<editor.IStandaloneCodeEditor | null>(null);
  const monacoRef = useRef<Monaco | null>(null);
  const parseTimeoutRef = useRef<NodeJS.Timeout | null>(null);

  // Register whistle language
  const handleEditorWillMount = useCallback((monaco: Monaco) => {
    // Register the language
    monaco.languages.register({ id: WHISTLE_LANGUAGE_ID });
    
    // Set the language configuration
    monaco.languages.setLanguageConfiguration(WHISTLE_LANGUAGE_ID, whistleLanguageConfig);
    
    // Set the tokenizer
    monaco.languages.setMonarchTokensProvider(WHISTLE_LANGUAGE_ID, whistleLanguage);
    
    // Register completion provider
    monaco.languages.registerCompletionItemProvider(WHISTLE_LANGUAGE_ID, {
      triggerCharacters: ['/', ':', '.'],
      provideCompletionItems: (model: editor.ITextModel, position: Position) => {
        const word = model.getWordUntilPosition(position);
        const range = {
          startLineNumber: position.lineNumber,
          endLineNumber: position.lineNumber,
          startColumn: word.startColumn,
          endColumn: word.endColumn,
        };
        
        return {
          suggestions: whistleCompletions.map(item => ({
            ...item,
            range,
          })),
        };
      },
    });
    
    // Define custom theme
    monaco.editor.defineTheme('whistle-dark', {
      base: 'vs-dark',
      inherit: true,
      rules: whistleThemeRules,
      colors: {
        'editor.background': '#09090b',
        'editor.foreground': '#fafafa',
        'editor.lineHighlightBackground': '#27272a',
        'editorLineNumber.foreground': '#71717a',
        'editorLineNumber.activeForeground': '#a1a1aa',
        'editor.selectionBackground': '#3f3f46',
        'editorCursor.foreground': '#fafafa',
      },
    });
    
    monaco.editor.defineTheme('whistle-light', {
      base: 'vs',
      inherit: true,
      rules: whistleThemeRules.map(rule => ({
        ...rule,
        // Adjust colors for light theme
        foreground: rule.foreground === '6A9955' ? '008000' : 
                   rule.foreground === 'CE9178' ? 'A31515' :
                   rule.foreground === '569CD6' ? '0000FF' :
                   rule.foreground === 'B5CEA8' ? '098658' :
                   rule.foreground,
      })),
      colors: {
        'editor.background': '#ffffff',
        'editor.foreground': '#09090b',
        'editor.lineHighlightBackground': '#f4f4f5',
        'editorLineNumber.foreground': '#a1a1aa',
        'editorLineNumber.activeForeground': '#52525b',
        'editor.selectionBackground': '#add6ff',
        'editorCursor.foreground': '#09090b',
      },
    });
  }, []);

  // Handle editor mount
  const handleEditorDidMount: OnMount = useCallback((editor, monaco) => {
    editorRef.current = editor;
    monacoRef.current = monaco;
    
    // Set initial decorations based on parse result
    if (parseResult) {
      updateDecorations(editor, monaco, parseResult.errors);
    }
  }, [parseResult]);

  // Update decorations for errors
  const updateDecorations = useCallback((
    editor: editor.IStandaloneCodeEditor,
    monaco: Monaco,
    errors: { line: number; message: string }[]
  ) => {
    const decorations = errors.map(error => ({
      range: new monaco.Range(error.line, 1, error.line, 1),
      options: {
        isWholeLine: true,
        className: 'bg-red-500/10',
        glyphMarginClassName: 'bg-red-500 rounded-full',
        glyphMarginHoverMessage: { value: error.message },
        overviewRuler: {
          color: '#ef4444',
          position: monaco.editor.OverviewRulerLane.Right,
        },
      },
    }));
    
    editor.createDecorationsCollection(decorations);
  }, []);

  // Handle content change with debounced parsing
  const handleEditorChange: OnChange = useCallback((value) => {
    if (value !== undefined) {
      setEditorContent(value);
      
      // Debounce parsing
      if (parseTimeoutRef.current) {
        clearTimeout(parseTimeoutRef.current);
      }
      
      parseTimeoutRef.current = setTimeout(() => {
        parseContent(value);
      }, 500);
    }
  }, [setEditorContent, parseContent]);

  // Update decorations when parse result changes
  useEffect(() => {
    if (editorRef.current && monacoRef.current && parseResult) {
      updateDecorations(editorRef.current, monacoRef.current, parseResult.errors);
    }
  }, [parseResult, updateDecorations]);

  // Cleanup timeout on unmount
  useEffect(() => {
    return () => {
      if (parseTimeoutRef.current) {
        clearTimeout(parseTimeoutRef.current);
      }
    };
  }, []);

  const editorTheme = theme === 'dark' || (theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches)
    ? 'whistle-dark'
    : 'whistle-light';

  return (
    <div className={className}>
      <Editor
        height="100%"
        language={WHISTLE_LANGUAGE_ID}
        theme={editorTheme}
        value={editorContent}
        onChange={handleEditorChange}
        beforeMount={handleEditorWillMount}
        onMount={handleEditorDidMount}
        options={{
          fontSize: 13,
          fontFamily: 'JetBrains Mono, Menlo, Monaco, Consolas, monospace',
          lineNumbers: 'on',
          minimap: { enabled: false },
          scrollBeyondLastLine: false,
          wordWrap: 'on',
          wrappingIndent: 'indent',
          automaticLayout: true,
          tabSize: 2,
          insertSpaces: true,
          glyphMargin: true,
          folding: true,
          foldingStrategy: 'indentation',
          lineDecorationsWidth: 10,
          renderLineHighlight: 'line',
          scrollbar: {
            verticalScrollbarSize: 10,
            horizontalScrollbarSize: 10,
          },
          padding: {
            top: 8,
            bottom: 8,
          },
          suggest: {
            showKeywords: true,
            showSnippets: true,
          },
        }}
      />
    </div>
  );
}
